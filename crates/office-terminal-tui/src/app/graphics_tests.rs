//! Unit tests for the terminal-graphics placement syncer.

use std::sync::{Arc, Mutex};

use office_terminal_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec, RgbaSlide,
    TransmitPayload,
};
use office_terminal_display::{CellRect, DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::mode::{AppMode, BlankMode};
use crate::app::state::PresentationState;
use crate::app::ui::render_app;
use crate::app::PresentationApp;

/// Canvas target the syncer resolves current/preview/sorter rasters against.
pub(super) fn canvas() -> RenderTargetSpec {
    RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    }
}

fn test_bitmap(width: u32, height: u32) -> RgbaSlide {
    RgbaSlide {
        width,
        height,
        pixels: vec![128u8; (width * height * 4) as usize].into(),
    }
}

pub(super) struct MockPipeline {
    pub(super) slides: usize,
}

impl RenderPipeline for MockPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, index: usize) -> PipelineEntry {
        if index < self.slides {
            PipelineEntry::Ready {
                bitmap: test_bitmap(4, 2),
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
}

#[derive(Default, Debug)]
pub(super) struct DriverLog {
    pub(super) staged: Vec<u32>,
    presented: Vec<(u32, CellRect)>,
    repositioned: Vec<(u32, CellRect)>,
    /// Full placement forms (whole-canvas vs zoomed crop), in emission
    /// order: presents and same-id repositions alike — both are terminal
    /// placements, and zoom/pan is precisely the reposition path.
    pub(super) presented_forms: Vec<(u32, Placement)>,
    pub(super) released: Vec<u32>,
    released_all: usize,
    prebuilt: Vec<(u32, u32, u32)>,
}

/// Cell rect of either placement form (whole-canvas or zoomed crop).
pub(super) fn placement_rect(p: &Placement) -> Option<CellRect> {
    match p {
        Placement::Cells(rect) => Some(*rect),
        Placement::CellsSubrect { rect, .. } => Some(*rect),
        Placement::ScrollbackBlock => None,
    }
}

pub(super) struct MockDriver {
    log: Arc<Mutex<DriverLog>>,
}

impl MockDriver {
    pub(super) fn new() -> (Self, Arc<Mutex<DriverLog>>) {
        let log = Arc::new(Mutex::new(DriverLog::default()));
        (Self { log: Arc::clone(&log) }, log)
    }
}

impl TerminalGraphicsDriver for MockDriver {
    fn name(&self) -> &'static str {
        "mock"
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
    fn stage(&mut self, _bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        self.log.lock().unwrap().staged.push(id.0);
        Ok(())
    }
    fn transmit_prebuilt(
        &mut self,
        id: ImageId,
        width: u32,
        height: u32,
        _payload: &TransmitPayload,
    ) -> Result<(), DisplayError> {
        self.log.lock().unwrap().prebuilt.push((id.0, width, height));
        Ok(())
    }
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        self.log.lock().unwrap().presented_forms.push((id.0, p.clone()));
        if let Some(rect) = placement_rect(&p) {
            self.log.lock().unwrap().presented.push((id.0, rect));
        }
        Ok(())
    }
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        self.log.lock().unwrap().presented_forms.push((id.0, p.clone()));
        if let Some(rect) = placement_rect(&p) {
            self.log.lock().unwrap().repositioned.push((id.0, rect));
        }
        Ok(())
    }
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        self.log.lock().unwrap().released.push(id.0);
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        self.log.lock().unwrap().released_all += 1;
        Ok(())
    }
}

/// Draws one 80x24 frame at the given size and returns the bindings the
/// render pass produced (the run loop would sync and publish them).
fn draw_frame(state: &PresentationState, width: u16, height: u16) -> FrameBindings {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut bindings = FrameBindings::default();
    terminal.draw(|f| render_app(state, f, &mut bindings)).unwrap();
    bindings
}

