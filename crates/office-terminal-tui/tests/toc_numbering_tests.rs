//! TOC numbering and row-grammar tests for the `/` outline modal.
//!
//! Kept separate from `overlay_tests.rs` so both files stay under the
//! repository's 600-line cap.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use office_terminal_tui::app::bindings::FrameBindings;
use office_terminal_tui::app::events::{handle_key_event, AppAction};
use office_terminal_tui::app::state::PresentationState;
use office_terminal_tui::app::ui::render_app;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Flatten a rendered terminal buffer into one string per row.
fn buffer_lines(buf: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn three_level_toc() -> Vec<office_terminal_core::TocEntry> {
    vec![
        office_terminal_core::TocEntry {
            title: "Setup".to_string(),
            page_index: 0,
            level: 0,
        },
        office_terminal_core::TocEntry {
            title: "Sub Chapter".to_string(),
            page_index: 63,
            level: 1,
        },
        office_terminal_core::TocEntry {
            title: "Detail".to_string(),
            page_index: 65,
            level: 2,
        },
        office_terminal_core::TocEntry {
            title: "Results".to_string(),
            page_index: 89,
            level: 0,
        },
    ]
}

#[test]
fn test_toc_entries_carry_tree_numbers() {
    let mut state = PresentationState::new(120, vec![]);
    state.set_toc(&three_level_toc());

    let entries = state.search_state().entries();
    let numbers: Vec<String> = entries.iter().map(|entry| entry.number.clone()).collect();
    assert_eq!(numbers, vec!["1", "1.1", "1.1.1", "2"]);
    // Indentation depth is normalized alongside the number.
    let levels: Vec<usize> = entries.iter().map(|entry| entry.level).collect();
    assert_eq!(levels, vec![0, 1, 2, 0]);

    // Numbers are structural: collapsing the tree must not renumber anything.
    state.open_search();
    handle_key_event(make_key(KeyCode::Left), &mut state);
    let after: Vec<String> = state
        .search_state()
        .entries()
        .iter()
        .map(|entry| entry.number.clone())
        .collect();
    assert_eq!(after, numbers);
}

#[test]
fn test_toc_modal_renders_tree_number_before_trailing_page() {
    let mut state = PresentationState::new(120, vec![]);
    state.set_toc(&three_level_toc());
    state.open_search();

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|frame| render_app(&state, frame, &mut bindings))
        .expect("draw TOC modal");

    let lines = buffer_lines(terminal.backend().buffer());
    let row = lines
        .iter()
        .find(|line| line.contains("Sub Chapter"))
        .expect("TOC row rendered");
    assert!(row.contains("1.1 Sub Chapter"), "row: {row:?}");
    // The page column is flush with the modal's right border.
    assert!(row.contains("[64]│"), "page suffix missing: {row:?}");
}

#[test]
fn test_flat_toc_rows_keep_leading_page_prefix() {
    let mut state = PresentationState::new(4, vec![]);
    state.set_toc(&[
        office_terminal_core::TocEntry {
            title: "Intro".to_string(),
            page_index: 0,
            level: 0,
        },
        office_terminal_core::TocEntry {
            title: "Body".to_string(),
            page_index: 1,
            level: 0,
        },
    ]);
    assert!(
        state
            .search_state()
            .entries()
            .iter()
            .all(|entry| entry.number.is_empty()),
        "flat decks must not gain tree numbers"
    );
    state.open_search();

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|frame| render_app(&state, frame, &mut bindings))
        .expect("draw TOC modal");

    let lines = buffer_lines(terminal.backend().buffer());
    let row = lines
        .iter()
        .find(|line| line.contains("Intro"))
        .expect("row");
    assert!(row.contains("[  1] Intro"), "row: {row:?}");
    assert!(!row.trim_end().ends_with("[1]"), "row: {row:?}");
}

#[test]
fn test_toc_search_selects_section_number_match() {
    let mut state = PresentationState::new(120, vec![]);
    state.set_toc(&three_level_toc());
    handle_key_event(make_key(KeyCode::Char('/')), &mut state);
    for c in "1.1".chars() {
        handle_key_event(make_key(KeyCode::Char(c)), &mut state);
    }
    assert_eq!(state.search_state().query(), "1.1");
    // The direct hit wins the cursor even though its parent renders first.
    assert_eq!(state.search_state().selected_slide_index(), Some(63));

    let action = handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 63);
}

#[test]
fn test_toc_modal_highlights_fuzzy_match_as_direct_match() {
    let mut state = PresentationState::new(120, vec![]);
    state.set_toc(&three_level_toc());
    state.open_search();
    // Fuzzy query "sc" matches "Sub Chapter"
    state.search_push_char('s');
    state.search_push_char('c');

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|frame| render_app(&state, frame, &mut bindings))
        .expect("draw TOC modal");

    let buffer = terminal.backend().buffer();
    let mut found = false;
    for y in 0..24 {
        for x in 0..70 {
            let cell = buffer.cell((x, y)).unwrap();
            if cell.symbol() == "S" {
                let cell_next = buffer.cell((x + 1, y)).unwrap();
                let cell_next2 = buffer.cell((x + 2, y)).unwrap();
                if cell_next.symbol() == "u" && cell_next2.symbol() == "b" {
                    // "Sub Chapter" matched via fuzzy search, so it must NOT be styled as dimmed ancestor node (DarkGray).
                    assert_ne!(
                        cell.fg,
                        Color::DarkGray,
                        "Fuzzy match must be rendered as direct match, not dimmed context"
                    );
                    found = true;
                }
            }
        }
    }
    assert!(found, "Sub Chapter row must be rendered");
}
