use office_terminal_tui::app::state::PresentationState;
use ratatui::backend::TestBackend;

#[test]
fn test_reproduce_search_modal_corrupt_entries_and_borders() {
    let mut state = PresentationState::new(10, vec![]);
    // Simulate TOC entry with UTF-16BE decoded via from_utf8_lossy (containing NUL bytes and replacement chars)
    let bad_toc = vec![
        office_terminal_core::TocEntry {
            title: String::from_utf8_lossy(&[
                239, 191, 189, 239, 191, 189, 0, 80, 0, 114, 0, 101, 0, 102, 0, 97, 0, 99, 0, 101,
            ])
            .into_owned(),
            page_index: 15,
            level: 0,
        },
        office_terminal_core::TocEntry {
            title: String::from_utf8_lossy(&[
                239, 191, 189, 239, 191, 189, 0, 67, 0, 104, 0, 111, 0, 111, 0, 115, 0, 105, 0, 110, 0, 103,
            ])
            .into_owned(),
            page_index: 22,
            level: 1,
        },
    ];
    state.set_toc(&bad_toc);
    state.open_search();

    let backend = TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            office_terminal_tui::app::ui::render_search_modal(&state, frame, frame.area());
        })
        .expect("draw search modal");

    let buf = terminal.backend().buffer().clone();

    // Verify right border is aligned on all box rows (col 75)
    let border_r = 75;
    for y in 2..=21 {
        let sym = buf[(border_r, y)].symbol();
        assert!(
            sym == "┐" || sym == "│" || sym == "┘",
            "Right border at row {} col {} must be border char, got {:?}",
            y,
            border_r,
            sym
        );
    }

    // Verify entries sanitized: no NUL bytes or replacement characters
    for entry in state.search_state().entries() {
        assert!(
            !entry.title.contains('\0') && !entry.title.contains('\u{FFFD}'),
            "Entry title must not contain raw NUL bytes or UTF-16 replacement characters! Found: {:?}",
            entry.title
        );
    }
}
