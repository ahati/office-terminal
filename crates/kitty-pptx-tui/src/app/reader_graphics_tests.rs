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

use crate::app::bindings::FrameBindings;
use crate::app::events::handle_key_event;
use crate::app::graphics::GraphicsSyncer;
use crate::app::reader_state::{ReaderFitMode, ReaderLayout};
use crate::app::reader_types::page_image_id;
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

/// Fresh `width`×45 terminal plus a reader-mode state over `page_sizes`.
fn reader_fixture(
    width: u16,
    total_pages: usize,
    page_sizes: Vec<(f32, f32)>,
) -> (Terminal<TestBackend>, PresentationState) {
    let terminal = Terminal::new(TestBackend::new(width, 45)).unwrap();
    let mut state = PresentationState::new(total_pages, vec![]);
    state.enable_reader_mode(total_pages, page_sizes);
    (terminal, state)
}

/// Draws one reader frame at `width`×45, publishes the frame bindings onto
/// the state exactly like the run loop, and returns them for geometry
/// assertions.
fn draw_reader(
    state: &mut PresentationState,
    terminal: &mut Terminal<TestBackend>,
    width: u16,
) -> FrameBindings {
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, width, 45), &mut bindings))
        .unwrap();
    let snapshot = bindings.clone();
    state.publish_frame_bindings(bindings);
    snapshot
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
        }
    }

    fn entry_at(&self, _index: usize, _target: &RenderTargetSpec) -> PipelineEntry {
        PipelineEntry::Ready {
            bitmap: test_page_bitmap(),
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
    let page_sizes = vec![(612.0, 792.0); 10]; // Letter / A4-proportional (ar = 0.7727)

    // 1. Zoom 1.0 (Fit)
    let (mut terminal, mut state) = reader_fixture(160, 10, page_sizes.clone());

    let b1 = draw_reader(&mut state, &mut terminal, 160);

    let p1 = &b1.reader;
    assert_eq!(p1.len(), 2, "Both spread pages visible at zoom 1.0");
    assert_eq!(p1[0].cell_rect.cols, 68, "Left pane fits at 68 cols");
    assert_eq!(
        p1[0].cell_rect.col, 11,
        "Row centered: the 138-col row gets (160 - 138) / 2 = 11 margin cols"
    );
    assert_eq!(p1[0].pixel_src.width, 1280, "Left pane full page width at fit (1280 class)");
    assert_eq!(p1[1].cell_rect.col, 81, "Right starts at col 81 (11 + 68 + 2 gutter)");
    assert_eq!(p1[1].cell_rect.cols, 68, "Right pane fits at 68 cols");
    assert_eq!(p1[1].pixel_src.width, 1280, "Right pane full page width (1280 class)");

    // 2. Zoom 1.5 (Reflows to a Single Column)
    state.reader_state_mut().unwrap().adjust_zoom(1.5);
    let b15 = draw_reader(&mut state, &mut terminal, 160);

    let p15 = &b15.reader;
    assert_eq!(p15.len(), 1, "Zoom reflows the 2-up row to one column");
    assert_eq!(p15[0].cell_rect.cols, 102, "Pane keeps its zoomed size (68 x 1.5)");
    assert_eq!(p15[0].cell_rect.col, 29, "Reflowed pane centers: (160 - 102) / 2");
    assert_eq!(p15[0].pixel_src.width, 1920, "zoom 1.5 snaps up to the 1920 class");

    // 3. Zoom 2.0 (FitWidth, Single Column)
    state.reader_state_mut().unwrap().reset_view();
    state.reader_state_mut().unwrap().cycle_fit(); // FitWidth
    state.reader_state_mut().unwrap().adjust_zoom(2.0);
    let b2 = draw_reader(&mut state, &mut terminal, 160);

    let p2 = &b2.reader;
    assert_eq!(p2[0].cell_rect.cols, 160, "Fit-width zoom 2 spans the full viewport");
    assert_eq!(p2[0].page_target.width_px, 3840, "zoom 2.0 hits the 3840 class cap");
    assert_eq!(p2[0].pixel_src.width, 1920, "Viewport window is 1600 px x 1.2 scale");
    assert_eq!(p2.len(), 1, "Zoom reflow keeps a single column");

    // 4. Zoom 3.0 (Clamped to Viewport with max_scroll_x > 0)
    state.reader_state_mut().unwrap().adjust_zoom(1.5); // 2.0 * 1.5 = 3.0
    let b3 = draw_reader(&mut state, &mut terminal, 160);

    let p3 = &b3.reader;
    assert_eq!(p3[0].cell_rect.cols, 160, "Clamped to viewport width");
    assert_eq!(p3.len(), 1, "Partner pushed out");
    assert!(p3[0].max_scroll_x > 0, "max_scroll_x > 0 when zoomed past viewport");
    assert!(
        p3[0].pixel_src.width < p3[0].page_target.width_px,
        "Cropped horizontally only when wider than viewport"
    );

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
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);

    let b = draw_reader(&mut state, &mut terminal, 160);

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let row_h = state
        .reader_state()
        .unwrap()
        .current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px);

    let initial_rows = b.reader[0].cell_rect.rows;
    let mut prev_src_y = 0u32;
    let mut prev_rows = initial_rows;

    // Step scroll_px by 1/8 of row_h and assert (cell_rect.row, src.y) at each step
    for step in 1..=7 {
        let scroll_offset = ((step as f32 * row_h / 8.0).round() as i64).max(1);
        state.reader_state_mut().unwrap().scroll_px = scroll_offset;
        // Intentional smoothing change: the renderer animates toward
        // scroll_px, so converge the displayed value before asserting
        // geometry at the requested offset.
        while state.reader_state().unwrap().advance_display() {}

        let b = draw_reader(&mut state, &mut terminal, 160);
        let placements = &b.reader;
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
    let advanced = state.reader_state_mut().unwrap().scroll_column((row_h / 8.0).round() as i64, row_h, row_h);
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
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec { width_px: 2560, height_px: 3312 };

    // Initial render
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    let initial_stages = log.lock().unwrap().staged.len();
    assert_eq!(initial_stages, 2, "Initially stages 2 spread pages");
    // Intentional id-scheme change (stable per-page ids): the reader image
    // pool starts at READER_IMAGE_ID_BASE (81), so pages 0/1 stage as 81/82
    // instead of the old row-parity ids 1/2.
    assert!(log.lock().unwrap().staged.contains(&page_image_id(0).0));
    assert!(log.lock().unwrap().staged.contains(&page_image_id(1).0));

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let row_h = state.reader_state().unwrap().current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px);

    // Step scroll within the row 4 times
    for step in 1..=4 {
        let offset = (step as f32 * row_h / 8.0).round() as i64;
        state.reader_state_mut().unwrap().scroll_px = offset;
        let bindings = draw_reader(&mut state, &mut terminal, 160);
        syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();
    }

    // Zero stage calls for new bitmaps of existing visible panes on pure movement
    let (id_p0, id_p1) = (page_image_id(0).0, page_image_id(1).0);
    let re_stages_of_existing = log.lock().unwrap().staged.iter().filter(|&&id| id == id_p0 || id == id_p1).count();
    assert_eq!(re_stages_of_existing, 2, "Zero re-stages for initial pages during scroll");
}

