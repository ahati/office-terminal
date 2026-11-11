//! Unit tests for the terminal-graphics placement syncer.

use std::sync::{Arc, Mutex};

use kitty_pptx_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use kitty_pptx_display::{
    CellRect, DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::graphics::GraphicsSyncer;
use crate::app::mode::{AppMode, BlankMode};
use crate::app::state::PresentationState;
use crate::app::ui::render_app;
use crate::app::PresentationApp;

fn test_bitmap(width: u32, height: u32) -> RgbaSlide {
    RgbaSlide {
        width,
        height,
        pixels: vec![128u8; (width * height * 4) as usize].into(),
    }
}

struct MockPipeline {
    slides: usize,
}

impl RenderPipeline for MockPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, index: usize) -> PipelineEntry {
        if index < self.slides {
            PipelineEntry::Ready {
                bitmap: test_bitmap(4, 2),
                staged: false,
            }
        } else {
            PipelineEntry::NotLoaded
        }
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
}

#[derive(Default, Debug)]
struct DriverLog {
    staged: Vec<u32>,
    presented: Vec<(u32, CellRect)>,
    repositioned: Vec<(u32, CellRect)>,
    released: Vec<u32>,
    released_all: usize,
}

struct MockDriver {
    log: Arc<Mutex<DriverLog>>,
}

impl MockDriver {
    fn new() -> (Self, Arc<Mutex<DriverLog>>) {
        let log = Arc::new(Mutex::new(DriverLog::default()));
        (
            Self {
                log: Arc::clone(&log),
            },
            log,
        )
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
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        if let Placement::Cells(rect) = p {
            self.log.lock().unwrap().presented.push((id.0, rect));
        }
        Ok(())
    }
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        if let Placement::Cells(rect) = p {
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

fn draw_frame(state: &PresentationState) {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    state.clear_placement_rects();
    terminal.draw(|f| render_app(state, f)).unwrap();
}

#[test]
fn test_direct_positioning_stages_and_presents_once() {
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let expected = state.placement_rects().0.expect("slide rect recorded");
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged, vec![1]);
        assert_eq!(log.presented, vec![(1, expected)]);
    }

    // An unchanged frame must not re-stage or re-place the image.
    syncer.sync(&state, &pipeline, &mut driver).unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(log.staged.len(), 1);
        assert_eq!(log.presented.len(), 1);
        assert!(log.repositioned.is_empty());
    }

    // Slide change: the old id is released and the new one placed.
    state.next_slide();
    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();
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
    let state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    // Simulate a resize: new rect for the same slide/id.
    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    state.clear_placement_rects();
    terminal.draw(|f| render_app(&state, f)).unwrap();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

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

    draw_frame(&state);
    let (current, preview) = state.placement_rects();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.staged, vec![1, 10]);
    assert_eq!(log.presented.len(), 2);
    assert_eq!(log.presented[0], (1, current.expect("current rect")));
    assert_eq!(log.presented[1], (10, preview.expect("preview rect")));
}

#[test]
fn test_presenter_navigation_keeps_current_image() {
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(3, vec![]);
    state.set_mode(AppMode::PresenterConsole);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    // Slide 1 was the preview (id 10) and becomes the current slide (id 2).
    // Releasing the old preview must not release the new current image.
    state.next_slide();
    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let log = log.lock().unwrap();
    assert!(
        log.released.contains(&10),
        "old preview id 10 should be released"
    );
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
    let state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    // No slide widget drawn this frame (e.g. sorter).
    state.clear_placement_rects();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.released, vec![1]);
}

#[test]
fn test_blank_screen_suppresses_graphics_once_then_resumes() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new();
    let mut state = PresentationState::new(2, vec![]);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    state.toggle_blank(BlankMode::Black);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();
    assert_eq!(log.lock().unwrap().released_all, 1);

    state.unblank();
    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log.staged.len(), 2);
    assert_eq!(log.presented.len(), 2);
}
#[test]
fn test_resize_preserves_slide_aspect_ratio() {
    let pipeline: Arc<dyn RenderPipeline> = Arc::new(MockPipeline { slides: 1 });
    let (driver, _) = MockDriver::new();
    let mut app = PresentationApp::new(pipeline, Box::new(driver), 1, vec![])
        .with_slide_dimensions(10_000_000, 7_500_000); // 4:3

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
