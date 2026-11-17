use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::PresentationState;
use crate::widget::SlideWidget;

/// Render the interactive thumbnail slide sorter grid.
pub fn render_sorter(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header bar
            Constraint::Min(4),    // Grid cards area
            Constraint::Length(1), // Footer status line
        ])
        .split(area);

    let header_area = chunks[0];
    let grid_area = chunks[1];
    let footer_area = chunks[2];

    // 1. Header Bar
    let header_line = Line::from(vec![
        Span::styled(
            " SLIDE SORTER ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            " Total: {} slides | Cursor: #{} | Active: #{} ",
            state.total_slides(),
            state.selected_sorter_index() + 1,
            state.current_slide() + 1
        )),
        Span::styled(
            "[Arrows] Move  [Enter] Select  [Tab/Esc/s] Return  [m] Media  [?] Help",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(header_line), header_area);

    // 2. Multi-column Grid
    let total = state.total_slides();
    if total == 0 {
        let empty_msg = Paragraph::new("No slides loaded in presentation")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty_msg, grid_area);
        return;
    }

    let cols = state.sorter_cols().max(1);
    let card_height = (grid_area.height / 3).clamp(5, 12);

    // Never plan more cards than the terminal image quota and image-id pool can keep resident; the
    // graphics syncer would otherwise leave the overflow blank.
    let (w_emu, h_emu) = state.slide_dimensions();
    let ar = if h_emu > 0 { w_emu as f32 / h_emu as f32 } else { 16.0 / 9.0 };
    let (target_w, target_h) = if ar >= 1.0 {
        (2560u32, ((2560.0 / ar).round() as u32).clamp(1024, 8192))
    } else {
        (((2560.0 * ar).round() as u32).clamp(1024, 8192), 2560u32)
    };
    let canvas_target = kitty_pptx_core::RenderTargetSpec {
        width_px: target_w,
        height_px: target_h,
    };
    let max_cards = crate::app::graphics_thumbnails::max_sorter_cards_for_target(&canvas_target);
    let capacity_rows = (max_cards / cols).max(1) as u16;
    let rows_visible = (grid_area.height / card_height).max(1).min(capacity_rows);
    let cursor = state.selected_sorter_index();
    let cursor_row = cursor / cols;

    let scroll_row = state.sorter_scroll_row_with_hysteresis(cursor_row, rows_visible as usize);

    let start_idx = scroll_row * cols;
    let num_rows_to_render =
        rows_visible.min(total.saturating_sub(start_idx).div_ceil(cols) as u16);

    let mut row_constraints = Vec::with_capacity(num_rows_to_render as usize + 1);
    for _ in 0..num_rows_to_render {
        row_constraints.push(Constraint::Length(card_height));
    }
    row_constraints.push(Constraint::Min(0));

    let row_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(grid_area);

    let col_constraints = vec![Constraint::Ratio(1, cols as u32); cols];

    for r in 0..num_rows_to_render {
        let row_area = row_rects[r as usize];
        let col_rects = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints.clone())
            .split(row_area);

        for c in 0..cols {
            let idx = start_idx + (r as usize) * cols + c;
            if idx >= total {
                break;
            }

            let card_rect = col_rects[c];
            let is_cursor = idx == state.selected_sorter_index();
            let is_active = idx == state.current_slide();
            let has_media = state.slide_has_media(idx);

            let media_tag = if has_media { " [M]" } else { "" };
            let (border_type, border_style, title_text) = if is_cursor && is_active {
                (
                    BorderType::Double,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    format!(" ▶ Slide {} (Active){} ◀ ", idx + 1, media_tag),
                )
            } else if is_cursor {
                (
                    BorderType::Thick,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    format!(" ▶ Slide {}{} ◀ ", idx + 1, media_tag),
                )
            } else if is_active {
                (
                    BorderType::Double,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    format!(" Slide {} (Active){} ", idx + 1, media_tag),
                )
            } else {
                (
                    BorderType::Plain,
                    Style::default().fg(Color::DarkGray),
                    format!(" Slide {}{} ", idx + 1, media_tag),
                )
            };

            let block = Block::default()
                .title(title_text)
                .borders(Borders::ALL)
                .border_type(border_type)
                .border_style(border_style);

            let inner = block.inner(card_rect);
            f.render_widget(block, card_rect);

            if inner.width > 0 && inner.height > 0 {
                let card = SlideWidget::thumbnail(idx, state.thumbnail_placement_sink())
                    .with_aspect_ratio(state.aspect_ratio());
                f.render_widget(card, inner);
            }
        }
    }

    // 3. Footer Status Line
    let footer_line = if let Some(toast) = state.toast() {
        Line::from(vec![Span::styled(
            format!(" 📢 Notification: {} ", toast),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )])
    } else {
        let total_rows = total.div_ceil(cols);
        Line::from(vec![
            Span::styled(
                format!(
                    " Row {}/{} | Slide {}/{} ",
                    scroll_row + 1,
                    total_rows,
                    state.selected_sorter_index() + 1,
                    total
                ),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | "),
            Span::styled(
                "[Enter] Open Slide | [Esc/Tab/s] Cancel | [Home/End] First/Last | [PgUp/PgDn] Page",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };
    f.render_widget(Paragraph::new(footer_line), footer_area);
}
