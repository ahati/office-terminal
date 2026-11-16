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
        KeyCode::Char('j') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(24);
            }
            AppAction::Redraw
        }
        KeyCode::Char('k') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_vertical(-24);
            }
            AppAction::Redraw
        }
        KeyCode::Down => {
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;
            let step = (cell_h_px * 2.0).round() as i32;

            let mut advanced = false;
            if let Some(reader) = state.reader_state_mut() {
                let page = reader.focused_page();
                let view = reader.page_view(page);
                let (pt_w, pt_h) = reader.page_size_pt(page);
                let ar = (pt_w / pt_h).max(0.1);
                let slot_w = reader.slot_width(viewport_w_px, cell_w_px);
                let pane_w = slot_w * view.zoom;
                let pane_h = pane_w / ar;
                let max_y = (pane_h - viewport_h_px).max(0.0) as u32;

                if view.fit == crate::app::reader_state::ReaderFitMode::FitPage
                    || max_y == 0
                    || !reader.scroll_vertical_bounded(step, max_y)
                {
                    reader.next_row();
                    advanced = true;
                }
            }
            if advanced {
                if let Some(reader) = state.reader_state() {
                    state.set_current_slide(reader.focused_page());
                }
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Up => {
            let mut retreated = false;
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let step = (cell_h_px * 2.0).round() as i32;

            if let Some(reader) = state.reader_state_mut() {
                let page = reader.focused_page();
                let view = reader.page_view(page);
                if view.fit == crate::app::reader_state::ReaderFitMode::FitPage
                    || view.scroll_y == 0
                    || !reader.scroll_vertical_bounded(-step, u32::MAX)
                {
                    reader.prev_row();
                    retreated = true;
                }
            }
            if retreated {
                if let Some(reader) = state.reader_state() {
                    state.set_current_slide(reader.focused_page());
                }
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
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
        KeyCode::Char('h') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_horizontal(-24);
            }
            AppAction::Redraw
        }
        KeyCode::Char('l') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.scroll_horizontal(24);
            }
            AppAction::Redraw
        }

        // Page turn with arrow keys
        KeyCode::Right => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.next_row();
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
            }
            AppAction::SlideChanged
        }
        KeyCode::Left => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.prev_row();
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
            }
            AppAction::SlideChanged
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
                    if let Some(reader) = state.reader_state() {
                        state.set_current_slide(reader.focused_page());
                    }
                    return AppAction::SlideChanged;
                }
                state.clear_digits();
            }
            if let Some(reader) = state.reader_state_mut() {
                reader.next_row();
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
            }
            AppAction::SlideChanged
        }
        KeyCode::PageUp | KeyCode::Char('p') | KeyCode::Backspace => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.prev_row();
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
            }
            AppAction::SlideChanged
        }
        KeyCode::Char('g') | KeyCode::Home => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                reader.jump_to_page(0);
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
            }
            AppAction::SlideChanged
        }
        KeyCode::Char('G') | KeyCode::End => {
            state.clear_digits();
            if let Some(reader) = state.reader_state_mut() {
                let total = reader.total_pages();
                reader.jump_to_page(total.saturating_sub(1));
            }
            if let Some(reader) = state.reader_state() {
                state.set_current_slide(reader.focused_page());
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
