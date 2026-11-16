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
    assert_eq!(reader.default_fit, ReaderFitMode::FitWidth);
    assert_eq!(reader.default_zoom, 1.0);
    assert!(reader.sync);
    assert!(reader.cover_alone);

    // Cover alone: page 0 is alone
    assert_eq!(reader.visible_pages(), vec![0]);
    assert_eq!(reader.focused_page(), 0);
}

#[test]
fn test_reader_spread_navigation() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);

    // Anchor 0 -> cover alone [0]
    assert_eq!(reader.visible_pages(), vec![0]);

    // Next row -> [1, 2]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![1, 2]);

    // Next row -> [3, 4]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![3, 4]);

    // Focus right -> focused page is 4
    reader.toggle_focus();
    assert_eq!(reader.focus, Slot::Right);
    assert_eq!(reader.focused_page(), 4);

    // Prev row -> [1, 2]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![1, 2]);

    // Prev row -> [0]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![0]);
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

    // Jump to page 4 (0-based index 4: pages 5 and 6 in 1-based, spread [3, 4] or [4, 5])
    reader.jump_to_page(4);
    assert!(reader.visible_pages().contains(&4));

    // Jump to 0 -> cover
    reader.jump_to_page(0);
    assert_eq!(reader.visible_pages(), vec![0]);

    // Jump beyond total
    reader.jump_to_page(99);
    assert_eq!(reader.anchor, 9);
}

#[test]
fn test_reader_scroll_and_zoom() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    reader.next_row(); // spread [1, 2]

    // Sync scroll: scrolls both visible pages
    reader.scroll_vertical(50);
    assert_eq!(reader.page_view(1).scroll_y, 50);
    assert_eq!(reader.page_view(2).scroll_y, 50);

    reader.scroll_horizontal(30);
    assert_eq!(reader.page_view(1).scroll_x, 30);
    assert_eq!(reader.page_view(2).scroll_x, 0);

    // Zoom
    reader.adjust_zoom(1.2);
    assert!((reader.page_view(1).zoom - 1.2).abs() < 0.01);

    // Reset view
    reader.reset_view();
    assert_eq!(reader.page_view(1).scroll_y, 0);
    assert!((reader.page_view(1).zoom - 1.0).abs() < 0.01);
}

#[test]
fn test_reader_fit_cycle() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    assert_eq!(reader.default_fit, ReaderFitMode::FitWidth);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitPage);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitWidth);
}

#[test]
fn test_reader_status_line_format() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);
    reader.next_row(); // spread [1, 2] -> 1-based "spread 2-3"

    let status = reader.status_line("report.pdf");
    assert!(status.contains("report.pdf"));
    assert!(status.contains("spread 2-3/10"));
    assert!(status.contains("fit:width 100%"));
    assert!(status.contains("focus:L"));
    assert!(status.contains("sync:on"));
    assert!(status.contains("cover:on"));
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
    assert_eq!(state.reader_state().unwrap().visible_pages(), vec![1, 2]);

    // Scroll vertical down
    let action = handle_key_event(make_key(KeyCode::Char('j')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().page_view(1).scroll_y, 24);

    // Focus toggle
    let action = handle_key_event(make_key(KeyCode::Tab), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().focus, Slot::Right);

    // Fit cycle
    let action = handle_key_event(make_key(KeyCode::Char('f')), &mut state);
    assert_eq!(action, AppAction::ScaleChanged);

    // Layout toggle
    let action = handle_key_event(make_key(KeyCode::Char('2')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Single);

    // Quit
    let action = handle_key_event(make_key(KeyCode::Char('q')), &mut state);
    assert_eq!(action, AppAction::Quit);
}
