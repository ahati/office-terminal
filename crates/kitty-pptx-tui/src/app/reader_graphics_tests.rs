//! L2 Layout and protocol golden tests for Reader mode (PRD 14a §11, Issues #24 & #26).

use std::sync::{Arc, Mutex};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use kitty_pptx_display::{
    CellRect, DriverCaps, ImageId, PixelRect, Placement, PlacementMode, TerminalGraphicsDriver,
};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::events::handle_key_event;
use crate::app::graphics::GraphicsSyncer;
use crate::app::reader_state::{ReaderFitMode, ReaderLayout};
use crate::app::state::PresentationState;
use crate::app::ui::reader::render_reader;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn test_page_bitmap() -> RgbaSlide {
    RgbaSlide {
        width: 2560,
        height: 3312,
        pixels: vec![128u8; 2560 * 3312 * 4].into(),
    }
}

struct MockPdfPipeline;

impl RenderPipeline for MockPdfPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}

    fn entry(&self, _index: usize) -> PipelineEntry {
        PipelineEntry::Ready {
            bitmap: test_page_bitmap(),
            staged: false,
        }
    }

    fn entry_at(&self, _index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        PipelineEntry::Ready {
            bitmap: test_page_bitmap(),
            staged: false,
        }
    }

    fn invalidate(&self, _cause: InvalidateCause) {}
}

#[derive(Default, Debug)]
struct DriverRecord {
    staged: Vec<u32>,
    presented: Vec<(u32, CellRect, PixelRect)>,
    released: Vec<u32>,
}

struct MockRecordingDriver {
    record: Arc<Mutex<DriverRecord>>,
}

impl MockRecordingDriver {
    fn new() -> (Self, Arc<Mutex<DriverRecord>>) {
        let record = Arc::new(Mutex::new(DriverRecord::default()));
        (
            Self {
                record: Arc::clone(&record),
            },
            record,
        )
    }
}

impl TerminalGraphicsDriver for MockRecordingDriver {
    fn name(&self) -> &'static str {
        "mock-recording"
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

    fn stage(&mut self, _slide: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let mut r = self.record.lock().unwrap();
        r.staged.push(id.0);
        Ok(())
    }

    fn present(&mut self, id: ImageId, placement: Placement) -> Result<(), DisplayError> {
        let mut r = self.record.lock().unwrap();
        match placement {
            Placement::Cells(rect) => {
                r.presented.push((id.0, rect, PixelRect { x: 0, y: 0, width: 0, height: 0 }));
            }
            Placement::CellsSubrect { rect, src } => {
                r.presented.push((id.0, rect, src));
            }
            Placement::ScrollbackBlock => {}
        }
        Ok(())
    }

    fn reposition(&mut self, _id: ImageId, _placement: Placement) -> Result<(), DisplayError> {
        Ok(())
    }

    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        let mut r = self.record.lock().unwrap();
        r.released.push(id.0);
        Ok(())
    }

    fn release_all(&mut self) -> Result<(), DisplayError> {
        Ok(())
    }
}

