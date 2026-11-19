//! Placement of the reader focus marker.
//!
//! The focused pane of the primary row is underlined so focus is visible
//! without the status line (PRD 14a §4). The marker used to be
//! `Borders::BOTTOM` on the pane's own rect: a Cyan `─` run painted over the
//! page's bottom row that rode up through the content while scrolling —
//! reported as "a horizontal line at the page bottom, either left or right
//! page". These tests pin the corrected placement: the marker lives in the
//! first free row *below* the focused pane (the gutter between layout
//! bands), never inside any pane, and it disappears when no free row exists.

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::reader_state::ReaderSlotPlacement;
use crate::app::ui::reader::render_reader;
use crate::app::state::PresentationState;

// 60 cols: the letter pane fit-height lands below the viewport bottom, so a
// free gutter row exists under the pane (at 120 cols the single-column pane
// fills the viewport and the marker is legitimately suppressed).
const WIDTH: u16 = 60;
const HEIGHT: u16 = 45;

fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![None; pages]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0); pages]);
    state
}

/// One drawn frame: the placed panes plus the rendered cell buffer.
fn draw(state: &mut PresentationState) -> (Vec<ReaderSlotPlacement>, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
        .unwrap();
    let panes = bindings.reader.clone();
    let buffer = terminal.backend().buffer().clone();
    (panes, buffer)
}

/// Glyph cells of `symbol` in the buffer, as (row, col) pairs.
fn glyph_cells(buffer: &ratatui::buffer::Buffer, symbol: &str) -> Vec<(u16, u16)> {
    let mut hits = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                if cell.symbol() == symbol {
                    hits.push((y, x));
                }
            }
        }
    }
    hits
}

/// Whether a pane's cell rect contains (row, col).
fn pane_contains(panes: &[ReaderSlotPlacement], row: u16, col: u16) -> bool {
    panes.iter().any(|p| {
        let r = p.cell_rect;
        row >= r.row && row < r.row.saturating_add(r.rows) && col >= r.col && col < r.col.saturating_add(r.cols)
    })
}

fn is_cyan_bold(buffer: &ratatui::buffer::Buffer, row: u16, col: u16) -> bool {
    buffer
        .cell((col, row))
        .is_some_and(|cell| cell.fg == Color::Cyan && cell.modifier.contains(Modifier::BOLD))
}

#[test]
fn the_focus_marker_lives_below_the_focused_pane_not_on_it() {
    let mut state = reader_state(30);
    let (panes, buffer) = draw(&mut state);
    assert!(!panes.is_empty(), "the frame places panes");

    // No marker glyph may sit inside any pane — that was the defect: a
    // Cyan line over the page's bottom row.
    for (row, col) in glyph_cells(&buffer, "─") {
        assert!(
            !pane_contains(&panes, row, col),
            "a focus marker glyph at ({row},{col}) overlaps a pane"
        );
    }

    // The focused pane (left, page 0) is underlined in the free gutter row
    // right below it, in Cyan + bold, spanning exactly its columns.
    let focused = state
        .reader_state()
        .expect("reader state")
        .focused_page();
    let pane = panes
        .iter()
        .find(|p| p.page == focused)
        .expect("the focused pane is placed");
    let marker_row = pane.cell_rect.row + pane.cell_rect.rows;
    for col in pane.cell_rect.col..pane.cell_rect.col + pane.cell_rect.cols {
        assert!(
            is_cyan_bold(&buffer, marker_row, col),
            "the gutter row below the focused pane must carry the marker at ({marker_row},{col})"
        );
    }
}

#[test]
fn the_marker_tracks_the_pane_bottom_while_scrolling() {
    let mut state = reader_state(30);
    // Scroll deep into the row so the pane is mid-viewport and shrinking.
    {
        let reader = state.reader_state_mut().unwrap();
        let (_, viewport_h_px, cell_w_px, cell_h_px) =
            crate::app::reader_types::reader_viewport_metrics(WIDTH, HEIGHT);
        let slot_w = reader.slot_width(WIDTH as f32 * cell_w_px, cell_w_px);
        let stride = reader.row_stride_px(viewport_h_px, slot_w, cell_w_px, cell_h_px);
        reader.scroll_px = (stride / 2.0) as i64;
        reader.snap_display();
    }
    let (panes, buffer) = draw(&mut state);

    // Still never inside a pane…
    for (row, col) in glyph_cells(&buffer, "─") {
        assert!(
            !pane_contains(&panes, row, col),
            "marker glyph at ({row},{col}) overlaps a pane while scrolling"
        );
    }
    // …and still under the focused pane, wherever its bottom now is.
    let focused = state.reader_state().unwrap().focused_page();
    let pane = panes.iter().find(|p| p.page == focused).expect("focused pane placed");
    let marker_row = pane.cell_rect.row + pane.cell_rect.rows;
    let mid_col = pane.cell_rect.col + pane.cell_rect.cols / 2;
    assert!(
        is_cyan_bold(&buffer, marker_row, mid_col),
        "the marker follows the pane bottom: expected at ({marker_row},{mid_col})"
    );
}

#[test]
fn the_marker_hides_when_the_pane_fills_the_viewport() {
    let mut state = reader_state(30);
    // Zoom the focused page past the viewport: the pane occupies every
    // available row, so there is no free row below it.
    {
        let reader = state.reader_state_mut().unwrap();
        reader.cycle_fit(); // FitWidth: pane height grows past the viewport
        reader.adjust_zoom(4.0);
        reader.snap_display();
    }
    let (panes, buffer) = draw(&mut state);
    let focused = state.reader_state().unwrap().focused_page();
    let pane = panes.iter().find(|p| p.page == focused).expect("focused pane placed");
    let marker_row = pane.cell_rect.row + pane.cell_rect.rows;

    let viewport_height = HEIGHT.saturating_sub(1);
    if marker_row >= viewport_height {
        // No room: the marker must be entirely absent, not clipped over
        // the status line or another pane.
        for (row, col) in glyph_cells(&buffer, "─") {
            let in_our_columns = col >= pane.cell_rect.col && col < pane.cell_rect.col + pane.cell_rect.cols;
            assert!(
                !(in_our_columns && row < viewport_height),
                "a marker appeared with no free row at ({row},{col})"
            );
        }
    }
}
