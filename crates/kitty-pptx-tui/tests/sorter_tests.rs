use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::state::{AppMode, PresentationState, PresenterMode};
use kitty_pptx_tui::app::ui::sorter::render_sorter;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_sorter_mode_toggles() {
    let mut state = PresentationState::new(8, vec![]);
    assert_eq!(state.mode(), AppMode::Fullscreen);
    assert_eq!(state.mode(), PresenterMode::Normal);

    // 1. Toggle via method
    state.toggle_sorter();
    assert_eq!(state.mode(), AppMode::Sorter);
    assert_eq!(state.mode(), PresenterMode::ThumbnailGrid);
    assert_eq!(state.selected_sorter_index(), 0);

    state.toggle_sorter();
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // 2. Toggle preserves PresenterConsole mode
    state.set_mode(AppMode::PresenterConsole);
    state.toggle_sorter();
    assert_eq!(state.mode(), AppMode::Sorter);

    state.toggle_sorter();
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    // 3. Toggle via key events ('s' and 'Tab')
    state.set_mode(AppMode::Fullscreen);
    let action_s = handle_key_event(make_key(KeyCode::Char('s')), &mut state);
    assert_eq!(action_s, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Sorter);

    let action_tab = handle_key_event(make_key(KeyCode::Tab), &mut state);
    assert_eq!(action_tab, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Tab opens, Esc cancels
    let _ = handle_key_event(make_key(KeyCode::Tab), &mut state);
    assert_eq!(state.mode(), AppMode::Sorter);

    let action_esc = handle_key_event(make_key(KeyCode::Esc), &mut state);
    assert_eq!(action_esc, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_sorter_initial_index_tracking() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_current_slide(6);

    state.toggle_sorter();
    assert_eq!(state.selected_sorter_index(), 6);
}

#[test]
fn test_2d_grid_cursor_movement_clamped() {
    // 10 slides in 4 columns:
    // Row 0: 0, 1, 2, 3
    // Row 1: 4, 5, 6, 7
    // Row 2: 8, 9
    let mut state = PresentationState::new(10, vec![]);
    state.set_sorter_cols(4);
    state.set_sorter_wrap(false);

    assert_eq!(state.selected_sorter_index(), 0);

    // Boundary clamp at top-left
    assert!(!state.sorter_left());
    assert_eq!(state.selected_sorter_index(), 0);
    assert!(!state.sorter_up());
    assert_eq!(state.selected_sorter_index(), 0);

    // Move right along row 0
    assert!(state.sorter_right());
    assert_eq!(state.selected_sorter_index(), 1);
    assert!(state.sorter_right());
    assert_eq!(state.selected_sorter_index(), 2);
    assert!(state.sorter_right());
    assert_eq!(state.selected_sorter_index(), 3);

    // Move down to row 1 (index 3 + 4 = 7)
    assert!(state.sorter_down());
    assert_eq!(state.selected_sorter_index(), 7);

    // Move left along row 1
    assert!(state.sorter_left());
    assert_eq!(state.selected_sorter_index(), 6);

    // Move up back to row 0 (index 6 - 4 = 2)
    assert!(state.sorter_up());
    assert_eq!(state.selected_sorter_index(), 2);

    // Move down to row 1 then row 2
    assert!(state.sorter_down()); // 6
    assert_eq!(state.selected_sorter_index(), 6);
    assert!(state.sorter_down()); // 6 + 4 = 10, clamped to last slide 9
    assert_eq!(state.selected_sorter_index(), 9);

    // Boundary clamp at bottom
    assert!(!state.sorter_down());
    assert_eq!(state.selected_sorter_index(), 9);
    assert!(!state.sorter_right());
    assert_eq!(state.selected_sorter_index(), 9);
}

#[test]
fn test_2d_grid_cursor_movement_wrapped() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_sorter_cols(4);
    state.set_sorter_wrap(true);

    // Wrap left from index 0 -> index 9
    assert_eq!(state.selected_sorter_index(), 0);
    assert!(state.sorter_left());
    assert_eq!(state.selected_sorter_index(), 9);

    // Wrap right from index 9 -> index 0
    assert!(state.sorter_right());
    assert_eq!(state.selected_sorter_index(), 0);

    // Wrap up from col 1, row 0 (index 1) to bottom row col 1 (index 9)
    state.set_selected_sorter_index(1);
    assert!(state.sorter_up());
    assert_eq!(state.selected_sorter_index(), 9);

    // Wrap down from bottom row col 1 (index 9) to top row col 1 (index 1)
    assert!(state.sorter_down());
    assert_eq!(state.selected_sorter_index(), 1);
}

#[test]
fn test_sorter_page_home_end_navigation() {
    let mut state = PresentationState::new(20, vec![]);
    state.set_sorter_cols(4);

    // End jumps to last slide
    assert!(state.sorter_end());
    assert_eq!(state.selected_sorter_index(), 19);

    // Home jumps to slide 0
    assert!(state.sorter_home());
    assert_eq!(state.selected_sorter_index(), 0);

    // Page down moves by 2 rows (8 slides)
    assert!(state.sorter_page_down());
    assert_eq!(state.selected_sorter_index(), 8);
    assert!(state.sorter_page_down());
    assert_eq!(state.selected_sorter_index(), 16);
    assert!(state.sorter_page_down());
    assert_eq!(state.selected_sorter_index(), 19); // clamped to 19

    // Page up moves backwards by 2 rows (8 slides)
    assert!(state.sorter_page_up());
    assert_eq!(state.selected_sorter_index(), 11);
    assert!(state.sorter_page_up());
    assert_eq!(state.selected_sorter_index(), 3);
    assert!(state.sorter_page_up());
    assert_eq!(state.selected_sorter_index(), 0); // clamped to 0
}

#[test]
fn test_sorter_enter_selection() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_current_slide(1);

    // Enter sorter mode
    state.toggle_sorter();
    assert_eq!(state.mode(), AppMode::Sorter);
    assert_eq!(state.selected_sorter_index(), 1);

    // Navigate to slide 5 (index 4)
    state.set_selected_sorter_index(4);

    // Press Enter to confirm selection
    let action = handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 4);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_sorter_esc_cancellation() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_current_slide(2);

    state.toggle_sorter();
    state.set_selected_sorter_index(7);

    // Press Esc to cancel without changing current slide
    let action = handle_key_event(make_key(KeyCode::Esc), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_sorter_ui_rendering_smoke() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = PresentationState::new(12, vec![]);
    state.set_mode(AppMode::Sorter);
    state.set_selected_sorter_index(3);

    terminal
        .draw(|f| {
            render_sorter(&state, f, f.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    // Check that header text is present in the rendered buffer
    let mut text_found = false;
    for y in 0..buffer.area.height {
        let mut line_str = String::new();
        for x in 0..buffer.area.width {
            line_str.push(
                buffer
                    .cell((x, y))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' '),
            );
        }
        if line_str.contains("SLIDE SORTER") {
            text_found = true;
            break;
        }
    }
    assert!(text_found, "Buffer should contain 'SLIDE SORTER' header");
}

/// Regression: cards used to be filled with a solid black block because the
/// direct-placement path only knew the current/preview image slots. Every card
/// must now reserve a real graphics canvas at its fitted rect.
#[test]
fn test_sorter_cards_reserve_a_thumbnail_canvas() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut state = PresentationState::new(12, vec![]);
    state.set_mode(AppMode::Sorter);

    terminal
        .draw(|f| {
            render_sorter(&state, f, f.area());
        })
        .unwrap();

    let cards = state.thumbnail_placement_rects();
    assert_eq!(
        cards.len(),
        12,
        "every visible card records a thumbnail rect"
    );

    let buffer = terminal.backend().buffer();
    for (slide, rect) in &cards {
        assert!(
            rect.cols > 0 && rect.rows > 0,
            "slide {} has a placeable rect: {rect:?}",
            slide + 1
        );
        let cell = buffer
            .cell((rect.col, rect.row))
            .unwrap_or_else(|| panic!("rect for slide {} is outside the buffer", slide + 1));
        assert_eq!(
            cell.bg,
            Color::Reset,
            "slide {} reserves a transparent graphics canvas instead of a black block",
            slide + 1
        );
    }
}
