//! Tests for the bounded first-frame synchronous paint (`paint_first_frame`)
//! and its readiness decision (`first_frame_needs_sync`).
//!
//! Interactive pipelines run with sync fallback off, so the run loop's
//! one-shot first paint is the only synchronous render the UI thread ever
//! issues. These tests pin its contract: exactly one render on a cold
//! pipeline, zero when the entry is already `Ready`, and the reader-mode
//! target discipline.

use std::sync::{Arc, Mutex};

use office_terminal_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderError, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use office_terminal_display::{DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver};

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
    /// Pages handed to `prefetch_window` (reader first-window warm-up).
    window_pages: Mutex<Vec<usize>>,
}

impl CountingPipeline {
    fn new(state: EntryState) -> Self {
        Self {
            state,
            renders: Mutex::new(Vec::new()),
            window_pages: Mutex::new(Vec::new()),
        }
    }

    fn entry(&self) -> PipelineEntry {
        match self.state {
            EntryState::NotLoaded => PipelineEntry::NotLoaded,
            EntryState::Rendering => PipelineEntry::Rendering,
            EntryState::Ready => PipelineEntry::Ready { bitmap: bitmap(4, 2) },
        }
    }

    fn window_pages(&self) -> Vec<usize> {
        self.window_pages.lock().expect("window log lock").clone()
    }
}

impl RenderPipeline for CountingPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}

    fn prefetch_window(&self, plan: &PrefetchPlan, targets: &[(usize, RenderTargetSpec)]) {
        let mut log = self.window_pages.lock().expect("window log lock");
        log.push(plan.center);
        log.extend(targets.iter().map(|(idx, _)| *idx));
    }
    fn entry(&self, _index: usize) -> PipelineEntry {
        self.entry()
    }
    fn entry_at(&self, _index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        self.entry()
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
    fn render(&self, index: usize, target: &RenderTargetSpec) -> Result<RgbaSlide, RenderError> {
        self.renders.lock().expect("render log lock").push((index, *target));
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
    PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 3, vec![]).with_slide_dimensions(10_000_000, 7_500_000)
    // 4:3
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
        !first_frame_needs_sync(&PipelineEntry::Ready { bitmap: bitmap(4, 2) }),
        "a ready entry stages directly, no render needed"
    );
}

#[test]
fn cold_pipeline_paints_first_frame_exactly_once() {
    let pipeline = Arc::new(CountingPipeline::new(EntryState::NotLoaded));
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
    let pipeline = Arc::new(CountingPipeline::new(EntryState::Ready));
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
    let pipeline = Arc::new(CountingPipeline::new(EntryState::Rendering));
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
    let pipeline = Arc::new(CountingPipeline::new(EntryState::NotLoaded));
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(&pipeline) as Arc<dyn RenderPipeline>;
    // Reader mode from page sizes in points (4:3), mirroring how the CLI
    // builder wires `with_reader_mode` after `with_slide_dimensions`.
    let mut app = PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 3, vec![]).with_reader_mode(
        true,
        3,
        vec![(800.0, 600.0); 3],
    );

    app.handle_resize(80, 24);
    app.paint_first_frame();

    let renders = render_log(&pipeline);
    assert_eq!(renders.len(), 1, "reader decks get the same first paint");
    assert_eq!(renders[0].0, 0, "current_slide mirrors the focused page");
    // 80x24 grid -> ~390-613 px pane -> the 1280 px raster class, not the old
    // fixed 2560 px: the first paint must match the class the pane layout
    // will look up or the entry misses and re-renders.
    let reader = app.state().reader_state().expect("reader state enabled");
    let (vw, vh, cw, _) = crate::app::reader_types::reader_viewport_metrics(80, 24);
    let expected = reader.page_render_target(0, reader.pane_width_px(0, vw, vh, cw));
    assert_eq!(expected.width_px, 1280, "a small pane snaps to the smallest class");
    assert_eq!(
        renders[0].1, expected,
        "uniform deck: the painted target is exactly the reader pane target"
    );
}

/// The reader first paint must warm the following pages on the worker pool
/// *before* the synchronous anchor render: an immediate scroller reaches the
/// first page boundary while the UI thread is still busy, and without the
/// warm-up the next page's raster only starts after that paint — the peek
/// pane stays blank until the anchor flips.
#[test]
fn reader_first_paint_warms_the_next_pages() {
    let pipeline = Arc::new(CountingPipeline::new(EntryState::NotLoaded));
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(&pipeline) as Arc<dyn RenderPipeline>;
    let mut app = PresentationApp::new(dyn_pipeline, Box::new(NullDriver), 4, vec![]).with_reader_mode(
        true,
        4,
        vec![(612.0, 792.0); 4],
    );

    // Same sequence as run(): resize first (establishes the target and the
    // layout size the warm-up reads), then the paint under test.
    app.handle_resize(120, 45);
    app.paint_first_frame();

    // The warm-up schedules every page after the anchor at the pane targets
    // the peek layout will look up.
    let warm = pipeline.window_pages();
    assert!(
        warm.contains(&0),
        "warm-up plan centers on the anchor page, got {warm:?}"
    );
    assert_eq!(
        warm.iter().filter(|&&p| p == 1).count(),
        1,
        "page 2 (index 1) must be scheduled for raster while the first frame paints"
    );
    assert!(warm.contains(&2) && warm.contains(&3), "whole window warmed: {warm:?}");

    // The synchronous anchor paint is unchanged.
    let renders = render_log(&pipeline);
    assert_eq!(renders.len(), 1);
    assert_eq!(renders[0].0, 0);
}

/// Non-reader decks keep the old contract: no reader window warm-up.
#[test]
fn slide_deck_first_paint_does_not_warm_a_reader_window() {
    let pipeline = Arc::new(CountingPipeline::new(EntryState::NotLoaded));
    let mut app = app_with(&pipeline);

    app.handle_resize(80, 24);
    app.paint_first_frame();

    assert!(
        pipeline.window_pages().is_empty(),
        "slide decks never issue a reader window warm-up"
    );
    assert_eq!(render_log(&pipeline).len(), 1);
}
