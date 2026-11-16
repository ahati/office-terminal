use super::*;
use crossterm::event::{KeyEventKind, KeyEventState};

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn make_ctrl_key(ch: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(ch),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_navigation_next_keys() {
    let next_keys = [
        KeyCode::Char(' '),
        KeyCode::Right,
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::Char('n'),
        KeyCode::Char('j'),
    ];

    for code in next_keys {
        let mut state = PresentationState::new(10, vec![]);
        let action = handle_key_event(make_key(code), &mut state);
        assert_eq!(action, AppAction::SlideChanged, "Key {:?} should advance slide", code);
        assert_eq!(state.current_slide(), 1);
    }
}

#[test]
fn test_navigation_prev_keys() {
    let prev_keys = [
        KeyCode::Left,
        KeyCode::Up,
        KeyCode::Backspace,
        KeyCode::PageUp,
        KeyCode::Char('k'),
        KeyCode::Char('P'),
    ];

    for code in prev_keys {
        let mut state = PresentationState::new(10, vec![]);
        state.set_current_slide(5);
        let action = handle_key_event(make_key(code), &mut state);
        assert_eq!(action, AppAction::SlideChanged, "Key {:?} should go to previous slide", code);
        assert_eq!(state.current_slide(), 4);
    }
}

#[test]
fn test_jump_and_digits() {
    let mut state = PresentationState::new(10, vec![]);
    handle_key_event(make_key(KeyCode::Char('3')), &mut state);
    handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(state.current_slide(), 2);

    handle_key_event(make_key(KeyCode::Char('g')), &mut state);
    assert_eq!(state.current_slide(), 0);

    handle_key_event(make_key(KeyCode::Char('G')), &mut state);
    assert_eq!(state.current_slide(), 9);
}

#[test]
fn test_toggle_presenter_and_help() {
    let mut state = PresentationState::new(5, vec![]);
    let action = handle_key_event(make_key(KeyCode::Char('p')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    let action2 = handle_key_event(make_key(KeyCode::Char('?')), &mut state);
    assert_eq!(action2, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::HelpModal);

    let action3 = handle_key_event(make_key(KeyCode::Esc), &mut state);
    assert_eq!(action3, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);
}

#[test]
fn test_quit_events() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(handle_key_event(make_key(KeyCode::Char('q')), &mut state), AppAction::Quit);
    assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Quit);
    assert_eq!(handle_key_event(make_ctrl_key('c'), &mut state), AppAction::Quit);
}
