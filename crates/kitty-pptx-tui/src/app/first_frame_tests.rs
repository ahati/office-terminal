//! Tests for the bounded first-frame synchronous paint (`paint_first_frame`)
//! and its readiness decision (`first_frame_needs_sync`).
//!
//! Interactive pipelines run with sync fallback off, so the run loop's
//! one-shot first paint is the only synchronous render the UI thread ever
//! issues. These tests pin its contract: exactly one render on a cold
//! pipeline, zero when the entry is already `Ready`, and the reader-mode
//! target discipline.

use std::sync::{Arc, Mutex};

use kitty_pptx_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderError, RenderPipeline,
    RenderTargetSpec, RgbaSlide,
};
use kitty_pptx_display::{
    DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver,
};

use super::{first_frame_needs_sync, PresentationApp};

fn bitmap(w: u32, h: u32) -> RgbaSlide {
    RgbaSlide {
        width: w,
        height: h,
        pixels: vec![0u8; (w * h * 4) as usize].into(),
    }
}

/// What `entry_at` reports for the initial page before the first paint.
enum EntryState {
    NotLoaded,
    Rendering,
    Ready,
}

/// Pipeline that counts direct `render` calls and answers lookups from a
/// fixed entry state, standing in for a cold / busy / warm pipeline.
struct CountingPipeline {
    state: EntryState,
    renders: Mutex<Vec<(usize, RenderTargetSpec)>>,
}

impl CountingPipeline {
    fn entry(&self) -> PipelineEntry {
        match self.state {
            EntryState::NotLoaded => PipelineEntry::NotLoaded,
            EntryState::Rendering => PipelineEntry::Rendering,
            EntryState::Ready => PipelineEntry::Ready {
                bitmap: bitmap(4, 2),
            },
        }
    }
}

impl RenderPipeline for CountingPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, _index: usize) -> PipelineEntry {
        self.entry()
    }
    fn entry_at(&self, _index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        self.entry()
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
    fn render(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, RenderError> {
        self.renders
            .lock()
            .expect("render log lock")
            .push((index, *target));
        Ok(bitmap(4, 2))
    }
}

/// Minimal driver: the first-frame paint never touches it.
struct NullDriver;

impl TerminalGraphicsDriver for NullDriver {
    fn name(&self) -> &'static str {
        "null"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: false,
            resident_ids: 8,
        }
    }
    fn placement_mode(&self) -> PlacementMode {
        PlacementMode::Interactive
    }
    fn stage(&mut self, _bmp: &RgbaSlide, _id: ImageId) -> Result<(), DisplayError> {
        Ok(())
    }
    fn present(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        Ok(())
    }
    fn reposition(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        Ok(())
    }
    fn release(&mut self, _id: ImageId) -> Result<(), DisplayError> {
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        Ok(())
    }
}

/// App wired exactly like `run()` does: `handle_resize` first (computes the
/// target and prefetches), then the first-frame paint under test.
fn app_with(pipeline: &Arc<CountingPipeline>) -> PresentationApp {
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(pipeline) as Arc<dyn RenderPipeline>;
    PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 3, vec![])
        .with_slide_dimensions(10_000_000, 7_500_000) // 4:3
}

fn render_log(pipeline: &CountingPipeline) -> Vec<(usize, RenderTargetSpec)> {
    pipeline.renders.lock().expect("render log lock").clone()
}

#[test]
fn decision_helper_syncs_for_everything_but_ready() {
    assert!(
        first_frame_needs_sync(&PipelineEntry::NotLoaded),
        "cold cache needs the synchronous paint"
    );
    assert!(
        first_frame_needs_sync(&PipelineEntry::Rendering),
        "a mid-flight worker must not leave the deck opening blank"
    );
    assert!(
        first_frame_needs_sync(&PipelineEntry::Failed {
            error: RenderError::RasterizationError("boom".to_string()),
        }),
        "a latched failure still gets one synchronous retry"
    );
    assert!(
        !first_frame_needs_sync(&PipelineEntry::Ready {
            bitmap: bitmap(4, 2),
        }),
        "a ready entry stages directly, no render needed"
    );
}

#[test]
fn cold_pipeline_paints_first_frame_exactly_once() {
    let pipeline = Arc::new(CountingPipeline {
        state: EntryState::NotLoaded,
        renders: Mutex::new(Vec::new()),
    });
    let mut app = app_with(&pipeline);

    // Same sequence as run(): resize establishes the target, then the paint.
    app.handle_resize(80, 24);
    app.paint_first_frame();

    let renders = render_log(&pipeline);
    assert_eq!(renders.len(), 1, "cold start: exactly one synchronous render");
    assert_eq!(renders[0].0, 0, "renders the initial slide");
    assert_eq!(
        renders[0].1, app.current_target,
        "renders at the target handle_resize just computed"
    );
}

#[test]
fn warm_pipeline_skips_the_first_frame_sync_render() {
    let pipeline = Arc::new(CountingPipeline {
        state: EntryState::Ready,
        renders: Mutex::new(Vec::new()),
    });
    let mut app = app_with(&pipeline);

    app.handle_resize(80, 24);
    app.paint_first_frame();

    assert!(
        render_log(&pipeline).is_empty(),
        "entry Ready (cache hit or worker won the race): no synchronous render"
    );
}

/// A pipeline still rasterizing the initial slide (worker won the race)
/// must not leave the deck opening blank: the first paint duplicates the
/// work idempotently and stages as soon as it lands.
#[test]
fn midflight_pipeline_still_paints_first_frame() {
    let pipeline = Arc::new(CountingPipeline {
        state: EntryState::Rendering,
        renders: Mutex::new(Vec::new()),
    });
    let mut app = app_with(&pipeline);

    app.handle_resize(80, 24);
    app.paint_first_frame();

    let renders = render_log(&pipeline);
    assert_eq!(renders.len(), 1, "Rendering: the sync paint runs anyway");
    assert_eq!(renders[0].0, 0);
    assert_eq!(renders[0].1, app.current_target);
}

#[test]
fn reader_deck_paints_first_frame_at_the_reader_target() {
    let pipeline = Arc::new(CountingPipeline {
        state: EntryState::NotLoaded,
        renders: Mutex::new(Vec::new()),
    });
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(&pipeline) as Arc<dyn RenderPipeline>;
    // Reader mode from page sizes in points (4:3), mirroring how the CLI
    // builder wires `with_reader_mode` after `with_slide_dimensions`.
    let mut app = PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 3, vec![])
        .with_reader_mode(true, 3, vec![(800.0, 600.0); 3]);

    app.handle_resize(80, 24);
    app.paint_first_frame();

    let renders = render_log(&pipeline);
    assert_eq!(renders.len(), 1, "reader decks get the same first paint");
    assert_eq!(renders[0].0, 0, "current_slide mirrors the focused page");
    assert_eq!(renders[0].1.width_px, 2560, "the fixed 2560-px-wide reader target");
    assert_eq!(
        renders[0].1,
        app.state()
            .reader_state()
            .expect("reader state enabled")
            .page_render_target(0),
        "uniform deck: the painted target is exactly the reader pane target"
    );
}
