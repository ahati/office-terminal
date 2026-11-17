use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::mode::AppMode;
use kitty_pptx_tui::app::reader_state::{ReaderFitMode, ReaderLayout, ReaderState, Slot};
use kitty_pptx_tui::app::state::PresentationState;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_reader_state_initialization() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let reader = ReaderState::new(10, page_sizes);

    assert_eq!(reader.total_pages(), 10);
    assert_eq!(reader.layout, ReaderLayout::Spread);
    assert_eq!(reader.focus, Slot::Left);
    assert_eq!(reader.default_fit, ReaderFitMode::FitPage);
    assert_eq!(reader.default_zoom, 1.0);
    assert!(reader.sync);
    assert!(!reader.cover_alone);

    // Initial pages: side-by-side spread [0, 1]
    assert_eq!(reader.visible_pages(), vec![0, 1]);
    assert_eq!(reader.focused_page(), 0);
}

#[test]
fn test_reader_spread_navigation() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);

    // Anchor 0 -> spread [0, 1]
    assert_eq!(reader.visible_pages(), vec![0, 1]);

    // Next row -> [2, 3]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![2, 3]);

    // Next row -> [4, 5]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![4, 5]);

    // Focus right -> focused page is 5
    reader.toggle_focus();
    assert_eq!(reader.focus, Slot::Right);
    assert_eq!(reader.focused_page(), 5);

    // Prev row -> [2, 3]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![2, 3]);

    // Prev row -> [0, 1]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![0, 1]);
}

#[test]
fn test_reader_single_navigation() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    reader.toggle_layout();
    assert_eq!(reader.layout, ReaderLayout::Single);

    assert_eq!(reader.visible_pages(), vec![0]);
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![1]);
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![2]);
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![1]);
}

#[test]
fn test_reader_jump_to_page() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);

    // Jump to page 4 -> spread [4, 5]
    reader.jump_to_page(4);
    assert_eq!(reader.visible_pages(), vec![4, 5]);
    assert_eq!(reader.focused_page(), 4);

    // Jump to page 5 -> spread [4, 5], focused on right (page 5)
    reader.jump_to_page(5);
    assert_eq!(reader.visible_pages(), vec![4, 5]);
    assert_eq!(reader.focused_page(), 5);

    // Jump to 0 -> spread [0, 1]
    reader.jump_to_page(0);
    assert_eq!(reader.visible_pages(), vec![0, 1]);

    // Jump beyond total
    reader.jump_to_page(99);
    assert_eq!(reader.anchor, 8);
}

#[test]
fn test_reader_scroll_and_zoom() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    reader.next_row(); // spread [2, 3]
    reader.cycle_fit(); // Switch visible pages [2, 3] to FitWidth to test vertical scrolling

    // Sync scroll: scrolls both visible pages
    reader.scroll_vertical(50);
    assert_eq!(reader.page_view(2).scroll_y, 50);
    assert_eq!(reader.page_view(3).scroll_y, 50);

    reader.scroll_horizontal(30);
    assert_eq!(reader.page_view(2).scroll_x, 30);
    assert_eq!(reader.page_view(3).scroll_x, 0);

    // Zoom
    reader.adjust_zoom(1.2);
    assert!((reader.page_view(2).zoom - 1.2).abs() < 0.01);

    // Reset view
    reader.reset_view();
    assert_eq!(reader.page_view(2).scroll_y, 0);
    assert!((reader.page_view(2).zoom - 1.0).abs() < 0.01);
}

#[test]
fn test_reader_fit_cycle() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    assert_eq!(reader.default_fit, ReaderFitMode::FitPage);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitWidth);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitPage);
}

#[test]
fn test_reader_status_line_format() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);
    reader.next_row(); // spread [2, 3] -> 1-based "spread 3-4"

    let status = reader.status_line("report.pdf");
    assert!(status.contains("report.pdf"));
    assert!(status.contains("spread 3-4/10"));
    assert!(status.contains("fit:page 100%"));
    assert!(status.contains("focus:L"));
    assert!(status.contains("sync:on"));
    assert!(status.contains("q:help"));
}

#[test]
fn test_reader_key_events() {
    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, vec![(612.0, 792.0); 10]);

    assert_eq!(state.mode(), AppMode::Reader);
    assert_eq!(state.base_mode(), AppMode::Reader);

    // Next row
    let action = handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().visible_pages(), vec![2, 3]);

    // Scroll vertical down in FitWidth mode moves column offset
    state.reader_state_mut().unwrap().cycle_fit();
    let action = handle_key_event(make_key(KeyCode::Char('j')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(state.reader_state().unwrap().scroll_px > 0);

    // Focus toggle
    let action = handle_key_event(make_key(KeyCode::BackTab), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().focus, Slot::Right);

    // Fit cycle
    let action = handle_key_event(make_key(KeyCode::Char('f')), &mut state);
    assert_eq!(action, AppAction::Redraw);

    // Layout toggle
    let action = handle_key_event(make_key(KeyCode::Char('2')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Single);

    // Quit
    let action = handle_key_event(make_key(KeyCode::Char('q')), &mut state);
    assert_eq!(action, AppAction::Quit);
}

#[test]
fn test_reader_arrow_keys_navigation() {
    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, vec![(612.0, 792.0); 10]);

    // Initial: anchor 0 (spread [0, 1])
    assert_eq!(state.reader_state().unwrap().anchor, 0);

    // Right Arrow advances fractionally (>= 8 steps per row)
    for _ in 0..7 {
        handle_key_event(make_key(KeyCode::Right), &mut state);
        assert_eq!(state.reader_state().unwrap().anchor, 0);
        assert!(state.reader_state().unwrap().scroll_px > 0);
    }

    // 8th step completes the row transition to spread [2, 3]
    let action = handle_key_event(make_key(KeyCode::Right), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.current_slide(), 2);

    // PageDown immediately advances to next row [4, 5] (snaps to row top)
    let action = handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 4);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    // PageUp retreats to previous spread [2, 3] (snaps to row top)
    let action = handle_key_event(make_key(KeyCode::PageUp), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    // Down Arrow also moves column fractionally
    let action = handle_key_event(make_key(KeyCode::Down), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert!(state.reader_state().unwrap().scroll_px > 0);
}

#[test]
fn test_reader_sorter_integration() {
    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(10, vec![(612.0, 792.0); 10]);

    // Tab opens sorter
    handle_key_event(make_key(KeyCode::Tab), &mut state);
    assert_eq!(state.mode(), AppMode::Sorter);

    // In sorter, move right twice to slide index 2
    handle_key_event(make_key(KeyCode::Right), &mut state);
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert_eq!(state.selected_sorter_index(), 2);

    // Press Enter to select slide 2 and exit sorter
    let action = handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.mode(), AppMode::Reader);
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.reader_state().unwrap().visible_pages(), vec![2, 3]);
}
