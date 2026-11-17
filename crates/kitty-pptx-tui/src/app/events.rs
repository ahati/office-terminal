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

    // 6. If in Reader Mode
    if state.base_mode() == AppMode::Reader {
        return crate::app::reader_events::handle_reader_key(key, state);
    }

    // 7. Normal / Presenter mode key handling
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
        KeyCode::Left => {
            if state.search_state().query().is_empty() {
                state.search_collapse();
            } else {
                state.search_backspace();
            }
            AppAction::Redraw
        }
        KeyCode::Right => {
            if state.search_state().query().is_empty() {
                state.search_expand();
            }
            AppAction::Redraw
        }
        KeyCode::Char(' ') if state.search_state().query().is_empty() => {
            if !state.search_expand() {
                state.search_collapse();
            }
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
#[path = "events_tests.rs"]
mod events_tests;
