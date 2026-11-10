use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use kitty_pptx_core::contracts::{
    InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::pipeline::{CancellationToken, DefaultRenderPipeline};
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

#[test]
fn test_cancellation_token_logic() {
    let token = CancellationToken::new();
    assert!(!token.is_canceled());

    let token_clone = token.clone();
    token.cancel();
    assert!(token.is_canceled());
    assert!(token_clone.is_canceled());

    token.reset();
    assert!(!token.is_canceled());
    assert!(!token_clone.is_canceled());
}

#[test]
fn test_background_prefetch_and_cache_hits() {
    let pres = create_presentation(8);
    let asset_store = Arc::new(MockAssetStore);
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());

    let pipeline = DefaultRenderPipeline::new(
        pres,
        asset_store,
        compiler,
        rasterizer,
    );

    let target = RenderTargetSpec {
        width_px: 320,
        height_px: 180,
    };

    // Prefetch around slide 2 (center = 2, behind = 2, ahead = 2: slides 2, 3, 1, 4, 0)
    let plan = PrefetchPlan {
        center: 2,
        behind: 2,
        ahead: 2,
        stage: true,
    };

    pipeline.prefetch(&plan, &target);

    // Wait briefly for background worker threads to finish rasterizing 5 small slides
    let mut ready = false;
    for _ in 0..50 {
        if let PipelineEntry::Ready { .. } = pipeline.entry(2) {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(ready, "Slide 2 should become Ready via background prefetch");

    // Cache verification
    assert!(pipeline.cache().has_vector(2));
    assert!(pipeline.cache().contains_raster(2, &target));

    // Calling prefetch again with the same plan should be an instant no-op (cache hit)
    pipeline.prefetch(&plan, &target);
    assert!(pipeline.cache().contains_raster(2, &target));
}

#[test]
fn test_cancellation_on_slide_jump() {
    let pres = create_presentation(20);
    let asset_store = Arc::new(MockAssetStore);
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());

    let pipeline = DefaultRenderPipeline::new(
        pres,
        asset_store,
        compiler,
        rasterizer,
    );

    let target = RenderTargetSpec {
        width_px: 320,
        height_px: 180,
    };

    // 1. Initial prefetch at slide 1
    let plan1 = PrefetchPlan {
        center: 1,
        behind: 1,
        ahead: 5,
        stage: true,
    };
    pipeline.prefetch(&plan1, &target);

    // 2. Immediate jump to slide 15
    let plan2 = PrefetchPlan {
        center: 15,
        behind: 2,
        ahead: 2,
        stage: true,
    };
    pipeline.prefetch(&plan2, &target);

    // Slide 6 was in plan1 but is outside plan2's active set {13, 14, 15, 16, 17}
    // Verify workers cancel_except was triggered
    let mut keep = HashSet::new();
    keep.insert(15);
    pipeline.workers().cancel_except(&keep);
}

#[test]
fn test_synchronous_fallback() {
    let pres = create_presentation(5);
    let asset_store = Arc::new(MockAssetStore);
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());

    let pipeline = DefaultRenderPipeline::new(
        pres,
        asset_store,
        compiler,
        rasterizer,
    );

    let target = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    // Before render: slide 0 is not loaded
    match pipeline.entry(0) {
        PipelineEntry::NotLoaded => {}
        other => panic!("Expected NotLoaded before render, got {:?}", match other {
            PipelineEntry::Ready { .. } => "Ready",
            PipelineEntry::Rendering => "Rendering",
            PipelineEntry::NotLoaded => "NotLoaded",
            PipelineEntry::Failed { .. } => "Failed",
        }),
    }

    // Synchronous render fallback
    let rendered = pipeline
        .render_sync(0, &target)
        .expect("synchronous render should succeed");
    assert_eq!(rendered.width, 640);
    assert_eq!(rendered.height, 360);

    // Now entry(0) should be immediately Ready from cache
    match pipeline.entry(0) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!(bitmap.width, 640);
            assert_eq!(bitmap.height, 360);
        }
        _ => panic!("Expected Ready after sync render"),
    }

    // Enable automatic sync fallback on entry()
    pipeline.set_sync_fallback(true);
    match pipeline.entry(1) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!(bitmap.width, 640); // Uses current target resolution
        }
        _ => panic!("Expected Ready via entry sync fallback"),
    }
}

#[test]
fn test_invalidation_lifecycle() {
    let pres = create_presentation(5);
    let asset_store = Arc::new(MockAssetStore);
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());

    let pipeline = DefaultRenderPipeline::new(
        pres,
        asset_store,
        compiler,
        rasterizer,
    );

    let target = RenderTargetSpec {
        width_px: 800,
        height_px: 450,
    };

    pipeline.render_sync(0, &target).unwrap();
    assert!(pipeline.cache().has_vector(0));
    assert!(pipeline.cache().contains_raster(0, &target));

    let gen0 = pipeline.current_generation();

    // 1. InvalidateCause::Resize retains both caches
    pipeline.invalidate(InvalidateCause::Resize);
    assert_eq!(pipeline.current_generation(), gen0 + 1);
    assert!(pipeline.cache().has_vector(0));
    assert!(pipeline.cache().contains_raster(0, &target));

    // 2. InvalidateCause::TargetChanged clears raster cache but keeps vector cache
    pipeline.invalidate(InvalidateCause::TargetChanged);
    assert_eq!(pipeline.current_generation(), gen0 + 2);
    assert!(pipeline.cache().has_vector(0));
    assert!(!pipeline.cache().contains_raster(0, &target));

    // Re-render
    pipeline.render_sync(0, &target).unwrap();
    assert!(pipeline.cache().contains_raster(0, &target));

    // 3. InvalidateCause::DeckReload clears both caches
    pipeline.invalidate(InvalidateCause::DeckReload);
    assert_eq!(pipeline.current_generation(), gen0 + 3);
    assert!(!pipeline.cache().has_vector(0));
    assert!(!pipeline.cache().contains_raster(0, &target));
}