#[test]
fn test_direct_positioning_stages_and_presents_once() {
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    let expected = bindings.current.expect("slide placement recorded").rect;
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged, vec![1]);
        assert_eq!(log.presented, vec![(1, expected)]);
    }

    // An unchanged frame must not re-stage or re-place the image.
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged.len(), 1);
        assert_eq!(log.presented.len(), 1);
        assert!(log.repositioned.is_empty());
    }

    // Slide change: the old id is released and the new one placed.
    state.next_slide();
    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged, vec![1, 2]);
        assert_eq!(log.presented.len(), 2);
        assert_eq!(log.presented[1].0, 2);
        assert_eq!(log.released, vec![1]);
    }
}

#[test]
fn test_rect_change_uses_reposition() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    // Simulate a resize: new rect for the same slide/id.
    let bindings = draw_frame(&state, 60, 20);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.presented.len(), 1);
    assert_eq!(log.repositioned.len(), 1);
    assert_eq!(log.repositioned[0].0, 1);
}

#[test]
fn test_presenter_mode_places_preview() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(2, vec![]);
    state.set_mode(AppMode::PresenterConsole);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    let (current, preview) = (bindings.current, bindings.preview);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.staged, vec![1, 10]);
    assert_eq!(log.presented.len(), 2);
    assert_eq!(log.presented[0], (1, current.expect("current placement").rect));
    assert_eq!(log.presented[1], (10, preview.expect("preview placement").rect));
}

#[test]
fn test_presenter_navigation_keeps_current_image() {
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    state.set_mode(AppMode::PresenterConsole);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    // Slide 1 was the preview (id 10) and becomes the current slide (id 2).
    // Releasing the old preview must not release the new current image.
    state.next_slide();
    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    let log = log.lock().unwrap();
    assert!(log.released.contains(&10), "old preview id 10 should be released");
    assert!(
        !log.released.contains(&2),
        "new current id 2 must not be released by preview cleanup"
    );
    assert!(
        log.presented.iter().any(|(id, _)| *id == 2),
        "new current slide should be presented"
    );
}

#[test]
fn test_missing_rect_releases_previous_image() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    // No slide widget drawn this frame (e.g. sorter): empty bindings.
    let empty = FrameBindings::default();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &empty)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.released, vec![1]);
}

#[test]
fn test_blank_screen_suppresses_graphics_once_then_resumes() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    state.toggle_blank(BlankMode::Black);
    let blank = FrameBindings::default();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &blank)
        .unwrap();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &blank)
        .unwrap();
    assert_eq!(log.lock().unwrap().released_all, 1);

    state.unblank();
    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log.staged.len(), 2);
    assert_eq!(log.presented.len(), 2);
}
#[test]
fn test_resize_preserves_slide_aspect_ratio() {
    let pipeline: Arc<dyn RenderPipeline> = Arc::new(MockPipeline { slides: 1 });
    let (driver, _) = MockDriver::new();
    let mut app =
        PresentationApp::new(pipeline, Box::new(driver), 1, vec![]).with_slide_dimensions(10_000_000, 7_500_000); // 4:3

    app.handle_resize(80, 24);
    let target = app.current_target;
    let aspect = target.width_px as f64 / target.height_px as f64;
    assert!(
        (aspect - 4.0 / 3.0).abs() < 0.01,
        "resized target must preserve the slide aspect: {}x{} (aspect {aspect:.3})",
        target.width_px,
        target.height_px
    );
    assert!((app.state.aspect_ratio() - 4.0 / 3.0).abs() < 0.001);
}

// ── failed-render surfacing ─────────────────────────────────────────────────

struct FailingPipeline;

impl RenderPipeline for FailingPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, _index: usize) -> PipelineEntry {
        PipelineEntry::Failed {
            error: office_terminal_core::RenderError::RasterizationError("boom: glyph atlas overflow".to_string()),
        }
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
}

