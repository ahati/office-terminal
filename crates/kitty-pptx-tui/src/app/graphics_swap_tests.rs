//! Regression tests for the terminal-graphics *swap* discipline.
//!
//! **Sorter → slide flicker.** Kitty deletes an image *and every placement of
//! it* the moment new data is transmitted under its id ("when re-transmitting
//! image data for a specific id, the existing image and all its placements
//! must be deleted ... the new data ... is not actually displayed until a
//! placement for it is created"). The canvas resolves a provisional bitmap
//! first — the sorter's 1280-px card raster, or the frame from before a
//! resize — and then the canvas-target raster for the same slide. That upgrade
//! used to re-transmit the *displayed* id, so the slide vanished for the whole
//! transmit + decode window and popped back in: "the slide appears, disappears
//! with a flicker, then appears again". The syncer now stages into a spare id
//! of the slot's own pool while the preferred id is on screen, and presents the
//! incoming bitmap before releasing the outgoing one, so the slot is never
//! empty on screen.
//!
//! **Sorter teardown.** Leaving the grid released every card before the slide
//! had any pixels, blanking the screen for the length of the canvas render. The
//! grid now stays resident until the slide view owns the screen.
//!
//! **Loop wake.** The run loop sampled `in_flight()` before the frame pass,
//! but the sorter schedules its window prefetch *inside* that pass. The wake
//! deadline therefore read an idle pipeline and blocked on stdin, so the grid
//! only filled in on the next key press. `PresentationApp::poll_timeout`
//! samples after the pass.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kitty_pptx_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use kitty_pptx_display::{
    DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver, CURRENT_IMAGE_ID_SLOTS,
    CURRENT_IMAGE_ID_BASE, PREVIEW_IMAGE_ID_BASE, THUMBNAIL_IMAGE_ID_BASE,
};
use ratatui::layout::Rect;

use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::loop_wake::{RedrawTracker, IDLE_POLL, RENDER_TICK};
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::app::PresentationApp;

const ASPECT: f32 = 16.0 / 9.0;
pub(crate) const CANVAS: RenderTargetSpec = RenderTargetSpec {
    width_px: 160,
    height_px: 90,
};

/// Raster size the pipeline reports for a slide; toggled between the sorter's
/// card raster and the canvas raster to model the upgrade.
pub(crate) const CARD_RASTER: (u32, u32) = (64, 36);
const CANVAS_RASTER: (u32, u32) = (160, 90);

fn bitmap(size: (u32, u32)) -> RgbaSlide {
    RgbaSlide {
        width: size.0,
        height: size.1,
        pixels: vec![9u8; (size.0 * size.1 * 4) as usize].into(),
    }
}

/// Pipeline whose per-slide outcome the test scripts directly.
#[derive(Default)]
pub(crate) struct ScriptedPipeline {
    sizes: Mutex<HashMap<usize, (u32, u32)>>,
    rendering: Mutex<HashSet<usize>>,
    in_flight: AtomicUsize,
}

impl ScriptedPipeline {
    pub(crate) fn ready(sizes: &[(usize, (u32, u32))]) -> Self {
        Self {
            sizes: Mutex::new(sizes.iter().copied().collect()),
            ..Self::default()
        }
    }

    pub(crate) fn set_size(&self, slide: usize, size: (u32, u32)) {
        self.sizes.lock().unwrap().insert(slide, size);
        self.rendering.lock().unwrap().remove(&slide);
    }

    /// The slide has no raster yet and its render is in flight.
    pub(crate) fn set_rendering(&self, slide: usize) {
        self.sizes.lock().unwrap().remove(&slide);
        self.rendering.lock().unwrap().insert(slide);
    }

    fn set_in_flight(&self, count: usize) {
        self.in_flight.store(count, Ordering::SeqCst);
    }

    /// Number of renders a window prefetch queues (the frame-pass side
    /// effect the wake deadline must observe).
    const PREFETCH_RENDERS: usize = 12;

    fn resolve(&self, index: usize) -> PipelineEntry {
        if let Some(size) = self.sizes.lock().unwrap().get(&index).copied() {
            return PipelineEntry::Ready {
                bitmap: bitmap(size),
            };
        }
        if self.rendering.lock().unwrap().contains(&index) {
            return PipelineEntry::Rendering;
        }
        PipelineEntry::NotLoaded
    }
}

impl RenderPipeline for ScriptedPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {
        // Scheduling work inside the frame pass is exactly what the sorter's
        // window prefetch does; the wake deadline has to see it.
        self.in_flight.store(Self::PREFETCH_RENDERS, Ordering::SeqCst);
    }

    fn entry(&self, index: usize) -> PipelineEntry {
        self.resolve(index)
    }

    fn entry_at(&self, index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        self.resolve(index)
    }

    fn invalidate(&self, _cause: InvalidateCause) {}

    fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }
}

