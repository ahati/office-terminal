use crossterm::event::{KeyCode, KeyEvent};
use crate::app::events::AppAction;
use crate::app::state::PresentationState;

/// Handles keyboard events when the base mode is [`AppMode::Reader`] (PRD 14a §10).
pub fn handle_reader_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    if state.reader_state().is_none() {
        return AppAction::None;
    }

    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,

        // Vertical scroll
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(24);
            }
            AppAction::Redraw
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(-24);
            }
            AppAction::Redraw
        }
        KeyCode::Char('d') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(200);
            }
            AppAction::Redraw
        }
        KeyCode::Char('u') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(-200);
            }
            AppAction::Redraw
        }

        // Horizontal pan
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_horizontal(-24);
            }
            AppAction::Redraw
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_horizontal(24);
            }
            AppAction::Redraw
        }

        // Focus & Sync
        KeyCode::Tab | KeyCode::BackTab => {
            if let Some(reader) = state.reader_state_mut() {
                reader.toggle_focus();
            }
            AppAction::Redraw
        }
        KeyCode::Char('y') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.set_sync(true);
            }
            AppAction::Redraw
        }

        // Fit & Zoom
        KeyCode::Char('f') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.cycle_fit();
            }
            AppAction::ScaleChanged
        }
        KeyCode::Char('+') | KeyCode::Char('=') if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(1.1);
            }
            AppAction::ScaleChanged
        }
        KeyCode::Char('+') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(1.1);
            }
            AppAction::ScaleChanged
        }
        KeyCode::Char('-') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(0.9);
            }
            AppAction::ScaleChanged
        }
        KeyCode::Char('=') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.reset_view();
            }
            AppAction::ScaleChanged
        }
        KeyCode::Char('Z') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.apply_zoom_all();
            }
            AppAction::ScaleChanged
        }

        // Layout toggles
        KeyCode::Char('2') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.toggle_layout();
            }
            AppAction::Redraw
        }
        KeyCode::Char('!') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.toggle_cover_alone();
            }
            AppAction::Redraw
        }

        // Page / Row navigation
        KeyCode::PageDown | KeyCode::Char('n') | KeyCode::Char(' ') | KeyCode::Enter => {
            if !state.digit_buffer().is_empty() {
                if let Ok(target) = state.digit_buffer().parse::<usize>() {
                    state.clear_digits();
                    if let Some(reader) = state.reader_state_mut() {
                        reader.jump_to_page(target.saturating_sub(1));
                    }
                    return AppAction::SlideChanged;
                }
                state.clear_digits();
            }
            if let Some(reader) = state.reader_state_mut() {
                reader.next_row();
            }
            AppAction::SlideChanged
        }
        KeyCode::PageUp | KeyCode::Char('p') => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.prev_row();
            }
            AppAction::SlideChanged
        }
        KeyCode::Char('g') | KeyCode::Home => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.jump_to_page(0);
            }
            AppAction::SlideChanged
        }
        KeyCode::Char('G') | KeyCode::End => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                let total = reader.total_pages();
                reader.jump_to_page(total.saturating_sub(1));
            }
            AppAction::SlideChanged
        }

        // Numeric slide jumping
        KeyCode::Char(c) if c.is_ascii_digit() => {
            state.push_digit(c);
            AppAction::Redraw
        }

        // TUI overlays & views
        KeyCode::Char('?') => {
            state.toggle_help();
            AppAction::Redraw
        }
        KeyCode::Char('b') => {
            state.toggle_blank(crate::app::state::BlankMode::Black);
            AppAction::Redraw
        }
        KeyCode::Char('t') => {
            state.toggle_sorter();
            AppAction::Redraw
        }
        KeyCode::Char('/') => {
            state.open_search();
            AppAction::Redraw
        }

        _ => AppAction::None,
    }
}