#[test]
fn test_dynamic_n_page_reflow_issue_29() {
    // 1. Narrow terminal (80 cols) -> reflows to 1 page
    let (mut term_narrow, mut state_narrow) = reader_fixture(80, 10, vec![(612.0, 792.0); 10]);
    let b_narrow = draw_reader(&mut state_narrow, &mut term_narrow, 80);

    let p_narrow = &b_narrow.reader;
    let primary_narrow: Vec<_> = p_narrow.iter().filter(|p| p.cell_rect.row == 0).collect();
    assert_eq!(primary_narrow.len(), 1, "Narrow terminal reflows to 1 page in row 0");

    // 2. Wide terminal (240 cols) -> fits 3 pages
    let (mut term_wide, mut state_wide) = reader_fixture(240, 10, vec![(612.0, 792.0); 10]);
    let b_wide = draw_reader(&mut state_wide, &mut term_wide, 240);

    let p_wide = &b_wide.reader;
    let primary_wide: Vec<_> = p_wide.iter().filter(|p| p.cell_rect.row == 0).collect();
    assert_eq!(primary_wide.len(), 3, "Wide terminal reflows to 3 pages in row 0");
}

#[test]
fn test_zoomed_spread_horizontal_row_slide_arrow_keys() {
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);

    // Fit-page baseline (zoom = 1.0): centered 138-col row.
    let b_fit = draw_reader(&mut state, &mut terminal, 160);

    let p_fit = &b_fit.reader;
    assert_eq!(p_fit.len(), 2);
    assert_eq!(p_fit[1].cell_rect.col, 81, "Centered fit row: right pane at 11 + 68 + 2");

    // Fit-width: panes fill their 79-col slots and the row exactly fills
    // the viewport. The zoom reflow keeps two columns at moderate zoom
    // (the plan tracks the 68-col fit-page width: 68 * 1.1 = 75 ideal cols).
    state.reader_state_mut().unwrap().cycle_fit();
    let b_fw = draw_reader(&mut state, &mut terminal, 160);

    let p_fw = &b_fw.reader;
    assert_eq!(p_fw.len(), 2);
    assert_eq!(p_fw[0].cell_rect.col, 0);
    assert_eq!(p_fw[1].cell_rect.col, 81, "Fit-width pair: 79 + 2 = col 81");

    // Zoom 1.1: the two-column plan holds, but the slot-fit panes
    // (79 * 1.1 = 87 cols) overflow the viewport — the row pan window opens.
    state.reader_state_mut().unwrap().adjust_zoom(1.1);
    let b_zoomed = draw_reader(&mut state, &mut terminal, 160);

    let p_zoomed = &b_zoomed.reader;
    assert_eq!(p_zoomed.len(), 2, "Zoom 1.1 keeps the two-column plan");
    let right_col_initial = p_zoomed[1].cell_rect.col;
    assert_eq!(right_col_initial, 89, "87 + 2 = col 89; right pane clipped at the edge");

    // Press Right arrow key -> should slide row horizontally to reveal right page
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(
        state.reader_state().unwrap().row_scroll_x() > 0,
        "Right arrow increases row_scroll_x when row overflows"
    );

    // Re-draw with updated row_scroll_x
    let b_slid = draw_reader(&mut state, &mut terminal, 160);

    let p_slid = &b_slid.reader;
    assert!(
        p_slid[1].cell_rect.col < right_col_initial,
        "Right pane slid leftwards to become more visible"
    );

    // Press Left arrow key -> should slide back left towards origin
    handle_key_event(make_key(KeyCode::Left), &mut state);
    let b_slid_back = draw_reader(&mut state, &mut terminal, 160);

    let p_slid_back = &b_slid_back.reader;
    assert!(
        p_slid_back[1].cell_rect.col > p_slid[1].cell_rect.col,
        "Left arrow slides panes back rightwards"
    );
}

