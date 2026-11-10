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

use crate::app::graphics::{render_mode_for, GraphicsSyncer};
use crate::app::mode::{AppMode, BlankMode};
use crate::app::state::PresentationState;
use crate::app::ui::render_app;
use crate::widget::SlideRenderMode;

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
    placeholders: bool,
}

impl MockDriver {
    fn new(placeholders: bool) -> (Self, Arc<Mutex<DriverLog>>) {
        let log = Arc::new(Mutex::new(DriverLog::default()));
        (
            Self {
                log: Arc::clone(&log),
                placeholders,
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
            placeholders: self.placeholders,
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
fn test_render_mode_selection_prefers_placeholders_only_when_supported() {
    let (placeholder_driver, _) = MockDriver::new(true);
    let (direct_driver, _) = MockDriver::new(false);
    assert_eq!(
        render_mode_for(&placeholder_driver),
        SlideRenderMode::Placeholder
    );
    assert_eq!(
        render_mode_for(&direct_driver),
        SlideRenderMode::DirectPositioning
    );
}

#[test]
fn test_direct_positioning_stages_and_presents_once() {
    let pipeline = MockPipeline { slides: 3 };
    let (mut driver, log) = MockDriver::new(false);
    let mut state = PresentationState::new(3, vec![]);
    state.set_slide_render_mode(SlideRenderMode::DirectPositioning);
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
    let (mut driver, log) = MockDriver::new(false);
    let mut state = PresentationState::new(2, vec![]);
    state.set_slide_render_mode(SlideRenderMode::DirectPositioning);
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
fn test_placeholder_mode_presents_after_frame() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new(true);
    let mut state = PresentationState::new(2, vec![]);
    state.set_slide_render_mode(SlideRenderMode::Placeholder);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let expected = state.placement_rects().0.expect("slide rect recorded");
    let log = log.lock().unwrap();
    assert_eq!(log.staged, vec![1]);
    // Placeholder placement carries the same cell rect as direct mode.
    assert_eq!(log.presented, vec![(1, expected)]);
}

#[test]
fn test_presenter_mode_places_preview() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new(false);
    let mut state = PresentationState::new(2, vec![]);
    state.set_slide_render_mode(SlideRenderMode::DirectPositioning);
    state.set_mode(AppMode::PresenterConsole);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    let (current, preview) = state.placement_rects();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.staged, vec![1, 2]);
    assert_eq!(log.presented.len(), 2);
    assert_eq!(log.presented[0], (1, current.expect("current rect")));
    assert_eq!(log.presented[1], (2, preview.expect("preview rect")));
}

#[test]
fn test_missing_rect_releases_previous_image() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new(false);
    let mut state = PresentationState::new(2, vec![]);
    state.set_slide_render_mode(SlideRenderMode::DirectPositioning);
    let mut syncer = GraphicsSyncer::new();

    draw_frame(&state);
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    // No slide widget drawn this frame (e.g. sorter on a direct driver).
    state.clear_placement_rects();
    syncer.sync(&state, &pipeline, &mut driver).unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.released, vec![1]);
}

#[test]
fn test_blank_screen_suppresses_graphics_once_then_resumes() {
    let pipeline = MockPipeline { slides: 2 };
    let (mut driver, log) = MockDriver::new(false);
    let mut state = PresentationState::new(2, vec![]);
    state.set_slide_render_mode(SlideRenderMode::DirectPositioning);
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