/// A failed background render must be recorded (canvas note + one placeholder
/// warning) instead of leaving the slide silently blank forever.
#[test]
fn a_failed_render_is_surfaced_in_state_and_not_staged() {
    let (mut driver, log) = MockDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = PresentationState::new(2, vec![]);

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &FailingPipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    assert!(
        log.lock().unwrap().staged.is_empty(),
        "nothing can be staged from a Failed entry"
    );
    let (slide, message) = state.render_failure().expect("the failure is recorded on the state");
    assert_eq!(slide, 0);
    assert!(message.contains("boom"), "message preserved: {message}");
    assert_eq!(state.warnings.len(), 1, "exactly one drawer entry");
    assert!(
        !state.warnings.warnings_for_slide(0).is_empty(),
        "the warning is attached to the failed slide"
    );

    // Idempotent per (slide, message): syncing every frame must not spam.
    for _ in 0..3 {
        syncer
            .sync(&mut state, &FailingPipeline, &mut driver, &canvas(), &bindings)
            .unwrap();
    }
    assert_eq!(state.warnings.len(), 1, "no duplicate warning entries");
}

/// Once the pipeline produces the bitmap, the failure record clears and the
/// slide stages normally.
#[test]
fn a_recovered_render_clears_the_failure_record() {
    let (mut driver, log) = MockDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = PresentationState::new(2, vec![]);

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &FailingPipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    assert!(state.render_failure().is_some());

    let pipeline = MockPipeline { slides: 2 };
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    assert!(
        state.render_failure().is_none(),
        "a successful stage clears the failure"
    );
    assert_eq!(log.lock().unwrap().staged, vec![1]);
}

/// The canvas draws a visible degraded-render note for the failed slide.
#[test]
fn a_failed_render_draws_a_note_on_the_canvas() {
    let mut state = PresentationState::new(2, vec![]);
    state.note_render_failure(0, "boom: glyph atlas overflow".to_string());

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut bindings = FrameBindings::default();
    terminal.draw(|f| render_app(&state, f, &mut bindings)).unwrap();

    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        text.contains("RENDER FAILED"),
        "the canvas shows the failure badge: {text:?}"
    );
    assert!(
        text.contains("glyph atlas overflow"),
        "the canvas shows the failure message"
    );

    // Navigating to a healthy slide hides the note.
    let mut other = PresentationState::new(2, vec![]);
    other.note_render_failure(1, "unrelated slide".to_string());
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut other_bindings = FrameBindings::default();
    terminal.draw(|f| render_app(&other, f, &mut other_bindings)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        !text.contains("RENDER FAILED"),
        "no note for a failure on another slide"
    );
}

/// A pipeline whose rasters carry worker-pre-encoded transmission payloads.
struct PrebuiltPipeline {
    slides: usize,
}

impl RenderPipeline for PrebuiltPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, index: usize) -> PipelineEntry {
        if index < self.slides {
            PipelineEntry::Ready {
                bitmap: test_bitmap(4, 2),
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }
    fn staged_payload(&self, _index: usize, _target: &RenderTargetSpec) -> Option<TransmitPayload> {
        Some(TransmitPayload {
            base64: "".into(),
            base64_len: 0,
        })
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
}

#[test]
fn current_slide_consumes_prebuilt_payload_without_on_thread_encode() {
    let pipeline = PrebuiltPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    let mut syncer = GraphicsSyncer::new();

    let bindings = draw_frame(&state, 80, 24);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();

    {
        let log = log.lock().unwrap();
        assert_eq!(
            log.prebuilt.len(),
            1,
            "staging must consume the worker's pre-encoded payload"
        );
        assert_eq!(log.prebuilt[0].1, 4, "payload width comes from the raster");
        assert_eq!(log.prebuilt[0].2, 2, "payload height comes from the raster");
        assert!(
            log.staged.is_empty(),
            "no on-thread encode may run while a payload exists"
        );
        assert_eq!(log.presented.len(), 1, "the slide is placed after staging");
    }

    // An unchanged frame must not re-transmit anything.
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas(), &bindings)
        .unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log.prebuilt.len(), 1);
    assert_eq!(log.presented.len(), 1);
}
