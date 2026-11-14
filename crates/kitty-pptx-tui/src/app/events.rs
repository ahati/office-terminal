use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::mode::AppMode;
use crate::app::state::{BlankMode, PresentationState};

/// Action resulting from an event handling cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// No state changed requiring pipeline or driver action.
    None,
    /// Slide index changed, requires prefetching and staging.
    SlideChanged,
    /// Terminal layout or UI mode changed, requires re-render.
    Redraw,
    /// Exit presentation.
    Quit,
    /// Effective zoom factor changed; recompute the render target.
    ScaleChanged,
    /// Launch a validated external URL in the OS browser.
    OpenExternal(String),
}

/// Handle a crossterm keyboard event.
pub fn handle_key_event(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    // 1. Check Ctrl-C first
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return AppAction::Quit;
    }

    // 2. Check Ctrl-O global hyperlink picker shortcut
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
        state.toggle_link_picker();
        return AppAction::Redraw;
    }

    // 3. Check Overlays
    match state.mode() {
        AppMode::HelpModal => return handle_help_key(key, state),
        AppMode::CommentsDrawer => return handle_comments_key(key, state),
        AppMode::WarningDrawer => return handle_warnings_key(key, state),
        AppMode::TocSearchModal => return handle_search_key(key, state),
        AppMode::FindModal => {
            return crate::app::find_state::handle_find_key(key, state)
        }
        AppMode::LinkPickerModal => {
            return crate::app::link_state::handle_link_picker_key(key, state)
        }
        _ => {}
    }

    // 4. Ctrl-F toggles the in-document find modal; `/` keeps the TOC search.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
        state.clear_digits();
        if state.mode() == AppMode::FindModal {
            state.close_find();
        } else {
            state.open_find();
        }
        return AppAction::Redraw;
    }

    // 4. If in Sorter Grid Mode
    if state.mode() == AppMode::Sorter {
        return handle_sorter_key(key, state);
    }

    // 5. If in Blank Screen mode
    if state.is_blank() {
        return handle_blank_key(key, state);
    }

    // 6. Normal / Presenter mode key handling
    handle_presentation_key(key, state)
}

