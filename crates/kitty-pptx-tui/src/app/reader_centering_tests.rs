//! Row layout policy goldens for Reader mode (PRD 14a §6.1-§6.3/§6.6):
//! centering, zoom reflow, and horizontal rest insets.
//!
//! Pages narrower than the viewport must not pile their leftover space up
//! on the right: rows center as a unit (zoomed-in overflow keeps the
//! flush-left pan origin), the column plan re-evaluates on every zoom step
//! (the focused page surviving the reflow as the row lead), and the
//! horizontal stream rests its first and last pages centered, unwinding
//! the rest insets linearly with the slide.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::events::handle_key_event;
use crate::app::reader_state::ReaderLayout;
use crate::app::state::PresentationState;
use crate::app::ui::reader::render_reader;

const WIDTH: u16 = 160;
const HEIGHT: u16 = 45;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn reader_fixture(total_pages: usize) -> PresentationState {
    let mut state = PresentationState::new(total_pages, vec![None; total_pages]);
    state.enable_reader_mode(total_pages, vec![(612.0, 792.0); total_pages]);
    state
}

/// Draws one reader frame at `WIDTH`×`HEIGHT` and returns the frame's
/// placement bindings for geometry assertions.
fn draw_reader(state: &mut PresentationState) -> FrameBindings {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
        .unwrap();
    bindings
}

#[test]
fn test_reader_rows_centered_when_narrow() {
    // Spread, fit-page: the 68 + 2 + 68 = 138-col row centers in 160 cols.
    let mut state = reader_fixture(10);
    let b = draw_reader(&mut state);

    let p = &b.reader;
    assert_eq!(p.len(), 2);
    assert_eq!(
        p[0].cell_rect.col, 11,
        "(160 - 138) / 2 = 11 cols of left margin"
    );
    assert_eq!(p[1].cell_rect.col, 81);
    assert_eq!(
        WIDTH.saturating_sub(p[1].cell_rect.col + p[1].cell_rect.cols),
        p[0].cell_rect.col,
        "Spread margins are symmetric, not right-only"
    );

    // Single mode: the lone 68-col page centers with equal margins.
    state.reader_state_mut().unwrap().toggle_layout();
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Single);
    let b_single = draw_reader(&mut state);

    let p_single = &b_single.reader;
    assert_eq!(p_single.len(), 1);
    assert_eq!(p_single[0].cell_rect.cols, 68);
    assert_eq!(p_single[0].cell_rect.col, 46, "(160 - 68) / 2 = 46 cols");
    assert_eq!(
        WIDTH.saturating_sub(p_single[0].cell_rect.col + p_single[0].cell_rect.cols),
        46,
        "Single-page margins are symmetric"
    );

    // Zoomed out: the smaller page re-centers in the freed space.
    state.reader_state_mut().unwrap().adjust_zoom(0.5);
    let b_small = draw_reader(&mut state);

    let p_small = &b_small.reader;
    assert_eq!(
        p_small[0].cell_rect.cols, 34,
        "68-col page shrinks to 34 cols"
    );
    assert_eq!(p_small[0].cell_rect.col, 63, "(160 - 34) / 2 = 63 cols");

    // Zoomed past the viewport: the flush-left pan origin returns, with a
    // cropped window and a horizontal pan bound.
    state.reader_state_mut().unwrap().reset_view();
    state.reader_state_mut().unwrap().adjust_zoom(2.5);
    let b_zoom = draw_reader(&mut state);

    let p_zoom = &b_zoom.reader;
    assert_eq!(
        p_zoom[0].cell_rect.col, 0,
        "Overflowing rows keep the pan origin"
    );
    assert_eq!(p_zoom[0].cell_rect.cols, WIDTH);
    assert!(p_zoom[0].max_scroll_x > 0);
    assert!(
        p_zoom[0].pixel_src.width < p_zoom[0].page_target.width_px,
        "Overflowing pane is cropped to the viewport"
    );
}

#[test]
fn test_horizontal_mode_sliding_issue_29() {
    let mut state = reader_fixture(10);
    state.set_layout_dimensions(WIDTH, HEIGHT);
    state.reader_state_mut().unwrap().toggle_horizontal();
    assert_eq!(
        state.reader_state().unwrap().layout,
        ReaderLayout::Horizontal
    );

    // Initial draw: the first page rests centered ((1600 - 680) / 2 = 460 px
    // = 46 cols) and the stream still tiles to the right edge.
    let b_init = draw_reader(&mut state);

    let p_init = &b_init.reader;
    assert!(p_init.len() >= 2, "Multiple pages laid out horizontally");
    assert_eq!(p_init[0].cell_rect.col, 46, "First page rests centered");
    assert_eq!(p_init[0].pixel_src.x, 0);

    // Smooth horizontal slide right: the rest inset unwinds, so the page
    // slides left without its left edge being cropped.
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(
        state.reader_state().unwrap().scroll_px > 0,
        "Right arrow slides smoothly"
    );

    let p_scrolled = &draw_reader(&mut state).reader;
    assert!(
        p_scrolled[0].cell_rect.col < p_init[0].cell_rect.col,
        "Page slides left as the rest inset unwinds"
    );
    assert_eq!(
        p_scrolled[0].pixel_src.x, 0,
        "No left crop while the inset is active"
    );

    // Keep sliding right until the inset is spent: the stream goes
    // flush-left and the crop window opens.
    let mut crop_started = false;
    for _ in 0..8 {
        handle_key_event(make_key(KeyCode::Right), &mut state);
        while state.reader_state().unwrap().advance_display() {}
        let b_step = draw_reader(&mut state);
        if b_step.reader[0].pixel_src.x > 0 {
            crop_started = true;
            assert_eq!(
                b_step.reader[0].cell_rect.col, 0,
                "Stream flush-left once the rest inset is spent"
            );
            break;
        }
    }
    assert!(crop_started, "Crop window opens after the inset unwinds");

    // Smooth horizontal slide left, back to the centered start rest.
    let mut returned_to_rest = false;
    for _ in 0..24 {
        let (anchor, scroll) = {
            let reader = state.reader_state().unwrap();
            (reader.anchor, reader.scroll_px)
        };
        if anchor == 0 && scroll == 0 {
            returned_to_rest = true;
            break;
        }
        handle_key_event(make_key(KeyCode::Left), &mut state);
        while state.reader_state().unwrap().advance_display() {}
    }
    assert!(
        returned_to_rest,
        "Left arrows walk the stream back to the start"
    );
    let b_rest = draw_reader(&mut state);
    assert_eq!(
        b_rest.reader[0].cell_rect.col, 46,
        "Left arrow slides back to the centered rest"
    );
    assert_eq!(b_rest.reader[0].pixel_src.x, 0);
}

