//! Mixed-orientation row layout goldens for Reader mode (PRD 14a §6.4).
//!
//! A document whose pages mix portrait and landscape (rotated figures,
//! native-landscape inserts) must read as ONE stable column: rows pair
//! consecutive pages on a per-document grid, a landscape page shares its row
//! with the next page instead of claiming the whole viewport, and the row
//! peeking in from below during a continuous scroll is exactly the row the
//! anchor will flip to — nothing vanishes or jumps sideways at the flip.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::events::handle_key_event;
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

/// The demo document's orientation mix: letter portrait pages with rotated
/// landscape figures at 2, 7 and 10, native landscape at 4, and a
/// landscape-box page rotated back to portrait at 5.
fn page_sizes() -> Vec<(f32, f32)> {
    let p = (612.0, 792.0); // letter portrait
    let l = (792.0, 612.0); // letter landscape (post-rotation display size)
    vec![p, p, l, p, l, p, p, l, p, l, p, p]
}

fn reader_fixture(pages: Vec<(f32, f32)>) -> PresentationState {
    let total = pages.len();
    let mut state = PresentationState::new(total, vec![None; total]);
    state.enable_reader_mode(total, pages);
    state.set_layout_dimensions(WIDTH, HEIGHT);
    state
}

/// Draws one reader frame and returns the frame's placement bindings.
fn draw_reader(state: &mut PresentationState) -> FrameBindings {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, WIDTH, HEIGHT), &mut bindings))
        .unwrap();
    let snapshot = bindings.clone();
    state.publish_frame_bindings(bindings);
    snapshot
}

#[test]
fn test_flip_continuity_across_orientation_change() {
    let mut state = reader_fixture(page_sizes());

    // Establish the published column plan from a rest frame.
    let b_rest = draw_reader(&mut state);
    assert_eq!(
        b_rest.reader.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![0, 1],
        "portrait-majority document starts as a 2-up row"
    );

    // Scroll deep into row [0,1] so the next row peeks in from below.
    let stride = {
        let reader = state.reader_state().unwrap();
        let (_, viewport_h_px, cell_w_px, cell_h_px) = crate::app::reader_types::reader_viewport_metrics(WIDTH, HEIGHT);
        reader.row_stride_px(
            viewport_h_px,
            reader.slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px),
            cell_w_px,
            cell_h_px,
        )
    };
    state.reader_state_mut().unwrap().scroll_px = (stride - 40.0) as i64;
    while state.reader_state().unwrap().advance_display() {}
    let b_peek = draw_reader(&mut state);

    let peek_row: Vec<&crate::app::reader_types::ReaderSlotPlacement> =
        b_peek.reader.iter().filter(|p| !p.is_primary).collect();
    let peek_pages: Vec<usize> = peek_row.iter().map(|p| p.page).collect();
    assert!(!peek_pages.is_empty(), "the incoming row must be on screen");
    assert!(
        peek_pages.contains(&2),
        "the landscape page 3 must be part of the incoming row: {peek_pages:?}"
    );

    // Cross the boundary exactly like continuous scrolling does, and let the
    // smoothed display converge onto the new anchor.
    state.reader_state_mut().unwrap().scroll_px = stride as i64;
    let geo = {
        let (_, viewport_h_px, cell_w_px, cell_h_px) = crate::app::reader_types::reader_viewport_metrics(WIDTH, HEIGHT);
        crate::app::reader_scroll::RowGeometry {
            viewport_h_px,
            slot_w_px: state
                .reader_state()
                .unwrap()
                .slot_width(WIDTH as f32 * cell_w_px, viewport_h_px, cell_w_px),
            cell_w_px,
            cell_h_px,
        }
    };
    let _ = state
        .reader_state_mut()
        .unwrap()
        .scroll_column_detailed(0, stride, stride - 40.0, geo);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    while state.reader_state().unwrap().advance_display() {}
    let b_flipped = draw_reader(&mut state);
    let flipped_row: Vec<&crate::app::reader_types::ReaderSlotPlacement> =
        b_flipped.reader.iter().filter(|p| p.is_primary).collect();
    let flipped_pages: Vec<usize> = flipped_row.iter().map(|p| p.page).collect();

    assert_eq!(
        peek_pages, flipped_pages,
        "the anchor flip must not change which pages are on screen"
    );
    // Between the two compared frames the scroll advanced exactly 40 px, so
    // every pane may only have glided up by that distance: same pages, same
    // columns, same crop, row delta bounded by the scrolled distance.
    let (_, _, _, cell_h_px) = crate::app::reader_types::reader_viewport_metrics(WIDTH, HEIGHT);
    let glided_rows = (40.0f32 / cell_h_px).ceil() as u16;
    for (peek, flipped) in peek_row.iter().zip(flipped_row.iter()) {
        assert_eq!(
            peek.cell_rect.col, flipped.cell_rect.col,
            "the flip must not shuffle any pane sideways (page {})",
            peek.page
        );
        assert_eq!(
            peek.cell_rect.cols, flipped.cell_rect.cols,
            "the flip must not resize any pane (page {})",
            peek.page
        );
        assert_eq!(
            peek.pixel_src.x, flipped.pixel_src.x,
            "the flip must not change the horizontal crop (page {})",
            peek.page
        );
        assert_eq!(
            peek.pixel_src.width, flipped.pixel_src.width,
            "the flip must not change the crop width (page {})",
            peek.page
        );
        let glide = peek.cell_rect.row.saturating_sub(flipped.cell_rect.row);
        assert!(
            glide <= glided_rows,
            "page {} must glide, not jump (row {} -> {})",
            peek.page,
            peek.cell_rect.row,
            flipped.cell_rect.row
        );
    }
}