#[test]
fn test_spread_zoom_layout_goldens_issue_24() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();

    let page_sizes = vec![(612.0, 792.0); 10]; // Letter / A4-proportional (ar = 0.7727)

    // 1. Zoom 1.0 (Fit)
    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);

    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p1 = state.reader_placements();
    assert_eq!(p1.len(), 2, "Both spread pages visible at zoom 1.0");
    assert_eq!(p1[0].cell_rect.cols, 68, "Left pane fits at 68 cols");
    assert_eq!(p1[0].cell_rect.col, 0);
    assert_eq!(p1[0].pixel_src.width, 2560, "Left pane full page width at fit");
    assert_eq!(p1[1].cell_rect.col, 70, "Right starts at col 70 (68 cols + 2 gutter)");
    assert_eq!(p1[1].cell_rect.cols, 68, "Right pane fits at 68 cols");
    assert_eq!(p1[1].pixel_src.width, 2560, "Right pane full page width");

    // 2. Zoom 1.5 (Grows)
    state.reader_state_mut().unwrap().adjust_zoom(1.5);
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p15 = state.reader_placements();
    assert_eq!(p15[0].cell_rect.cols, 102, "Left pane grows to 102 cols");
    assert_eq!(p15[0].pixel_src.width, 2560, "Left pane remains full page width");
    assert_eq!(p15.len(), 2, "Partner still placed with remaining cols");
    assert_eq!(p15[1].cell_rect.col, 104, "Right starts at col 104");
    assert_eq!(p15[1].cell_rect.cols, 56, "Right gets 160 - 104 = 56 cols");

    // 3. Zoom 2.0 (Owns Row in FitWidth)
    state.reader_state_mut().unwrap().reset_view();
    state.reader_state_mut().unwrap().cycle_fit(); // FitWidth (slot = 79 cols)
    state.reader_state_mut().unwrap().adjust_zoom(2.0); // 79 * 2 = 158 cols
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p2 = state.reader_placements();
    assert!(p2[0].cell_rect.cols >= 158 && p2[0].cell_rect.cols <= 160);
    assert_eq!(p2[0].pixel_src.width, 2560, "Full page width");
    assert_eq!(p2.len(), 1, "Right pane pushed out when left owns row");

    // 4. Zoom 3.0 (Clamped to Viewport with max_scroll_x > 0)
    state.reader_state_mut().unwrap().adjust_zoom(1.5); // 2.0 * 1.5 = 3.0
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p3 = state.reader_placements();
    assert_eq!(p3[0].cell_rect.cols, 160, "Clamped to viewport width");
    assert_eq!(p3.len(), 1, "Partner pushed out");
    assert!(p3[0].max_scroll_x > 0, "max_scroll_x > 0 when zoomed past viewport");
    assert!(p3[0].pixel_src.width < 2560, "Cropped horizontally only when wider than viewport");

    // 5. Horizontal pan h/l no-ops when page fits
    state.reader_state_mut().unwrap().reset_view();
    let moved = state.reader_state_mut().unwrap().scroll_horizontal_bounded(24, 0);
    assert!(!moved, "l is a no-op when page fits without overflow");

    // 6. Toggling layout 2 and back preserves fit basis
    state.reader_state_mut().unwrap().toggle_layout();
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Single);
    state.reader_state_mut().unwrap().toggle_layout();
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Spread);
    assert_eq!(state.reader_state().unwrap().default_fit, ReaderFitMode::FitPage);
}

#[test]
fn test_fractional_column_scroll_goldens_issue_26() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();
    let page_sizes = vec![(612.0, 792.0); 10];

    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);

    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let row_h = state
        .reader_state()
        .unwrap()
        .current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px);

    let initial_rows = state.reader_placements()[0].cell_rect.rows;
    let mut prev_src_y = 0u32;
    let mut prev_rows = initial_rows;

    // Step scroll_px by 1/8 of row_h and assert (cell_rect.row, src.y) at each step
    for step in 1..=7 {
        let scroll_offset = ((step as f32 * row_h / 8.0).round() as i64).max(1);
        state.reader_state_mut().unwrap().scroll_px = scroll_offset;

        terminal
            .draw(|f| {
                render_reader(&state, f, Rect::new(0, 0, 160, 45));
            })
            .unwrap();

        let placements = state.reader_placements();
        assert!(!placements.is_empty());
        let top_pane = &placements[0];

        // Row stays pinned to viewport top
        assert_eq!(top_pane.cell_rect.row, 0, "cell_rect.row stays at viewport top");
        // src.y advances with fractional offset
        assert!(top_pane.pixel_src.y > prev_src_y, "src.y advances as column scrolls");
        // rows shrinks as pane top is clipped
        assert!(top_pane.cell_rect.rows <= prev_rows, "rows shrinks while clipped");

        // Next row is partially visible below it with vertical gutter
        let next_row_placements: Vec<_> = placements.iter().filter(|p| p.page >= 2).collect();
        assert!(!next_row_placements.is_empty(), "Next row visible below clipped top row");
        assert_eq!(
            next_row_placements[0].cell_rect.row,
            top_pane.cell_rect.rows + crate::app::ui::reader::VERTICAL_GUTTER_ROWS
        );

        prev_src_y = top_pane.pixel_src.y;
        prev_rows = top_pane.cell_rect.rows;
    }

    // At step 8 (row_h), advance to next row
    let advanced = state.reader_state_mut().unwrap().scroll_column((row_h / 8.0).round() as i64, row_h);
    assert!(advanced);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    // Enter/PageDown/n snap to row tops (scroll_px == 0)
    state.reader_state_mut().unwrap().scroll_px = 50;
    handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);
    assert_eq!(state.reader_state().unwrap().anchor, 4);

    // g goes to column start, G goes to column end
    handle_key_event(make_key(KeyCode::Char('g')), &mut state);
    assert_eq!(state.reader_state().unwrap().anchor, 0);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    handle_key_event(make_key(KeyCode::Char('G')), &mut state);
    assert_eq!(state.reader_state().unwrap().anchor, 8);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);
}