#[test]
fn test_dynamic_columns_and_row_advancement() {
    // Wide terminal (240 cols) -> 3 columns
    let (mut term_wide, mut state_wide) = reader_fixture(240, 10, vec![(612.0, 792.0); 10]);
    draw_reader(&mut state_wide, &mut term_wide, 240);

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
    let (mut term_narrow, mut state_narrow) = reader_fixture(80, 10, vec![(612.0, 792.0); 10]);
    draw_reader(&mut state_narrow, &mut term_narrow, 80);

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
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);
    state.reader_state_mut().unwrap().toggle_horizontal();

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec { width_px: 2560, height_px: 3312 };

    // Initial draw and sync
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    let initial_staged = log.lock().unwrap().staged.clone();
    assert!(!initial_staged.is_empty());

    // Advance anchor by 1 page
    handle_key_event(make_key(KeyCode::PageDown), &mut state);
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    let post_staged = log.lock().unwrap().staged.clone();
    // Image IDs for pages that were already visible/staged should NOT appear a second time in staged list
    for id in &initial_staged {
        let count = post_staged.iter().filter(|x| *x == id).count();
        assert_eq!(count, 1, "ImageId {} was not re-staged on page shift", id);
    }
}

#[test]
fn test_row_transition_zero_restage_stable_ids() {
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec { width_px: 2560, height_px: 3312 };

    // Initial draw and sync: spread [0, 1] stages under stable per-page ids.
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let (row_stride, base_h) = {
        let reader = state.reader_state().unwrap();
        (
            reader.current_row_stride_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px, cell_h_px),
            reader.current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px),
        )
    };

    // Scroll near the row boundary so pages 2,3 peek from below and stage.
    state.reader_state_mut().unwrap().scroll_px = (row_stride - 8.0).round() as i64;
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    let peek_placements: Vec<_> = bindings
        .reader
        .iter()
        .filter(|p| p.page >= 2)
        .collect();
    assert!(peek_placements.len() >= 2, "Next-row pages peek from below");
    for p in &peek_placements {
        assert_eq!(p.id, page_image_id(p.page), "pane ids are page-stable, not row-parity");
    }
    let staged_before = log.lock().unwrap().staged.len();
    let presented_before = log.lock().unwrap().presented.len();
    assert_eq!(staged_before, 4, "All four visible panes staged exactly once");

    // Cross the row boundary exactly like the j handler does: pages 2,3
    // become the primary row with the SAME stable ids.
    state.reader_state_mut().unwrap().scroll_px = (row_stride + 4.0).round() as i64;
    let _ = state
        .reader_state_mut()
        .unwrap()
        .scroll_column_detailed(0, row_stride, base_h);
    assert_eq!(state.reader_state().unwrap().anchor, 2, "Anchor advanced to the next row");

    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer.sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings).unwrap();

    assert_eq!(
        log.lock().unwrap().staged.len(),
        staged_before,
        "row transition re-places panes without re-transmitting them"
    );
    assert!(
        log.lock().unwrap().presented.len() > presented_before,
        "the moved panes were re-placed at their new cells"
    );
}
