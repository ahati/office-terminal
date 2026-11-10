use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::state::PresentationState;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_next_slide_navigation_keys() {
    let next_keys = [
        KeyCode::Right,
        KeyCode::Char(' '),
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::Char('n'),
        KeyCode::Char('j'),
    ];

    for code in next_keys {
        let mut state = PresentationState::new(10, vec![]);
        assert_eq!(state.current_slide(), 0);

        let action = handle_key_event(key(code), &mut state);
        assert_eq!(action, AppAction::SlideChanged, "Key {:?} should trigger SlideChanged", code);
        assert_eq!(state.current_slide(), 1, "Key {:?} should move to slide 1", code);
    }
}

#[test]
fn test_prev_slide_navigation_keys() {
    let prev_keys = [
        KeyCode::Left,
        KeyCode::Backspace,
        KeyCode::Up,
        KeyCode::PageUp,
        KeyCode::Char('k'),
        KeyCode::Char('P'),
    ];

    for code in prev_keys {
        let mut state = PresentationState::new(10, vec![]);
        state.set_current_slide(4);
        assert_eq!(state.current_slide(), 4);

        let action = handle_key_event(key(code), &mut state);
        assert_eq!(action, AppAction::SlideChanged, "Key {:?} should trigger SlideChanged", code);
        assert_eq!(state.current_slide(), 3, "Key {:?} should move to slide 3", code);
    }
}

#[test]
fn test_slide_index_clamping_boundaries() {
    // 1. Clamping at upper boundary
    let mut state = PresentationState::new(3, vec![]);
    assert_eq!(state.current_slide(), 0);

    state.next_slide();
    assert_eq!(state.current_slide(), 1);

    state.next_slide();
    assert_eq!(state.current_slide(), 2);

    // Beyond last slide
    let advanced = state.next_slide();
    assert!(!advanced);
    assert_eq!(state.current_slide(), 2);

    let action = handle_key_event(key(KeyCode::Char(' ')), &mut state);
    assert_eq!(action, AppAction::None);
    assert_eq!(state.current_slide(), 2);

    // 2. Clamping at lower boundary
    state.first_slide();
    assert_eq!(state.current_slide(), 0);

    let backed = state.prev_slide();
    assert!(!backed);
    assert_eq!(state.current_slide(), 0);

    let action_back = handle_key_event(key(KeyCode::Left), &mut state);
    assert_eq!(action_back, AppAction::None);
    assert_eq!(state.current_slide(), 0);
}

#[test]
fn test_zero_slide_deck_safety() {
    let mut state = PresentationState::new(0, vec![]);
    assert_eq!(state.current_slide(), 0);
    assert_eq!(state.total_slides(), 0);

    assert!(!state.next_slide());
    assert!(!state.prev_slide());
    assert!(!state.first_slide());
    assert!(!state.last_slide());
    assert!(!state.jump_to_slide(1));

    let action = handle_key_event(key(KeyCode::Char(' ')), &mut state);
    assert_eq!(action, AppAction::None);
    assert_eq!(state.current_slide(), 0);
}

#[test]
fn test_one_slide_deck_safety() {
    let mut state = PresentationState::new(1, vec![]);
    assert_eq!(state.current_slide(), 0);

    assert!(!state.next_slide());
    assert!(!state.prev_slide());
    assert_eq!(state.current_slide(), 0);
}

#[test]
fn test_first_and_last_slide_keys() {
    let mut state = PresentationState::new(15, vec![]);
    state.set_current_slide(7);

    // 'g' jumps to slide 0 (first slide)
    let action_g = handle_key_event(key(KeyCode::Char('g')), &mut state);
    assert_eq!(action_g, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 0);

    // 'G' jumps to last slide (14)
    let action_big_g = handle_key_event(key(KeyCode::Char('G')), &mut state);
    assert_eq!(action_big_g, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 14);
}

#[test]
fn test_digit_jump_sequence() {
    let mut state = PresentationState::new(25, vec![]);

    // Type "1", "2" then Enter -> jumps to slide 12 (0-indexed 11)
    handle_key_event(key(KeyCode::Char('1')), &mut state);
    assert_eq!(state.digit_buffer(), "1");

    handle_key_event(key(KeyCode::Char('2')), &mut state);
    assert_eq!(state.digit_buffer(), "12");

    let action = handle_key_event(key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 11);
    assert_eq!(state.digit_buffer(), "");

    // Jump beyond total slides clamps to last slide (24)
    handle_key_event(key(KeyCode::Char('9')), &mut state);
    handle_key_event(key(KeyCode::Char('9')), &mut state);
    handle_key_event(key(KeyCode::Enter), &mut state);
    assert_eq!(state.current_slide(), 24);
}
