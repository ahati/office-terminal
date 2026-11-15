//! Target-aware pipeline entry tests for slide-sorter thumbnails.
//!
//! The sorter asks for card-sized rasters while the canvas keeps its own
//! full-resolution target, so `entry_at`/`prefetch_thumbnails` must not leak
//! one class of raster into the other.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use kitty_pptx_core::contracts::{PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::pipeline::DefaultRenderPipeline;
use kitty_pptx_core::{AssetError, DefaultSlideCompiler, TinySkiaRasterizer};

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

const CARD: RenderTargetSpec = RenderTargetSpec {
    width_px: 320,
    height_px: 180,
};

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

    match pipeline.entry_at(0, &CARD) {
        PipelineEntry::NotLoaded | PipelineEntry::Rendering => {}
        _ => panic!("nothing is cached for slide 0 yet"),
    }

    let bitmap = pipeline.render_sync(0, &CARD).expect("card render");
    assert_eq!((bitmap.width, bitmap.height), (320, 180));

    match pipeline.entry_at(0, &CARD) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!((bitmap.width, bitmap.height), (320, 180));
        }
        _ => panic!("entry_at must serve the card raster it just cached"),
    }
}

#[test]
fn entry_at_never_falls_back_to_another_target() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &CANVAS).expect("canvas render");

    // The canvas raster is cached, but a card must be rendered at its own size
    // instead of silently shipping an 8 MB bitmap for a 320 px card.
    match pipeline.entry_at(0, &CARD) {
        PipelineEntry::NotLoaded | PipelineEntry::Rendering => {}
        other => panic!(
            "entry_at must not reuse the canvas raster, got {}",
            entry_kind(&other)
        ),
    }
}

#[test]
fn entry_ignores_card_sized_rasters() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &CARD).expect("card render");
    // Move the pipeline's main target to the canvas without rendering slide 0.
    pipeline.prefetch(&focus_on(1), &CANVAS);

    match pipeline.entry(0) {
        PipelineEntry::NotLoaded | PipelineEntry::Rendering => {}
        other => panic!(
            "a card-sized raster must not be upscaled onto the canvas, got {}",
            entry_kind(&other)
        ),
    }
}

#[test]
fn entry_still_accepts_a_larger_cached_raster() {
    let pipeline = pipeline();
    pipeline.render_sync(0, &CANVAS).expect("canvas render");
    // Zoom out: the canvas is now requested at half the cached resolution.
    let half = RenderTargetSpec {
        width_px: 960,
        height_px: 540,
    };
    pipeline.prefetch(&focus_on(1), &half);

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
fn prefetch_thumbnails_renders_at_the_card_target() {
    let pipeline = pipeline();
    pipeline.prefetch_thumbnails(&[0, 1], &CARD);

    let mut ready = false;
    for _ in 0..100 {
        if matches!(pipeline.entry_at(0, &CARD), PipelineEntry::Ready { .. })
            && matches!(pipeline.entry_at(1, &CARD), PipelineEntry::Ready { .. })
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(ready, "thumbnails become ready from the worker pool");
    assert!(pipeline.cache().contains_raster(0, &CARD));
    assert!(
        !pipeline.cache().contains_raster(0, &CANVAS),
        "thumbnail prefetch must not render the canvas target"
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