fn handle_help_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('h') => {
            state.close_help();
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_comments_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c') => {
            state.close_comments();
            AppAction::Redraw
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.scroll_comments_up();
            AppAction::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.scroll_comments_down();
            AppAction::Redraw
        }
        KeyCode::PageUp => {
            for _ in 0..5 {
                state.scroll_comments_up();
            }
            AppAction::Redraw
        }
        KeyCode::PageDown => {
            for _ in 0..5 {
                state.scroll_comments_down();
            }
            AppAction::Redraw
        }
        KeyCode::Char('?') => {
            state.toggle_help();
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_warnings_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    let is_w = key.code == KeyCode::Char('W')
        || (key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::SHIFT));

    if is_w || key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
        state.close_warnings();
        return AppAction::Redraw;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.scroll_warnings_up();
            AppAction::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.scroll_warnings_down();
            AppAction::Redraw
        }
        KeyCode::PageUp => {
            for _ in 0..5 {
                state.scroll_warnings_up();
            }
            AppAction::Redraw
        }
        KeyCode::PageDown => {
            for _ in 0..5 {
                state.scroll_warnings_down();
            }
            AppAction::Redraw
        }
        KeyCode::Char('?') => {
            state.toggle_help();
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_search_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
        state.exit_search(false);
        return AppAction::Redraw;
    }

    match key.code {
        KeyCode::Esc => {
            state.exit_search(false);
            AppAction::Redraw
        }
        KeyCode::Enter => {
            if state.exit_search(true) {
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Up => {
            state.search_up();
            AppAction::Redraw
        }
        KeyCode::Down => {
            state.search_down();
            AppAction::Redraw
        }
        KeyCode::Backspace => {
            state.search_backspace();
            AppAction::Redraw
        }
        KeyCode::Char(c) => {
            state.search_push_char(c);
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_sorter_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    let is_w = key.code == KeyCode::Char('W')
        || (key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::SHIFT));

    if is_w {
        state.toggle_warnings();
        return AppAction::Redraw;
    }

    match key.code {
        KeyCode::Char('q') => AppAction::Quit,
        KeyCode::Esc | KeyCode::Tab | KeyCode::Char('s') => {
            state.exit_sorter(false);
            AppAction::Redraw
        }
        KeyCode::Enter => {
            state.exit_sorter(true);
            AppAction::SlideChanged
        }
        KeyCode::Char('c') => {
            state.toggle_comments();
            AppAction::Redraw
        }
        KeyCode::Char('/') => {
            state.open_search();
            AppAction::Redraw
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.sorter_left();
            AppAction::Redraw
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.sorter_right();
            AppAction::Redraw
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.sorter_up();
            AppAction::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.sorter_down();
            AppAction::Redraw
        }
        KeyCode::PageUp => {
            state.sorter_page_up();
            AppAction::Redraw
        }
        KeyCode::PageDown => {
            state.sorter_page_down();
            AppAction::Redraw
        }
        KeyCode::Home => {
            state.sorter_home();
            AppAction::Redraw
        }
        KeyCode::End => {
            state.sorter_end();
            AppAction::Redraw
        }
        KeyCode::Char('m') => {
            let msg = if let Some(slide) = state.current_slide_model() {
                crate::app::media::try_play_current_slide_media(slide)
                    .unwrap_or_else(|| "No media found on current slide".to_string())
            } else {
                "No media found on current slide".to_string()
            };
            state.set_toast(msg);
            AppAction::Redraw
        }
        KeyCode::Char('?') => {
            state.toggle_help();
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_blank_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    match key.code {
        KeyCode::Char('b') => {
            state.toggle_blank(BlankMode::Black);
            AppAction::Redraw
        }
        KeyCode::Char('w') => {
            state.toggle_blank(BlankMode::White);
            AppAction::Redraw
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            state.unblank();
            AppAction::Redraw
        }
        _ => {
            state.unblank();
            AppAction::Redraw
        }
    }
}

fn handle_presentation_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    let is_w = key.code == KeyCode::Char('W')
        || (key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::SHIFT));

    if is_w {
        state.clear_digits();
        state.toggle_warnings();
        return AppAction::Redraw;
    }

    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,

        // Overlays
        KeyCode::Char('o') => {
            state.clear_digits();
            state.toggle_link_picker();
            AppAction::Redraw
        }
        KeyCode::Char('c') => {
            state.clear_digits();
            state.toggle_comments();
            AppAction::Redraw
        }
        KeyCode::Char('/') => {
            state.clear_digits();
            state.open_search();
            AppAction::Redraw
        }

        // Help
        KeyCode::Char('?') | KeyCode::Char('h') => {
            state.toggle_help();
            AppAction::Redraw
        }

        // Zoom steppers (`+`/`=` and `-`/`_`).
        KeyCode::Char('+') | KeyCode::Char('=') => {
            state.clear_digits();
            if state.nav_mut().zoom_in() {
                AppAction::ScaleChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            state.clear_digits();
            if state.nav_mut().zoom_out() {
                AppAction::ScaleChanged
            } else {
                AppAction::Redraw
            }
        }

        // Toggle Presenter Mode
        KeyCode::Char('p') => {
            state.toggle_presenter_mode();
            AppAction::Redraw
        }

        // Toggle Sorter Grid Mode
        KeyCode::Tab | KeyCode::Char('s') => {
            state.clear_digits();
            state.toggle_sorter();
            AppAction::Redraw
        }

        // Play Media on current slide
        KeyCode::Char('m') => {
            let msg = if let Some(slide) = state.current_slide_model() {
                crate::app::media::try_play_current_slide_media(slide)
                    .unwrap_or_else(|| "No media found on current slide".to_string())
            } else {
                "No media found on current slide".to_string()
            };
            state.set_toast(msg);
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
}
