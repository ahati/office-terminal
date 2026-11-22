//! Reader status-line goldens (PRD 14a §8/statusbar): the label reports the
//! primary row's pages, never preview rows, and tracks the zoom reflow's
//! column plan.

use office_terminal_tui::app::bindings::FrameBindings;
use office_terminal_tui::app::reader_state::ReaderState;
use office_terminal_tui::app::state::PresentationState;
use office_terminal_tui::app::ui::render_reader;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

/// Reader-mode state over `pages` letter-sized pages.
fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0); pages]);
    state
}

/// Draws one reader frame at `w` x `h` and publishes the bindings like the
/// run loop.
fn draw_reader(
    state: &mut PresentationState,
    terminal: &mut Terminal<TestBackend>,
    w: u16,
    h: u16,
) {
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_reader(state, f, Rect::new(0, 0, w, h), &mut bindings))
        .unwrap();
    state.publish_frame_bindings(bindings);
}

#[test]
fn test_reader_status_line_format() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);
    reader.next_row(); // spread [2, 3] -> 1-based "spread 3-4"

    let status = reader.status_line("report.pdf", &[], 2);
    // Full-line golden: everything is a segment joined by the one `·`
    // policy — the exact separator contract.
    assert_eq!(
        status,
        "report.pdf · spread 3-4/10 · fit:page 100% · ?:help · q:quit"
    );
}

#[test]
fn test_status_line_uses_primary_placements_at_zoom_0_5() {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut state = reader_state(10);

    // Zoom out to 0.5: the reflow packs four columns per row and multiple
    // rows fit the viewport.
    state.reader_state_mut().unwrap().adjust_zoom(0.5);

    draw_reader(&mut state, &mut terminal, 160, 45);

    let placements = state.reader_placements_last();
    assert!(
        placements.len() > 2,
        "At zoom 0.5, multiple rows (primary + next) are placed in the viewport"
    );

    let reader = state.reader_state().unwrap();
    let status = reader.status_line("test.pdf", placements, state.reader_num_columns());
    assert!(
        status.contains("spread 1-4"),
        "Status line must report the primary 4-up row 1-4, got: {}",
        status
    );
    assert!(
        !status.contains("spread 1-8"),
        "Status line must NOT include the preview row (pages 5-8) in the label"
    );
}

#[test]
fn the_optional_cover_segment_joins_like_any_other() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);
    reader.cover_alone = true;

    let status = reader.status_line("book.pdf", &[], 2);
    assert_eq!(
        status,
        "book.pdf · p.1/10 · fit:page 100% · cover:on · ?:help · q:quit",
        "cover:on and the hints join like every other segment; one separator policy"
    );
}
