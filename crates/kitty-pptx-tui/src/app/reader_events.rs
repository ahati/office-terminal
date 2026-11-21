use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use crate::app::events::AppAction;
use crate::app::reader_scroll::VERTICAL_GUTTER_ROWS;
use crate::app::reader_state::{ReaderLayout, Slot};
use crate::app::state::PresentationState;
use crate::app::ui::reader_layout::plan_row_columns;

/// Handles keyboard events when the base mode is [`AppMode::Reader`] (PRD 14a §10).
pub fn handle_reader_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    if state.reader_state().is_none() {
        return AppAction::None;
    }

    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,

        // Column scroll (vertical continuous flow). j/k report `Redraw`
        // even when clamped (vim feedback); arrows report `None` when
        // nothing moved.
        KeyCode::Char('j') => scroll_vertical(state, column_step(state, 1), AppAction::Redraw),
        KeyCode::Char('k') => scroll_vertical(state, column_step(state, -1), AppAction::Redraw),
        KeyCode::Down => scroll_vertical(state, column_step(state, 1), AppAction::None),
        KeyCode::Up => scroll_vertical(state, column_step(state, -1), AppAction::None),

        // Horizontal navigation: arrows may advance rows, h/l only pan.
        KeyCode::Right => step_horizontal(state, 1),
        KeyCode::Left => step_horizontal(state, -1),

        // Half-viewport column jumps
        KeyCode::Char('d') => half_page_scroll(state, 1),
        KeyCode::Char('u') => half_page_scroll(state, -1),

        // Horizontal pan (clamped to overflow)
        KeyCode::Char('h') => pan_horizontal(state, -1),
        KeyCode::Char('l') => pan_horizontal(state, 1),

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
            reflow_row_after_view_change(state);
            AppAction::Redraw
        }
        KeyCode::Char('+') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(1.1);
            }
            reflow_row_after_view_change(state);
            AppAction::Redraw
        }
        KeyCode::Char('-') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.adjust_zoom(0.9);
            }
            reflow_row_after_view_change(state);
            AppAction::Redraw
        }
        KeyCode::Char('=') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.reset_view();
            }
            reflow_row_after_view_change(state);
            AppAction::Redraw
        }
        KeyCode::Char('Z') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.apply_zoom_all();
            }
            reflow_row_after_view_change(state);
            AppAction::Redraw
        }

        // Layout toggles
        KeyCode::Char('2') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.toggle_layout();
            }
            AppAction::Redraw
        }
        KeyCode::Char('H') => {
            if let Some(reader) = state.reader_state_mut() {
                reader.toggle_horizontal();
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
            // Horizontal streams rest their final page at the end clamp,
            // where it sits centered when narrower than the viewport — not
            // at the page's left edge (PRD 14a §6.6). Computed before the
            // mutable borrow below.
            let horizontal_end_scroll = state
                .reader_state()
                .filter(|r| r.layout == ReaderLayout::Horizontal && r.total_pages() > 1)
                .map(|r| {
                    let last = r.total_pages() - 1;
                    let (_, viewport_h_px, _, _) = reader_viewport_metrics(state);
                    let (pt_w, pt_h) = r.page_size_pt(last);
                    let ar = (pt_w / pt_h).max(0.1);
                    ((viewport_h_px * ar).max(50.0) as i64).saturating_sub(1)
                });
            if let Some(reader) = state.reader_state_mut() {
                let total = reader.total_pages();
                reader.jump_to_page(total.saturating_sub(1));
                if let Some(end_scroll) = horizontal_end_scroll {
                    reader.scroll_px = end_scroll;
                    reader.snap_display();
                }
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

/// Handles mouse events when the base mode is [`AppMode::Reader`].
///
/// Wheel notches scroll the column through the same path as the j/k keys
/// (three cell heights per notch, direction-correct); every other kind is
/// ignored and keeps the default hyperlink hit-testing route. Overlay modals
/// keep the wheel inert, mirroring how key routing defers to overlays first.
pub fn handle_reader_mouse(mouse: MouseEvent, state: &mut PresentationState) -> AppAction {
    if state.reader_state().is_none() || state.mode().is_overlay() || state.is_blank() {
        return AppAction::None;
    }
    let dir = match mouse.kind {
        MouseEventKind::ScrollDown => 1,
        MouseEventKind::ScrollUp => -1,
        _ => return AppAction::None,
    };
    let (_, _, _, cell_h_px) = reader_viewport_metrics(state);
    let notch = ((cell_h_px * 3.0).round() as i64).max(1);
    scroll_vertical(state, dir * notch, AppAction::None)
}

/// Signed scroll step shared by the j/k and Up/Down keys: one eighth of the
/// current row stride (min 24 px) in `dir`'s direction.
fn column_step(state: &PresentationState, dir: i64) -> i64 {
    let Some(reader) = state.reader_state() else {
        return 0;
    };
    let (viewport_w_px, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(state);
    let slot_w_px = reader.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
    dir * reader.scroll_step_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px)
}

/// Shared vertical column scroll (j/k, Up/Down, mouse wheel).
///
/// Returns `SlideChanged` when the anchor advanced, `Redraw` when the
/// offset moved, and `idle` when the column is clamped at a document
/// boundary and nothing moved.
fn scroll_vertical(state: &mut PresentationState, delta_px: i64, idle: AppAction) -> AppAction {
    let (viewport_w_px, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(state);
    let mut anchor_changed = false;
    let mut moved = false;
    if let Some(reader) = state.reader_state_mut() {
        let slot_w_px = reader.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
        let row_stride = reader.row_stride_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px);
        let base_h = row_stride - (VERTICAL_GUTTER_ROWS as f32) * cell_h_px;
        let geo = crate::app::reader_scroll::RowGeometry {
            viewport_h_px,
            slot_w_px,
            cell_w_px,
            cell_h_px,
        };
        let (ach, mv) = reader.scroll_column_detailed(delta_px, row_stride, base_h, geo);
        anchor_changed = ach;
        moved = mv;
    }
    if anchor_changed {
        if let Some(reader) = state.reader_state() {
            state.set_current_slide(reader.focused_page());
        }
        AppAction::SlideChanged
    } else if moved {
        AppAction::Redraw
    } else {
        idle
    }
}

/// Half-viewport column jump shared by `d` (down) and `u` (up).
fn half_page_scroll(state: &mut PresentationState, dir: i64) -> AppAction {
    let (viewport_w_px, viewport_h_px, cell_w_px, cell_h_px) = reader_viewport_metrics(state);
    if let Some(reader) = state.reader_state_mut() {
        let slot_w_px = reader.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
        let row_stride = reader.row_stride_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px);
        let base_h = row_stride - (VERTICAL_GUTTER_ROWS as f32) * cell_h_px;
        let step = ((viewport_h_px / 2.0).round() as i64).max(1);
        let geo = crate::app::reader_scroll::RowGeometry {
            viewport_h_px,
            slot_w_px,
            cell_w_px,
            cell_h_px,
        };
        reader.scroll_column(dir * step, row_stride, base_h, geo);
    }
    AppAction::Redraw
}

/// `max_scroll_x` recorded for `page` by the last drawn frame (0 when the
/// page was not placed).
fn page_pan_max(state: &PresentationState, page: usize) -> u32 {
    state
        .reader_placements_last()
        .iter()
        .find(|p| p.page == page)
        .map(|p| p.max_scroll_x)
        .unwrap_or(0)
}

/// Arrow-key horizontal step: slides the horizontal strip, pans the zoomed
/// row/page, and advances the row when nothing overflows.
fn step_horizontal(state: &mut PresentationState, dir: i64) -> AppAction {
    state.clear_digits();
    let (_, viewport_h_px, cell_w_px, _) = reader_viewport_metrics(state);

    // Pan bound of the focused page from the last drawn frame, resolved
    // before the mutable reader borrow (only zoomed pages have overflow).
    let focused_pan_max = match state.reader_state() {
        Some(reader) => {
            let page = reader.focused_page();
            if reader.page_view(page).zoom > 1.0 {
                page_pan_max(state, page)
            } else {
                0
            }
        }
        None => 0,
    };

    let mut slide_to_set = None;
    let mut action = AppAction::Redraw;

    if let Some(reader) = state.reader_state_mut() {
        if reader.layout == ReaderLayout::Horizontal {
            let (pt_w, pt_h) = reader.page_size_pt(reader.anchor);
            let ar = (pt_w / pt_h).max(0.1);
            let page_w_px = (viewport_h_px * ar).max(50.0);
            let step = ((page_w_px / 8.0).round() as i64).max(24);
            let (anchor_changed, moved) = reader.scroll_horizontal_continuous(dir * step, page_w_px);
            if anchor_changed {
                slide_to_set = Some(reader.focused_page());
                action = AppAction::SlideChanged;
            } else if moved {
                action = AppAction::Redraw;
            } else {
                action = AppAction::None;
            }
        } else {
            let max_row_x = reader.max_row_scroll_x();
            if max_row_x > 0 {
                let step = ((cell_w_px * 3.0).round() as i64).max(24);
                if reader.scroll_row_horizontal(dir * step) {
                    action = AppAction::Redraw;
                } else {
                    action = AppAction::None;
                }
            } else {
                let max_x = focused_pan_max;
                if max_x > 0 {
                    if reader.scroll_horizontal_bounded(dir as i32 * 24, max_x) {
                        action = AppAction::Redraw;
                    } else {
                        action = AppAction::None;
                    }
                } else {
                    if dir > 0 {
                        reader.next_row();
                    } else {
                        reader.prev_row();
                    }
                    slide_to_set = Some(reader.focused_page());
                    action = AppAction::SlideChanged;
                }
            }
        }
    }

    if let Some(slide) = slide_to_set {
        state.set_current_slide(slide);
    }
    action
}

/// Vim h/l pan: clamped horizontal pan that never advances rows and keeps
/// reporting `Redraw` (only a horizontal strip that cannot move reports
/// `None`).
fn pan_horizontal(state: &mut PresentationState, dir: i64) -> AppAction {
    let (_, viewport_h_px, _, _) = reader_viewport_metrics(state);

    // Pan bound of the focused page from the last drawn frame, resolved
    // before the mutable reader borrow.
    let focused_pan_max = state
        .reader_state()
        .map(|reader| page_pan_max(state, reader.focused_page()))
        .unwrap_or(0);

    let mut slide_to_set = None;
    let mut action = AppAction::Redraw;

    if let Some(reader) = state.reader_state_mut() {
        if reader.layout == ReaderLayout::Horizontal {
            let (pt_w, pt_h) = reader.page_size_pt(reader.anchor);
            let ar = (pt_w / pt_h).max(0.1);
            let page_w_px = (viewport_h_px * ar).max(50.0);
            let step = ((page_w_px / 8.0).round() as i64).max(24);
            let (anchor_changed, moved) = reader.scroll_horizontal_continuous(dir * step, page_w_px);
            if anchor_changed {
                slide_to_set = Some(reader.focused_page());
                action = AppAction::SlideChanged;
            } else if !moved {
                action = AppAction::None;
            }
        } else {
            let max_row_x = reader.max_row_scroll_x();
            if max_row_x > 0 {
                reader.scroll_row_horizontal(dir * 24);
            } else {
                let max_x = focused_pan_max;
                reader.scroll_horizontal_bounded(dir as i32 * 24, max_x);
            }
        }
    }

    if let Some(slide) = slide_to_set {
        state.set_current_slide(slide);
    }
    action
}

/// Reader viewport metrics for the current terminal grid (cached cell probe
/// and canonical floors; see `reader_types::reader_viewport_metrics`).
fn reader_viewport_metrics(state: &PresentationState) -> (f32, f32, f32, f32) {
    let (cols, rows) = state.layout_size();
    crate::app::reader_types::reader_viewport_metrics(cols, rows)
}

/// Row reflow check after a zoom-affecting view change (`+`/`-`/`=`/`Z`).
///
/// The render path recomputes the column plan from the anchor page's
/// zoomed width every frame (PRD 14a §6.3), so zooming in can drop columns
/// and the row the user was reading may no longer hold the focused pane.
/// When that happens, the focused page becomes the new row lead with a
/// page-turn snap — the zoom follows the page under the cursor instead of
/// silently scrolling it off. Zooming out only ever adds columns, so the
/// anchor is untouched there. `Single`/`Horizontal` rows always hold their
/// lead pane, and neither needs this check.
fn reflow_row_after_view_change(state: &mut PresentationState) {
    let (viewport_w_px, viewport_h_px, cell_w_px, _) = reader_viewport_metrics(state);
    let viewport_cols = (viewport_w_px / cell_w_px).round() as u16;
    let (focused, anchor, layout, total) = match state.reader_state() {
        Some(reader) => (
            reader.focused_page(),
            reader.anchor,
            reader.layout,
            reader.total_pages(),
        ),
        None => return,
    };
    if layout != ReaderLayout::Spread || total <= 1 {
        return;
    }

    let mut re_anchored = None;
    if let Some(reader) = state.reader_state_mut() {
        let planned = plan_row_columns(
            viewport_cols,
            viewport_h_px,
            cell_w_px,
            reader.dominant_page_pt(),
            reader.default_zoom,
        ) as usize;
        if focused >= anchor.saturating_add(planned) {
            // Page-turn snap onto the focused page as the new row lead.
            reader.anchor = focused;
            reader.focused_slot = 0;
            reader.focus = Slot::Left;
            reader.scroll_px = 0;
            reader.snap_display();
            reader.set_row_scroll_x(0);
            re_anchored = Some(focused);
        }
    }
    if let Some(page) = re_anchored {
        state.set_current_slide(page);
    }
}