#[test]
fn test_recording_driver_scroll_zero_re_render_issue_26() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();
    let page_sizes = vec![(612.0, 792.0); 10];

    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec { width_px: 2560, height_px: 3312 };

    // Initial render
    terminal.draw(|f| render_reader(&state, f, Rect::new(0, 0, 160, 45))).unwrap();
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target).unwrap();

    let initial_stages = log.lock().unwrap().staged.len();
    assert_eq!(initial_stages, 2, "Initially stages 2 spread pages");

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let row_h = state.reader_state().unwrap().current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px);

    // Step scroll within the row 4 times
    for step in 1..=4 {
        let offset = (step as f32 * row_h / 8.0).round() as i64;
        state.reader_state_mut().unwrap().scroll_px = offset;
        terminal.draw(|f| render_reader(&state, f, Rect::new(0, 0, 160, 45))).unwrap();
        syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target).unwrap();
    }

    // Zero stage calls for new bitmaps of existing visible panes on pure movement
    let re_stages_of_existing = log.lock().unwrap().staged.iter().filter(|&&id| id == 1 || id == 2).count();
    assert_eq!(re_stages_of_existing, 2, "Zero re-stages for initial pages during scroll");
}

#[test]
fn test_dynamic_n_page_reflow_issue_29() {
    let page_sizes = vec![(612.0, 792.0); 10];

    // 1. Narrow terminal (80 cols) -> reflows to 1 page
    let backend_narrow = TestBackend::new(80, 45);
    let mut term_narrow = Terminal::new(backend_narrow).unwrap();
    let mut state_narrow = PresentationState::new(10, vec![]);
    state_narrow.enable_reader_mode(10, page_sizes.clone());

    term_narrow
        .draw(|f| {
            render_reader(&state_narrow, f, Rect::new(0, 0, 80, 45));
        })
        .unwrap();

    let p_narrow = state_narrow.reader_placements();
    let primary_narrow: Vec<_> = p_narrow.iter().filter(|p| p.cell_rect.row == 0).collect();
    assert_eq!(primary_narrow.len(), 1, "Narrow terminal reflows to 1 page in row 0");

    // 2. Wide terminal (240 cols) -> fits 3 pages
    let backend_wide = TestBackend::new(240, 45);
    let mut term_wide = Terminal::new(backend_wide).unwrap();
    let mut state_wide = PresentationState::new(10, vec![]);
    state_wide.enable_reader_mode(10, page_sizes.clone());

    term_wide
        .draw(|f| {
            render_reader(&state_wide, f, Rect::new(0, 0, 240, 45));
        })
        .unwrap();

    let p_wide = state_wide.reader_placements();
    let primary_wide: Vec<_> = p_wide.iter().filter(|p| p.cell_rect.row == 0).collect();
    assert_eq!(primary_wide.len(), 3, "Wide terminal reflows to 3 pages in row 0");
}

#[test]
fn test_horizontal_mode_sliding_issue_29() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();
    let page_sizes = vec![(612.0, 792.0); 10];

    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);
    state.reader_state_mut().unwrap().toggle_horizontal();
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Horizontal);

    // Initial draw
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_init = state.reader_placements();
    assert!(p_init.len() >= 2, "Multiple pages laid out horizontally");
    assert_eq!(p_init[0].cell_rect.col, 0);
    assert_eq!(p_init[0].pixel_src.x, 0);

    // Smooth horizontal slide right
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(state.reader_state().unwrap().scroll_px > 0, "Right arrow slides smoothly");

    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_scrolled = state.reader_placements();
    assert!(p_scrolled[0].pixel_src.x > 0, "Pixel src.x shifted right");

    // Smooth horizontal slide left
    handle_key_event(make_key(KeyCode::Left), &mut state);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0, "Left arrow slides back to origin");
}