#[test]
fn test_horizontal_rest_centering() {
    let mut state = reader_fixture(10);
    state.set_layout_dimensions(WIDTH, HEIGHT);
    state.reader_state_mut().unwrap().toggle_horizontal();

    // Start rest: page 0 centered (460 px = 46 cols), stream tiling right.
    let b_start = draw_reader(&mut state);
    assert_eq!(b_start.reader[0].page, 0);
    assert_eq!(
        b_start.reader[0].cell_rect.col, 46,
        "First page rests centered"
    );
    assert_eq!(b_start.reader[0].pixel_src.x, 0);
    assert!(
        b_start.reader.len() >= 2,
        "Stream still fills the viewport rightward"
    );

    // G lands at the end clamp: the final page rests centered too.
    handle_key_event(make_key(KeyCode::Char('G')), &mut state);
    {
        let reader = state.reader_state().unwrap();
        assert_eq!(reader.anchor, 9);
        assert_eq!(
            reader.scroll_px, 679,
            "End jump lands at the clamp (680 - 1)"
        );
    }
    let b_end = draw_reader(&mut state);
    assert_eq!(b_end.reader.len(), 1, "Nothing follows the last page");
    assert_eq!(b_end.reader[0].page, 9);
    assert_eq!(
        b_end.reader[0].cell_rect.col, 46,
        "Last page rests centered"
    );
    assert_eq!(b_end.reader[0].pixel_src.x, 0);

    // Sliding left from the end winds the inset down: the page moves left
    // continuously instead of jumping.
    handle_key_event(make_key(KeyCode::Left), &mut state);
    let b_off = draw_reader(&mut state);
    assert!(
        b_off.reader[0].cell_rect.col < 46,
        "End inset winds down as the stream slides left"
    );
    assert_eq!(
        b_off.reader[0].pixel_src.x, 0,
        "No crop while the end inset is active"
    );
}

#[test]
fn test_zoom_reflow_columns_and_focus() {
    let mut state = reader_fixture(10);
    state.set_layout_dimensions(WIDTH, HEIGHT);

    // Baseline: 2-up spread [0, 1].
    let b = draw_reader(&mut state);
    assert_eq!(b.reader.len(), 2);

    // Focus the right pane (page 1). A 1.1x step still plans two columns
    // (68-col pages: two columns hold up to ~1.16x).
    state.reader_state_mut().unwrap().toggle_focus();
    assert_eq!(state.reader_state().unwrap().focused_page(), 1);
    handle_key_event(make_key(KeyCode::Char('+')), &mut state);
    assert_eq!(
        draw_reader(&mut state).reader.len(),
        2,
        "1.1x keeps the two-column plan"
    );

    // The second 1.1x step (1.21x) reflows to a single column: the focused
    // page becomes the row lead instead of scrolling off.
    handle_key_event(make_key(KeyCode::Char('+')), &mut state);
    {
        let reader = state.reader_state().unwrap();
        assert_eq!(reader.anchor, 1, "Focused page re-anchors the row");
        assert_eq!(reader.focused_page(), 1);
        assert_eq!(reader.scroll_px, 0, "The reflow snaps like a page turn");
    }
    let b_z2 = draw_reader(&mut state);
    assert_eq!(b_z2.reader.len(), 1, "Zoom reflows the row to one column");
    assert_eq!(b_z2.reader[0].page, 1);
    assert_eq!(b_z2.reader[0].cell_rect.cols, 82, "68 x 1.21 = 82 cols");
    assert_eq!(
        b_z2.reader[0].cell_rect.col, 39,
        "Reflowed single-pane row centers: (160 - 82) / 2"
    );

    // Zooming back out regains the partner column without losing the page.
    handle_key_event(make_key(KeyCode::Char('-')), &mut state);
    let b_out = draw_reader(&mut state);
    assert_eq!(b_out.reader.len(), 2, "A 0.9 step packs two columns again");
    assert_eq!(b_out.reader[0].page, 1);
    assert_eq!(b_out.reader[1].page, 2);
    assert_eq!(state.reader_state().unwrap().anchor, 1, "Anchor holds the focused page");
}
