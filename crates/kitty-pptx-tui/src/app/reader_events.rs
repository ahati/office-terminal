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

        // Column scroll (vertical continuous flow)
        KeyCode::Char('j') => {
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(24);
                reader.scroll_column(step, row_h);
            }
            AppAction::Redraw
        }
        KeyCode::Char('k') => {
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(24);
                reader.scroll_column(-step, row_h);
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

            let mut advanced = false;
            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(1);
                advanced = reader.scroll_column(step, row_h);
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
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            let mut retreated = false;
            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(1);
                retreated = reader.scroll_column(-step, row_h);
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
        KeyCode::Right => {
            state.clear_digits();
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            let mut moved = false;
            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(1);
                moved = reader.scroll_column(step, row_h);
            }
            if moved {
                if let Some(reader) = state.reader_state() {
                    state.set_current_slide(reader.focused_page());
                }
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Left => {
            state.clear_digits();
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            let mut moved = false;
            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((row_h / 8.0).round() as i64).max(1);
                moved = reader.scroll_column(-step, row_h);
            }
            if moved {
                if let Some(reader) = state.reader_state() {
                    state.set_current_slide(reader.focused_page());
                }
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Char('d') => {
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((viewport_h_px / 2.0).round() as i64).max(1);
                reader.scroll_column(step, row_h);
            }
            AppAction::Redraw
        }
        KeyCode::Char('u') => {
            let (cols, rows) = state.layout_size();
            let metrics = kitty_pptx_display::probe_cell_pixel_size();
            let cell_w_px = metrics.width_px.max(4.0) as f32;
            let cell_h_px = metrics.height_px.max(8.0) as f32;
            let viewport_w_px = (cols as f32) * cell_w_px;
            let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;

            if let Some(reader) = state.reader_state_mut() {
                let row_h = reader.current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
                let step = ((viewport_h_px / 2.0).round() as i64).max(1);
                reader.scroll_column(-step, row_h);
            }
            AppAction::Redraw
        }

        // Horizontal pan (clamped to overflow)
        KeyCode::Char('h') => {
            if let Some(reader) = state.reader_state_mut() {
                let page = reader.focused_page();
                let max_x = reader
                    .placements()
                    .iter()
                    .find(|p| p.page == page)
                    .map(|p| p.max_scroll_x)
                    .unwrap_or(0);
                reader.scroll_horizontal_bounded(-24, max_x);
            }
            AppAction::Redraw
        }
        KeyCode::Char('l') => {
            if let Some(reader) = state.reader_state_mut() {
                let page = reader.focused_page();
                let max_x = reader
                    .placements()
                    .iter()
                    .find(|p| p.page == page)
                    .map(|p| p.max_scroll_x)
                    .unwrap_or(0);
                reader.scroll_horizontal_bounded(24, max_x);
            }
            AppAction::Redraw
        }

        // Focus & Sync
        KeyCode::BackTab | KeyCode::Char('w') | KeyCode::Char('\\') => {
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
            AppAction::Redraw
        }
        KeyCode::Char('+') | KeyCode::Char('=') if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(1.1);
            }
            AppAction::Redraw
        }
        KeyCode::Char('+') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(1.1);
            }
            AppAction::Redraw
        }
        KeyCode::Char('-') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(0.9);
            }
            AppAction::Redraw
        }
        KeyCode::Char('=') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.reset_view();
            }
            AppAction::Redraw
        }
        KeyCode::Char('Z') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.apply_zoom_all();
            }
            AppAction::Redraw
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
        KeyCode::Tab | KeyCode::Char('t') | KeyCode::Char('s') => {
            state.clear_digits();
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