#[test]
fn test_zoomed_spread_horizontal_row_slide_arrow_keys() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();
    let page_sizes = vec![(612.0, 792.0); 10];

    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);

    // Initial draw in fit mode (zoom = 1.0)
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_fit = state.reader_placements();
    assert_eq!(p_fit.len(), 2);
    assert_eq!(p_fit[1].cell_rect.col, 70);

    // Zoom in 1.5x so the row overflows viewport width
    state.reader_state_mut().unwrap().adjust_zoom(1.5);
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_zoomed = state.reader_placements();
    assert_eq!(p_zoomed.len(), 2);
    let right_col_initial = p_zoomed[1].cell_rect.col;
    assert_eq!(right_col_initial, 104);

    // Press Right arrow key -> should slide row horizontally to reveal right page
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(
        state.reader_state().unwrap().row_scroll_x() > 0,
        "Right arrow increases row_scroll_x when row overflows"
    );

    // Re-draw with updated row_scroll_x
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_slid = state.reader_placements();
    assert!(
        p_slid[1].cell_rect.col < right_col_initial,
        "Right pane slid leftwards to become more visible"
    );

    // Press Left arrow key -> should slide back left towards origin
    handle_key_event(make_key(KeyCode::Left), &mut state);
    terminal
        .draw(|f| {
            render_reader(&state, f, Rect::new(0, 0, 160, 45));
        })
        .unwrap();

    let p_slid_back = state.reader_placements();
    assert!(
        p_slid_back[1].cell_rect.col > p_slid[1].cell_rect.col,
        "Left arrow slides panes back rightwards"
    );
}

#[test]
fn test_dynamic_columns_and_row_advancement() {
    let page_sizes = vec![(612.0, 792.0); 10];

    // Wide terminal (240 cols) -> 3 columns
    let backend_wide = TestBackend::new(240, 45);
    let mut term_wide = Terminal::new(backend_wide).unwrap();
    let mut state_wide = PresentationState::new(10, vec![]);
    state_wide.enable_reader_mode(10, page_sizes.clone());

    term_wide
        .draw(|f| {
            render_reader(&state_wide, f, Rect::new(0, 0, 240, 45));
        })
        .unwrap();

    assert_eq!(state_wide.reader_num_columns(), 3);
    assert_eq!(state_wide.reader_state().unwrap().num_columns(), 3);

    // Advance row via PageDown
    handle_key_event(make_key(KeyCode::PageDown), &mut state_wide);
    assert_eq!(
        state_wide.reader_state().unwrap().anchor,
        3,
        "Anchor advances by num_columns (3)"
    );

    // Narrow terminal (80 cols) -> 1 column
    let backend_narrow = TestBackend::new(80, 45);
    let mut term_narrow = Terminal::new(backend_narrow).unwrap();
    let mut state_narrow = PresentationState::new(10, vec![]);
    state_narrow.enable_reader_mode(10, page_sizes);

    term_narrow
        .draw(|f| {
            render_reader(&state_narrow, f, Rect::new(0, 0, 80, 45));
        })
        .unwrap();

    assert_eq!(state_narrow.reader_num_columns(), 1);
    assert_eq!(state_narrow.reader_state().unwrap().num_columns(), 1);

    // Advance row via PageDown
    handle_key_event(make_key(KeyCode::PageDown), &mut state_narrow);
    assert_eq!(
        state_narrow.reader_state().unwrap().anchor,
        1,
        "Anchor advances by num_columns (1)"
    );
}

#[test]
fn test_horizontal_mode_zero_restaging_on_advance() {
    let backend = TestBackend::new(160, 45);
    let mut terminal = Terminal::new(backend).unwrap();
    let page_sizes = vec![(612.0, 792.0); 10];

    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, page_sizes);
    state.reader_state_mut().unwrap().toggle_horizontal();

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec { width_px: 2560, height_px: 3312 };

    // Initial draw and sync
    terminal.draw(|f| render_reader(&state, f, Rect::new(0, 0, 160, 45))).unwrap();
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target).unwrap();

    let initial_staged = log.lock().unwrap().staged.clone();
    assert!(!initial_staged.is_empty());

    // Advance anchor by 1 page
    handle_key_event(make_key(KeyCode::PageDown), &mut state);
    terminal.draw(|f| render_reader(&state, f, Rect::new(0, 0, 160, 45))).unwrap();
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target).unwrap();

    let post_staged = log.lock().unwrap().staged.clone();
    // Image IDs for pages that were already visible/staged should NOT appear a second time in staged list
    for id in &initial_staged {
        let count = post_staged.iter().filter(|x| *x == id).count();
        assert_eq!(count, 1, "ImageId {} was not re-staged on page shift", id);
    }
}