#[test]
fn test_landscape_page_pairs_with_the_next_page() {
    let mut state = reader_fixture(page_sizes());

    // Row over the landscape page 3 (0-based 2): it must share the row with
    // the following portrait page, contained in its column box, not claim
    // the whole viewport.
    state.reader_state_mut().unwrap().anchor = 2;
    let b = draw_reader(&mut state);

    let pages: Vec<usize> = b.reader.iter().map(|p| p.page).collect();
    assert_eq!(
        pages,
        vec![2, 3],
        "a landscape page pairs with the following page on the stable grid"
    );
    let landscape = &b.reader[0];
    assert!(
        landscape.cell_rect.cols < 80,
        "the landscape pane stays in its column box (got {} cols)",
        landscape.cell_rect.cols
    );
    assert_eq!(b.reader_columns, 2, "the row plan keeps two columns");
}

#[test]
fn test_rows_align_on_one_stable_grid() {
    let mut state = reader_fixture(page_sizes());

    // Consecutive rows must sit at identical x positions so vertical
    // scrolling glides without sideways shuffling.
    let mut prev_cols: Option<Vec<(usize, u16)>> = None;
    for anchor in [0usize, 2, 4, 6, 8, 10] {
        state.reader_state_mut().unwrap().anchor = anchor;
        let b = draw_reader(&mut state);
        let cols: Vec<(usize, u16)> = b.reader.iter().map(|p| (p.page, p.cell_rect.col)).collect();
        if let Some(prev) = prev_cols.replace(cols.clone()) {
            let left_cols: Vec<u16> = cols.iter().map(|c| c.1).collect();
            let prev_left: Vec<u16> = prev.iter().map(|c| c.1).collect();
            assert_eq!(
                left_cols, prev_left,
                "anchor {anchor}: every row of the document shares one column grid"
            );
        }
    }
}

#[test]
fn test_scroll_together_through_the_orientation_change() {
    let mut state = reader_fixture(page_sizes());

    // Continuous j-scrolling across the first boundary: every frame keeps
    // at least one of the visible pages anchored at the same cell columns —
    // pages glide, nothing teleports.
    let mut prev: Option<Vec<(usize, u16, u16)>> = None; // (page, col, row)
    for _ in 0..24 {
        handle_key_event(make_key(KeyCode::Char('j')), &mut state);
        while state.reader_state().unwrap().advance_display() {}
        let b = draw_reader(&mut state);
        let cur: Vec<(usize, u16, u16)> = b
            .reader
            .iter()
            .map(|p| (p.page, p.cell_rect.col, p.cell_rect.row))
            .collect();
        if let Some(p) = prev.replace(cur.clone()) {
            let shared: Vec<(usize, (u16, u16))> = p
                .iter()
                .filter_map(|(page, _col, _row)| cur.iter().find(|c| c.0 == *page).map(|c| (*page, (c.1, c.2))))
                .collect();
            // Every page still on screen must have moved at most one row
            // up (a j step is 1/8 of the stride) and kept its column.
            for (page, (col, row)) in shared {
                let (_, prev_col, prev_row) = p.iter().find(|c| c.0 == page).unwrap();
                assert_eq!(col, *prev_col, "page {page} must not shuffle sideways while scrolling");
                assert!(
                    row <= *prev_row,
                    "page {page} must only glide upward (row {prev_row} -> {row})"
                );
                assert!(
                    prev_row.saturating_sub(row) <= 6,
                    "page {page} glides smoothly (row {prev_row} -> {row})"
                );
            }
        }
    }
}
