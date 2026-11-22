//! Target-aware pipeline entry tests.
//!
//! The sorter grid reuses the canvas raster (`entry_at(slide, canvas_target)`)
//! and lets the terminal scale it into the card cells, so explicit-target
//! lookups must be exact while `entry()` may fall back to another cached
//! resolution (e.g. right after a resize) instead of leaving the canvas blank.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use office_terminal_core::contracts::{PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::pipeline::DefaultRenderPipeline;
use office_terminal_core::{AssetError, DefaultSlideCompiler, TinySkiaRasterizer};

struct MockAssetStore;

impl AssetStore for MockAssetStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("mock store".into()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn create_presentation(num_slides: usize) -> Arc<pptx_model::Presentation> {
    let mut slides = Vec::with_capacity(num_slides);
    for i in 0..num_slides {
        slides.push(pptx_model::Slide {
            theme: None,
            index: i,
            slide_number: i + 1,
            part_name: Some(format!("ppt/slides/slide{}.xml", i + 1)),
            background: Some(pptx_model::Fill::Solid {
                color: "FFFFFF".into(),
            }),
            elements: Vec::new(),
            element_sources: Vec::new(),
            notes: None,
            comments: Vec::new(),
            hidden: false,
            parse_error: None,
        });
    }

    Arc::new(pptx_model::Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides,
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    })
}

fn pipeline() -> DefaultRenderPipeline {
    DefaultRenderPipeline::with_worker_count(
        create_presentation(4),
        Arc::new(MockAssetStore),
        Arc::new(DefaultSlideCompiler::new()),
        Arc::new(TinySkiaRasterizer::new()),
        2,
    )
}

/// A small raster (as an export or a legacy card target would produce).
const SMALL: RenderTargetSpec = RenderTargetSpec {
    width_px: 320,
    height_px: 180,
};

/// A mid-size raster, e.g. the canvas before a zoom-in.
const MID: RenderTargetSpec = RenderTargetSpec {
    width_px: 960,
    height_px: 540,
};

/// The canvas target the sorter shares its bitmaps with.
const CANVAS: RenderTargetSpec = RenderTargetSpec {
    width_px: 1920,
    height_px: 1080,
};

/// Centers a prefetch window on `slide` so other slides are not rendered.
fn focus_on(slide: usize) -> PrefetchPlan {
    PrefetchPlan {
        center: slide,
        behind: 0,
        ahead: 0,
        stage: false,
    }
}

#[test]
fn entry_at_renders_and_caches_the_requested_target() {
    let pipeline = pipeline();

    match pipeline.entry_at(0, &CANVAS) {
        PipelineEntry::NotLoaded | PipelineEntry::Rendering => {}
        _ => panic!("nothing is cached for slide 0 yet"),
    }

    let bitmap = pipeline.render_sync(0, &CANVAS).expect("canvas render");
    assert_eq!((bitmap.width, bitmap.height), (1920, 1080));

    match pipeline.entry_at(0, &CANVAS) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!((bitmap.width, bitmap.height), (1920, 1080));
        }
        _ => panic!("entry_at must serve the raster it just cached"),
    }
}

#[test]
fn entry_at_is_exact_target_only() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &CANVAS).expect("canvas render");

    // A sorter card asks for the canvas target; a mismatch must not silently
    // hand back a differently-sized bitmap, because the terminal would scale
    // it into the card as if it were the canvas frame.
    match pipeline.entry_at(0, &SMALL) {
        PipelineEntry::NotLoaded | PipelineEntry::Rendering => {}
        other => panic!(
            "entry_at must not reuse another target, got {}",
            entry_kind(&other)
        ),
    }
}

#[test]
fn entry_falls_back_to_the_largest_cached_resolution() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &SMALL).expect("small render");
    pipeline.render_sync(0, &MID).expect("mid render");
    // Move the canvas target without rendering slide 0 at it.
    pipeline.prefetch(&focus_on(1), &CANVAS);

    match pipeline.entry(0) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!(
                (bitmap.width, bitmap.height),
                (960, 540),
                "the sharpest cached raster backs the canvas, not an arbitrary one"
            );
        }
        other => panic!("expected a fallback raster, got {}", entry_kind(&other)),
    }
}

#[test]
fn entry_still_accepts_a_larger_cached_raster() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &CANVAS).expect("canvas render");
    // Zoom out: the canvas is now requested at half the cached resolution.
    pipeline.prefetch(&focus_on(1), &MID);

    match pipeline.entry(0) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!(
                (bitmap.width, bitmap.height),
                (1920, 1080),
                "the sharpest available raster backs the zoomed-out canvas"
            );
        }
        other => panic!("expected the larger raster, got {}", entry_kind(&other)),
    }
}

#[test]
fn prefetch_warms_the_canvas_target_for_the_sorter() {
    let pipeline = pipeline();
    // The sorter prefetches its visible page at the canvas target.
    pipeline.prefetch(
        &PrefetchPlan {
            center: 0,
            behind: 0,
            ahead: 2,
            stage: false,
        },
        &CANVAS,
    );

    let mut ready = false;
    for _ in 0..100 {
        if matches!(pipeline.entry_at(0, &CANVAS), PipelineEntry::Ready { .. })
            && matches!(pipeline.entry_at(1, &CANVAS), PipelineEntry::Ready { .. })
            && matches!(pipeline.entry_at(2, &CANVAS), PipelineEntry::Ready { .. })
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(ready, "the visible page renders on the workers");
    assert!(pipeline.cache().contains_raster(0, &CANVAS));
    assert!(
        !pipeline.cache().contains_raster(0, &SMALL),
        "the sorter does not render a second, card-sized raster"
    );
}

fn entry_kind(entry: &PipelineEntry) -> &'static str {
    match entry {
        PipelineEntry::Ready { .. } => "Ready",
        PipelineEntry::Rendering => "Rendering",
        PipelineEntry::NotLoaded => "NotLoaded",
        PipelineEntry::Failed { .. } => "Failed",
    }
}
