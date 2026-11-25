//! The reader draws no in-content focus marker.
//!
//! Focus used to be underlined with a Cyan `─` run on the focused pane
//! (originally `Borders::BOTTOM` *on* the pane, later the gutter row below
//! it) — either way it read as a stray horizontal line at the page bottom,
//! on whichever pane held focus (left or right in a spread). Focus is
//! reported by the status line alone (`focus:L` / `focus:R`), and no
//! `─` glyph may appear anywhere in a reader frame, at rest or while
//! scrolling.

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::state::PresentationState;
use crate::app::ui::reader::render_reader;

// 60 cols: the letter pane leaves a free gutter row below it at rest, which
// is exactly where a marker would appear if one were drawn.
const WIDTH: u16 = 60;
const HEIGHT: u16 = 45;

fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![None; pages]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0); pages]);
    state
}

fn draw(state: &mut PresentationState) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
        .unwrap();
    assert!(
        !bindings.reader.is_empty(),
        "the fixture places panes; otherwise the assertion below proves nothing"
    );
    terminal.backend().buffer().clone()
}

fn assert_no_horizontal_rule(buffer: &ratatui::buffer::Buffer) {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                assert_ne!(
                    cell.symbol(),
                    "─",
                    "a horizontal-rule glyph at ({x},{y}): focus must stay on the status line"
                );
            }
        }
    }
}

#[test]
fn no_focus_line_at_rest() {
    let mut state = reader_state(30);
    assert_no_horizontal_rule(&draw(&mut state));
}

#[test]
fn no_focus_line_while_scrolling() {
    let mut state = reader_state(30);
    // Scroll to the middle of the row: the pane is mid-viewport, the state
    // a marker used to ride along with.
    {
        let reader = state.reader_state_mut().unwrap();
        let (_, viewport_h_px, cell_w_px, cell_h_px) = crate::app::reader_types::reader_viewport_metrics(WIDTH, HEIGHT);
        let slot_w = reader.slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px);
        let stride = reader.row_stride_px(viewport_h_px, slot_w, cell_w_px, cell_h_px);
        reader.scroll_px = (stride / 2.0) as i64;
        reader.snap_display();
    }
    assert_no_horizontal_rule(&draw(&mut state));
}

#[test]
fn no_focus_line_when_zoomed_or_on_either_slot() {
    // Focus either slot (Tab cycles) and zoom past the viewport: none of
    // those states may reintroduce a line.
    for (slot, zoom) in [(0usize, 1.0f32), (1, 1.0), (0, 4.0), (1, 4.0)] {
        let mut state = reader_state(30);
        let reader = state.reader_state_mut().unwrap();
        reader.focused_slot = slot;
        reader.cycle_fit();
        reader.adjust_zoom(zoom);
        reader.snap_display();
        assert_no_horizontal_rule(&draw(&mut state));
    }
}