/// Driver calls in issue order — the present/release *sequence* is the point.
#[derive(Default, Debug)]
pub(crate) struct CallLog {
    pub(crate) calls: Vec<String>,
}

impl CallLog {
    fn record(&mut self, call: impl Into<String>) {
        self.calls.push(call.into());
    }

    pub(crate) fn position(&self, needle: &str) -> Option<usize> {
        self.calls.iter().position(|call| call.contains(needle))
    }

    pub(crate) fn count_prefix(&self, prefix: &str) -> usize {
        self.calls.iter().filter(|call| call.starts_with(prefix)).count()
    }
}

pub(crate) struct LoggingDriver {
    log: Arc<Mutex<CallLog>>,
}

impl LoggingDriver {
    pub(crate) fn new() -> (Self, Arc<Mutex<CallLog>>) {
        let log = Arc::new(Mutex::new(CallLog::default()));
        (
            Self {
                log: Arc::clone(&log),
            },
            log,
        )
    }
}

impl TerminalGraphicsDriver for LoggingDriver {
    fn name(&self) -> &'static str {
        "logging"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: true,
            resident_ids: 128,
        }
    }
    fn placement_mode(&self) -> PlacementMode {
        PlacementMode::Interactive
    }
    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        self.log
            .lock()
            .unwrap()
            .record(format!("stage id={} {}x{}", id.0, bmp.width, bmp.height));
        Ok(())
    }
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        match p {
            Placement::Cells(rect) => {
                self.log.lock().unwrap().record(format!("present id={} {rect:?}", id.0));
            }
            Placement::CellsSubrect { rect, .. } => {
                self.log
                    .lock()
                    .unwrap()
                    .record(format!("present id={} subrect {rect:?}", id.0));
            }
            Placement::ScrollbackBlock => {}
        }
        Ok(())
    }
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        if let Placement::Cells(rect) = p {
            self.log
                .lock()
                .unwrap()
                .record(format!("reposition id={} {rect:?}", id.0));
        }
        Ok(())
    }
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        self.log.lock().unwrap().record(format!("release id={}", id.0));
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        self.log.lock().unwrap().record("release_all");
        Ok(())
    }
}

fn canvas_bindings() -> FrameBindings {
    let mut bindings = FrameBindings::default();
    bindings.record_current(Rect::new(0, 0, 80, 24), ASPECT, 1.0, (0.0, 0.0));
    bindings
}

fn slide_state(slides: usize) -> PresentationState {
    PresentationState::new(slides, vec![])
}

