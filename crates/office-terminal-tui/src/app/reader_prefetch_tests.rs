//! Reader prefetch-target planning tests.
//!
//! The prefetch window must size its targets with the column plan the
//! renderer will lay out with ([`ReaderState::planned_columns`]) — not the
//! stored effective count, which converges to the plan one frame later. On
//! tall viewports the renderer plans one column while the stored count
//! still says two, and a target from the stale plan never matches the
//! placement lookup: the peek pane stays `NotLoaded` (blank) forever.
//!
//! The syncer must also re-fire prefetching while a visible pane's exact
//! target raster is unscheduled: `prefetch_window` skips pages that are in
//! flight (keyed by page, not target), so a stale-class job would
//! otherwise suppress the correct one with no later retry.

use std::sync::{Arc, Mutex};

use office_terminal_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use office_terminal_display::{
    DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver,
};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::reader_types::reader_viewport_metrics;
use crate::app::state::PresentationState;
use crate::app::ui::reader::render_reader;

/// Half-screen-style grid: tall enough that the renderer plans ONE column
/// while the stored default count still says two, and the two plans land
/// on DIFFERENT raster classes (1920 vs 1280) — the stale-plan geometry.
const WIDTH: u16 = 103;
const HEIGHT: u16 = 90;
const PAGES: usize = 4;

fn bitmap(w: u32, h: u32) -> RgbaSlide {
    RgbaSlide {
        width: w,
        height: h,
        pixels: vec![128u8; (w * h * 4) as usize].into(),
    }
}

/// Page 0 is `Ready` (the anchor paints), every later page `NotLoaded` —
/// the state right after opening before the peek raster lands.
struct SplitReadinessPipeline {
    window_prefetches: Mutex<Vec<Vec<(usize, RenderTargetSpec)>>>,
}

impl SplitReadinessPipeline {
    fn new() -> Self {
        Self {
            window_prefetches: Mutex::new(Vec::new()),
        }
    }

    fn window_pages(&self) -> Vec<Vec<usize>> {
        self.window_prefetches
            .lock()
            .expect("prefetch log lock")
            .iter()
            .map(|targets| targets.iter().map(|(page, _)| *page).collect())
            .collect()
    }

    fn last_targets(&self) -> Vec<(usize, RenderTargetSpec)> {
        self.window_prefetches
            .lock()
            .expect("prefetch log lock")
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

impl RenderPipeline for SplitReadinessPipeline {
    fn entry_at(&self, index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        if index == 0 {
            PipelineEntry::Ready {
                bitmap: bitmap(1920, 2485),
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }

    fn entry(&self, index: usize) -> PipelineEntry {
        self.entry_at(index, &RenderTargetSpec {
            width_px: 1920,
            height_px: 2485,
        })
    }

    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}

    fn prefetch_window(&self, _plan: &PrefetchPlan, targets: &[(usize, RenderTargetSpec)]) {
        self.window_prefetches
            .lock()
            .expect("prefetch log lock")
            .push(targets.to_vec());
    }

    fn invalidate(&self, _cause: InvalidateCause) {}
}

struct RecordingDriver;

impl TerminalGraphicsDriver for RecordingDriver {
    fn name(&self) -> &'static str {
        "prefetch-test"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: true,
            resident_ids: 64,
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

/// The stored effective count at this geometry still says two columns
/// (the stale plan), while the renderer plans one — the mismatch trigger.
#[test]
fn planned_columns_diverges_from_stale_effective_plan() {
    let mut state = PresentationState::new(PAGES, vec![None; PAGES]);
    state.enable_reader_mode(PAGES, vec![(612.0, 792.0); PAGES]);
    state.set_layout_dimensions(WIDTH, HEIGHT);

    let (vw, vh, cw, _) = reader_viewport_metrics(WIDTH, HEIGHT);
    let reader = state.reader_state().expect("reader state");

    let planned = reader.planned_columns(WIDTH, vh, cw);
    assert_eq!(planned, 1, "only one page column fits at this geometry");

    // Planned-plan pane width: the page fills the viewport height (1080 px)
    // -> pane width 1080 * 0.7727 -> the 1920 px raster class.
    let pane_w = reader.pane_width_px_for(1, vw, vh, cw, planned);
    let target = reader.page_render_target(1, pane_w);
    assert_eq!(target.width_px, 1920, "planned-plan target hits the 1920 class");

    // The stale effective plan (two columns, 606 px slot) would have
    // produced a different, never-matching class.
    let stale_pane_w = reader.pane_width_px(1, vw, vh, cw);
    let stale_target = reader.page_render_target(1, stale_pane_w);
    assert_ne!(
        target, stale_target,
        "this geometry must reproduce the stale-plan mismatch to guard the fix"
    );
}

/// While the peek pane's exact-target raster is `NotLoaded`, the syncer
/// keeps re-firing `prefetch_window` with the placement's own target —
/// even when the target list itself is unchanged between frames (the
/// stale-plan target would otherwise suppress it forever).
#[test]
fn syncer_refires_prefetch_while_peek_raster_is_unscheduled() {
    let pipeline = Arc::new(SplitReadinessPipeline::new());
    let dyn_pipeline: Arc<dyn RenderPipeline> = Arc::clone(&pipeline) as Arc<dyn RenderPipeline>;
    let mut syncer = GraphicsSyncer::new();
    let mut driver = RecordingDriver;
    let canvas = RenderTargetSpec {
        width_px: 1920,
        height_px: 2485,
    };

    let mut state = PresentationState::new(PAGES, vec![None; PAGES]);
    state.enable_reader_mode(PAGES, vec![(612.0, 792.0); PAGES]);
    state.set_layout_dimensions(WIDTH, HEIGHT);
    state.publish_frame_bindings(FrameBindings::default());

    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();

    let mut frame = |state: &mut PresentationState,
                     syncer: &mut GraphicsSyncer,
                     driver: &mut RecordingDriver,
                     pipeline: &dyn RenderPipeline| {
        let mut bindings = FrameBindings::default();
        terminal
            .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
            .unwrap();
        state.publish_frame_bindings(bindings.clone());
        syncer
            .sync(state, pipeline, driver, &canvas, &bindings)
            .unwrap();
    };

    frame(&mut state, &mut syncer, &mut driver, dyn_pipeline.as_ref());
    frame(&mut state, &mut syncer, &mut driver, dyn_pipeline.as_ref());

    // Both frames saw the peek pane unscheduled; the second frame must have
    // re-fired prefetching despite unchanged targets.
    let calls = pipeline.window_pages();
    assert!(
        calls.len() >= 2,
        "prefetch_window must re-fire while a placement is NotLoaded, calls={calls:?}"
    );
    assert!(
        calls.iter().all(|pages| pages.contains(&1)),
        "page 2 (index 1) must be in every prefetch window, calls={calls:?}"
    );

    // The scheduled target must be the placement's own (1920-class), not a
    // stale-plan class.
    for (page, target) in pipeline.last_targets() {
        if page == 1 {
            assert_eq!(
                target.width_px, 1920,
                "peek raster must be scheduled at the placement's target class"
            );
        }
    }
}
