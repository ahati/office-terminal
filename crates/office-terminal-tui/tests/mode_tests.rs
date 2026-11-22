use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use office_terminal_tui::app::events::{handle_key_event, AppAction};
use office_terminal_tui::app::state::{AppMode, BlankMode, PresentationState};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl_key(ch: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(ch),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_toggle_presenter_mode() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    let action1 = handle_key_event(key(KeyCode::Char('p')), &mut state);
    assert_eq!(action1, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    let action2 = handle_key_event(key(KeyCode::Char('p')), &mut state);
    assert_eq!(action2, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_toggle_status_line() {
    let mut state = PresentationState::new(5, vec![]);
    assert!(state.status_line_visible());

    let action = handle_key_event(key(KeyCode::Char('t')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(!state.status_line_visible());

    let action2 = handle_key_event(key(KeyCode::Char('t')), &mut state);
    assert_eq!(action2, AppAction::Redraw);
    assert!(state.status_line_visible());
}

#[test]
fn test_blank_screen_toggles_and_resume() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(state.blank_mode(), BlankMode::None);

    // Press 'b' -> Black blank screen
    let action_b = handle_key_event(key(KeyCode::Char('b')), &mut state);
    assert_eq!(action_b, AppAction::Redraw);
    assert_eq!(state.blank_mode(), BlankMode::Black);
    assert!(state.is_blank());

    // Press Space while blank -> resumes
    let action_resume = handle_key_event(key(KeyCode::Char(' ')), &mut state);
    assert_eq!(action_resume, AppAction::Redraw);
    assert_eq!(state.blank_mode(), BlankMode::None);
    assert!(!state.is_blank());

    // Press 'w' -> White blank screen
    let action_w = handle_key_event(key(KeyCode::Char('w')), &mut state);
    assert_eq!(action_w, AppAction::Redraw);
    assert_eq!(state.blank_mode(), BlankMode::White);
    assert!(state.is_blank());

    // Press 'w' again -> resumes
    let action_w2 = handle_key_event(key(KeyCode::Char('w')), &mut state);
    assert_eq!(action_w2, AppAction::Redraw);
    assert_eq!(state.blank_mode(), BlankMode::None);
}

#[test]
fn test_help_modal_navigation_and_restore() {
    let mut state = PresentationState::new(5, vec![]);
    state.set_mode(AppMode::PresenterConsole);

    // Open Help
    let action_h = handle_key_event(key(KeyCode::Char('h')), &mut state);
    assert_eq!(action_h, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::HelpModal);

    // In Help modal, '?' closes it
    let action_q = handle_key_event(key(KeyCode::Char('?')), &mut state);
    assert_eq!(action_q, AppAction::Redraw);
    // Restores to PresenterConsole!
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    // Open Help with '?'
    handle_key_event(key(KeyCode::Char('?')), &mut state);
    assert_eq!(state.mode(), AppMode::HelpModal);

    // Close Help with 'q'
    let action_close = handle_key_event(key(KeyCode::Char('q')), &mut state);
    assert_eq!(action_close, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);
}

#[test]
fn test_quit_shortcuts() {
    let mut state = PresentationState::new(5, vec![]);

    assert_eq!(handle_key_event(key(KeyCode::Char('q')), &mut state), AppAction::Quit);
    assert_eq!(handle_key_event(key(KeyCode::Esc), &mut state), AppAction::Quit);
    assert_eq!(handle_key_event(ctrl_key('c'), &mut state), AppAction::Quit);
}