#[test]
fn an_upgrade_swaps_image_ids_instead_of_retransmitting_the_displayed_one() {
    let pipeline = ScriptedPipeline::ready(&[(0, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = slide_state(4);
    let bindings = canvas_bindings();

    // The card raster is resolved first (the sorter's dedicated target is a
    // legitimate provisional bitmap, exactly like the pre-resize frame).
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    let first_id = state.current_image_id();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.calls[0], format!("stage id={} 64x36", first_id.0));
        assert!(log.position("present").is_some());
    }

    // The canvas-target raster lands: the display must not be re-transmitted.
    pipeline.set_size(0, CANVAS_RASTER);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(
        log.count_prefix(&format!("stage id={} ", first_id.0)),
        1,
        "the displayed image id is never re-transmitted: {:?}",
        log.calls
    );
    let stage = log.position("160x90").expect("the canvas raster is staged");
    let present = log
        .position(&format!("present id={}", first_id.0 + 1))
        .expect("the spare id is presented");
    let release = log
        .position(&format!("release id={}", first_id.0))
        .expect("the outgoing bitmap is released");
    assert!(
        stage < present && present < release,
        "stage → present → release keeps the slot filled: {:?}",
        log.calls
    );
    // The spare id comes from the current-slide pool, not from another view.
    assert!((first_id.0..first_id.0 + CURRENT_IMAGE_ID_SLOTS).contains(&(first_id.0 + 1)));
}

#[test]
fn a_slide_jump_that_reuses_the_slot_id_swaps_first() {
    // Slides are 8 apart, so both resolve to the same current-slide id: the
    // jump must not re-transmit the id that is still displayed.
    let pipeline = ScriptedPipeline::ready(&[(0, CARD_RASTER), (8, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = slide_state(16);
    let bindings = canvas_bindings();

    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    let displayed = state.current_image_id();

    state.set_current_slide(8);
    assert_eq!(
        state.current_image_id(),
        displayed,
        "the slot id repeats every 8 slides"
    );
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(
        log.count_prefix(&format!("stage id={} ", displayed.0)),
        1,
        "the displayed id is transmitted once only: {:?}",
        log.calls
    );
    let present = log
        .position(&format!("present id={}", displayed.0 + 1))
        .expect("the new slide is presented under the spare id");
    let release = log
        .position(&format!("release id={}", displayed.0))
        .expect("the outgoing slide is released");
    assert!(
        present < release,
        "the incoming slide is presented before the outgoing one is released: {:?}",
        log.calls
    );
}

#[test]
fn the_sorter_grid_survives_until_the_slide_is_on_screen() {
    let pipeline = ScriptedPipeline::ready(&[(0, CARD_RASTER), (1, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = slide_state(6);
    state.set_mode(AppMode::Sorter);

    let mut sorter = FrameBindings::default();
    sorter.record_thumbnail(0, Rect::new(0, 0, 20, 6), ASPECT);
    sorter.record_thumbnail(1, Rect::new(21, 0, 20, 6), ASPECT);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &sorter)
        .unwrap();
    let cards = sorter.thumbnails.len();
    assert_eq!(cards, 2);
    {
        let log = log.lock().unwrap();
        assert!(
            log.calls.iter().any(|c| c.starts_with(&format!(
                "stage id={THUMBNAIL_IMAGE_ID_BASE}"
            ))),
            "cards live in the sorter id range: {:?}",
            log.calls
        );
    }

    // Back to the slide view while slide 1 is still rendering: the grid must
    // stay resident instead of leaving an empty screen.
    state.set_current_slide(1);
    state.set_mode(AppMode::Fullscreen);
    pipeline.set_rendering(1);
    let bindings = canvas_bindings();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(
            log.count_prefix("release id="),
            0,
            "no card may be released while the slide has no pixels: {:?}",
            log.calls
        );
    }

    // The render lands: the slide is presented first, then the grid is torn
    // down, so the screen is never empty.
    pipeline.set_size(1, CANVAS_RASTER);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    let log = log.lock().unwrap();
    let present = log
        .position(&format!("present id={}", state.current_image_id().0))
        .expect("the slide is presented");
    let first_card_release = log
        .calls
        .iter()
        .position(|call| call.starts_with(&format!("release id={THUMBNAIL_IMAGE_ID_BASE}")))
        .expect("the grid is released once the slide is up");
    assert!(
        present < first_card_release,
        "present-then-release keeps the transition seamless: {:?}",
        log.calls
    );
    assert!(
        !log.calls
            .iter()
            .any(|c| c.contains(&format!("id={PREVIEW_IMAGE_ID_BASE}"))),
        "the preview pool stays untouched: {:?}",
        log.calls
    );
}

#[test]
fn renders_in_flight_and_queued_completions_keep_the_loop_awake() {
    let pipeline = Arc::new(ScriptedPipeline::default());
    let (driver, _log) = LoggingDriver::new();
    let (done_tx, done_rx) = crossbeam_channel::unbounded();
    let mut app = PresentationApp::new(pipeline.clone(), Box::new(driver), 8, vec![None; 8])
        .with_render_events(done_rx);
    // Hide the status line: no clock deadline, so an idle slide view has no
    // reason to wake on its own.
    app.state_mut().toggle_status_line();

    let mut tracker = RedrawTracker::new();
    tracker.evaluate(app.state(), 0);
    tracker.mark_drawn(app.state());
    assert_eq!(app.poll_timeout(&tracker), IDLE_POLL);

    // ── Frame pass ──
    // The sorter issues its visible-window prefetch while the frame pass
    // runs. The wake deadline is computed afterwards and samples the worker
    // count itself; a count latched before this pass would report an idle
    // pipeline and let the loop block on stdin, so the grid would only fill
    // in on the next key press.
    assert_eq!(app.poll_timeout(&tracker), IDLE_POLL);
    pipeline.prefetch(
        &PrefetchPlan {
            center: 0,
            behind: 0,
            ahead: ScriptedPipeline::PREFETCH_RENDERS - 1,
            stage: true,
        },
        &CANVAS,
    );
    assert_eq!(app.poll_timeout(&tracker), RENDER_TICK);

    // A completion that raced the drain is not slept through either.
    pipeline.set_in_flight(0);
    assert_eq!(app.poll_timeout(&tracker), IDLE_POLL);
    done_tx.send(()).unwrap();
    assert_eq!(app.poll_timeout(&tracker), std::time::Duration::ZERO);
}

/// The current slide id must stay inside the pool the swap draws its spare id
/// from — the invariant `stage_id` relies on.
#[test]
fn the_current_slide_id_pool_covers_the_swap_range() {
    let mut state = slide_state(16);
    for slide in 0..16 {
        state.set_current_slide(slide);
        let id = state.current_image_id().0;
        assert!(
            (CURRENT_IMAGE_ID_BASE..CURRENT_IMAGE_ID_BASE + CURRENT_IMAGE_ID_SLOTS).contains(&id),
            "slide {slide} resolves current id {id} outside the current pool"
        );
    }
}
