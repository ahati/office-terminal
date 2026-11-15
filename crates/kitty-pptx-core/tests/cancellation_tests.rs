//! Worker-pool cancellation tests.
//!
//! `cancel_except` used to drain canceled tasks from the queue without
//! releasing their `in_flight` slots. `is_in_flight` gates scheduling, so a
//! leaked slot meant `prefetch` would never render that slide again — a
//! permanently blank canvas or sorter card.

use std::collections::HashSet;
use std::sync::Arc;

use kitty_pptx_core::contracts::{RenderTargetSpec, WarningEvent};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::pipeline::{PipelineCache, WorkerPool};
use kitty_pptx_core::{AssetError, DefaultSlideCompiler, TinySkiaRasterizer};

struct NullStore;

impl AssetStore for NullStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("null store".into()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn presentation(slides: usize) -> Arc<pptx_model::Presentation> {
    let slides = (0..slides)
        .map(|i| pptx_model::Slide {
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
        })
        .collect();
    Arc::new(pptx_model::Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides,
        default_text_color: None,
        major_font: None,
        minor_font: None,
        hlink_color: None,
        fol_hlink_color: None,
    })
}

/// A pool with no worker threads: every scheduled task stays queued, which
/// makes the cancellation paths deterministic.
fn queued_pool() -> WorkerPool {
    let (_warn_tx, _warn_rx) = crossbeam_channel::unbounded::<WarningEvent>();
    WorkerPool::new(
        0,
        presentation(6),
        Arc::new(NullStore),
        Arc::new(DefaultSlideCompiler::new()),
        Arc::new(TinySkiaRasterizer::new()),
        PipelineCache::new(),
    )
}

fn target() -> RenderTargetSpec {
    RenderTargetSpec {
        width_px: 800,
        height_px: 450,
    }
}

#[test]
fn canceled_queued_tasks_release_their_in_flight_slots() {
    let pool = queued_pool();
    for slide in 0..3 {
        pool.schedule(slide, 100 - slide as u32, target());
    }
    assert_eq!(pool.queue_len(), 3);
    assert_eq!(pool.in_flight_count(), 3);

    // The sorter scrolled to a page that no longer contains these slides.
    let keep: HashSet<usize> = HashSet::from([3]);
    pool.cancel_except(&keep);

    assert_eq!(pool.queue_len(), 0, "canceled tasks leave the queue");
    assert_eq!(
        pool.in_flight_count(),
        0,
        "canceled queued tasks must not stay in flight, or prefetch never retries them"
    );
    for slide in 0..3 {
        assert!(!pool.is_in_flight(slide), "slide {slide} must be retryable");
    }
}

#[test]
fn kept_tasks_stay_in_flight_through_cancellation() {
    let pool = queued_pool();
    pool.schedule(0, 100, target());
    pool.schedule(1, 99, target());

    pool.cancel_except(&HashSet::from([1]));

    assert!(!pool.is_in_flight(0), "canceled task released");
    assert!(pool.is_in_flight(1), "visible slide keeps its slot");
    assert_eq!(pool.queue_len(), 1);
}

#[test]
fn canceled_slides_can_be_scheduled_again() {
    let pool = queued_pool();
    pool.schedule(0, 100, target());
    pool.cancel_except(&HashSet::new());
    assert!(!pool.is_in_flight(0));

    // Scrolling back to the slide must queue it again.
    pool.schedule(0, 100, target());
    assert!(pool.is_in_flight(0));
    assert_eq!(pool.queue_len(), 1);
}
