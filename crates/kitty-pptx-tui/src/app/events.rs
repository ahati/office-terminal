use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::state::{BlankMode, PresentationState};

/// Action resulting from an event handling cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// No state changed requiring pipeline or driver action.
    None,
    /// Slide index changed, requires prefetching and staging.
    SlideChanged,
    /// Terminal layout or UI mode changed, requires re-render.
    Redraw,
    /// Exit presentation.
    Quit,
}

/// Handle a crossterm keyboard event.
pub fn handle_key_event(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    // 1. Check Ctrl-C first
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return AppAction::Quit;
    }

    // 2. If in Help Modal
    if state.mode() == crate::app::state::AppMode::HelpModal {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('h') => {
                state.close_help();
                return AppAction::Redraw;
            }
            _ => return AppAction::None,
        }
    }

    // 3. If in Blank Screen mode
    if state.is_blank() {
        match key.code {
            KeyCode::Char('b') => {
                state.toggle_blank(BlankMode::Black);
                return AppAction::Redraw;
            }
            KeyCode::Char('w') => {
                state.toggle_blank(BlankMode::White);
                return AppAction::Redraw;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                state.unblank();
                return AppAction::Redraw;
            }
            _ => {
                // Any other key unblanks and resumes
                state.unblank();
                return AppAction::Redraw;
            }
        }
    }

    // 4. Normal / Presenter mode key handling
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,

        // Help
        KeyCode::Char('?') | KeyCode::Char('h') => {
            state.toggle_help();
            AppAction::Redraw
        }

        // Toggle Presenter Mode
        KeyCode::Char('p') => {
            state.toggle_presenter_mode();
            AppAction::Redraw
        }

        // Blank screen toggles
        KeyCode::Char('b') => {
            state.toggle_blank(BlankMode::Black);
            AppAction::Redraw
        }
        KeyCode::Char('w') => {
            state.toggle_blank(BlankMode::White);
            AppAction::Redraw
        }

        // Toggle Fullscreen minimal status line
        KeyCode::Char('t') => {
            state.toggle_status_line();
            AppAction::Redraw
        }

        // Digits: accumulate into digit buffer
        KeyCode::Char(c @ '0'..='9') => {
            state.push_digit(c);
            AppAction::Redraw
        }

        // Enter: commit digit jump if typed, or next slide
        KeyCode::Enter => {
            if !state.digit_buffer().is_empty() {
                if state.commit_digits() {
                    AppAction::SlideChanged
                } else {
                    AppAction::Redraw
                }
            } else if state.next_slide() {
                AppAction::SlideChanged
            } else {
                AppAction::None
            }
        }

        // Jump to first slide or commit digits
        KeyCode::Char('g') => {
            if !state.digit_buffer().is_empty() {
                if state.commit_digits() {
                    AppAction::SlideChanged
                } else {
                    AppAction::Redraw
                }
            } else if state.first_slide() {
                AppAction::SlideChanged
            } else {
                AppAction::None
            }
        }

        // Jump to last slide
        KeyCode::Char('G') => {
            state.clear_digits();
            if state.last_slide() {
                AppAction::SlideChanged
            } else {
                AppAction::None
            }
        }

        // Next slide: Space, Right, Down, PageDown, 'n', 'j'
        KeyCode::Right
        | KeyCode::Char(' ')
        | KeyCode::Down
        | KeyCode::PageDown
        | KeyCode::Char('n')
        | KeyCode::Char('j') => {
            state.clear_digits();
            if state.next_slide() {
                AppAction::SlideChanged
            } else {
                AppAction::None
            }
        }

        // Previous slide: Backspace, Left, Up, PageUp, 'k', 'P'
        KeyCode::Left
        | KeyCode::Backspace
        | KeyCode::Up
        | KeyCode::PageUp
        | KeyCode::Char('k')
        | KeyCode::Char('P') => {
            state.clear_digits();
            if state.prev_slide() {
                AppAction::SlideChanged
            } else {
                AppAction::None
            }
        }

        _ => AppAction::None,
    }
}

/// Handle a terminal resize event.
pub fn handle_resize_event(_cols: u16, _rows: u16) -> AppAction {
    AppAction::Redraw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, KeyEventKind, KeyEventState};

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
        assert_eq!(state.mode(), crate::app::state::AppMode::PresenterConsole);

        let action2 = handle_key_event(make_key(KeyCode::Char('?')), &mut state);
        assert_eq!(action2, AppAction::Redraw);
        assert_eq!(state.mode(), crate::app::state::AppMode::HelpModal);

        let action3 = handle_key_event(make_key(KeyCode::Esc), &mut state);
        assert_eq!(action3, AppAction::Redraw);
        assert_eq!(state.mode(), crate::app::state::AppMode::PresenterConsole);
    }

    #[test]
    fn test_quit_events() {
        let mut state = PresentationState::new(5, vec![]);
        assert_eq!(handle_key_event(make_key(KeyCode::Char('q')), &mut state), AppAction::Quit);
        assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Quit);
        assert_eq!(handle_key_event(make_ctrl_key('c'), &mut state), AppAction::Quit);
    }
}
