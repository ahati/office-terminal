use kitty_pptx_display::{probe_cell_pixel_size, CellRect, ImageId, PixelRect};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::reader_state::{ReaderFitMode, ReaderLayout, ReaderSlotPlacement, Slot};
use crate::app::state::PresentationState;

/// Renders the document reader UI with focusable left-aligned panes (PRD 14a §6).
pub fn render_reader(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }

    let Some(reader) = state.reader_state() else {
        return;
    };

    let viewport_height = area.height.saturating_sub(1);
    let viewport_area = Rect::new(area.x, area.y, area.width, viewport_height);
    let status_area = Rect::new(area.x, area.y + viewport_height, area.width, 1);

    let metrics = probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;

    let viewport_w_px = (viewport_area.width as f32) * cell_w_px;
    let viewport_h_px = (viewport_area.height as f32) * cell_h_px;

    let gutter_cols = 2u16;
    let gutter_px = (gutter_cols as f32) * cell_w_px;

    let vis = reader.visible_pages();
    let is_spread = reader.layout == ReaderLayout::Spread && vis.len() > 1;

    let slot_w_px = if is_spread {
        ((viewport_w_px - gutter_px) / 2.0).max(10.0)
    } else {
        viewport_w_px
    };

    let mut placements = Vec::new();

    // ─── Render Left Pane ────────────────────────────────────────────────────────
    if let Some(&left_page) = vis.first() {
        let view = reader.page_view(left_page);
        let (pt_w, pt_h) = reader.page_size_pt(left_page);
        let ar = (pt_w / pt_h).max(0.1);

        let (base_w_px, base_h_px) = match view.fit {
            ReaderFitMode::FitWidth => (slot_w_px, slot_w_px / ar),
            ReaderFitMode::FitPage => {
                let h = viewport_h_px.min(slot_w_px / ar);
                let w = h * ar;
                (w, h)
            }
        };

        let pane_w_px = base_w_px * view.zoom;
        let pane_h_px = base_h_px * view.zoom;

        let left_max_cols = if is_spread {
            (viewport_area.width.saturating_sub(gutter_cols)) / 2
        } else {
            viewport_area.width
        };
        let cols = ((pane_w_px / cell_w_px).round() as u16).clamp(1, left_max_cols);
        let rows = ((pane_h_px / cell_h_px).round() as u16).clamp(1, viewport_area.height);

        let cell_rect = CellRect {
            row: viewport_area.y,
            col: viewport_area.x,
            rows,
            cols,
        };

        let target_w = 2560u32;
        let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
        let page_target = kitty_pptx_core::RenderTargetSpec {
            width_px: target_w,
            height_px: target_h,
        };

        let scale = target_w as f32 / pane_w_px.max(1.0);
        let screen_w_px = (cell_rect.cols as f32) * cell_w_px;
        let screen_h_px = (cell_rect.rows as f32) * cell_h_px;
        let full_page_h_px = pane_w_px / ar;
        let max_scroll_x = (pane_w_px - screen_w_px).max(0.0) as u32;
        let max_scroll_y = (full_page_h_px - screen_h_px).max(0.0) as u32;

        let scroll_x = view.scroll_x.min(max_scroll_x);
        let scroll_y = view.scroll_y.min(max_scroll_y);

        let (src_w, src_x) = if max_scroll_x == 0 {
            (target_w, 0)
        } else {
            let w = ((screen_w_px * scale).round() as u32).min(target_w);
            let x = ((scroll_x as f32 * scale).round() as u32).min(target_w.saturating_sub(w));
            (w, x)
        };

        let (src_h, src_y) = if max_scroll_y == 0 {
            (target_h, 0)
        } else {
            let h = ((screen_h_px * scale).round() as u32).min(target_h);
            let y = ((scroll_y as f32 * scale).round() as u32).min(target_h.saturating_sub(h));
            (h, y)
        };

        let pixel_src = PixelRect {
            x: src_x,
            y: src_y,
            width: src_w.max(1),
            height: src_h.max(1),
        };

        let left_id = match reader.layout {
            ReaderLayout::Single => {
                if left_page.is_multiple_of(2) {
                    ImageId(1)
                } else {
                    ImageId(3)
                }
            }
            ReaderLayout::Spread => {
                if (left_page / 2).is_multiple_of(2) {
                    ImageId(1)
                } else {
                    ImageId(3)
                }
            }
        };

        placements.push(ReaderSlotPlacement {
            page: left_page,
            id: left_id,
            cell_rect,
            pixel_src,
            page_target,
        });

        // Focus and placeholder indicator
        let is_focused = reader.focus == Slot::Left;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(if is_focused { Borders::BOTTOM } else { Borders::NONE })
            .border_style(border_style);
        f.render_widget(block, Rect::new(cell_rect.col, cell_rect.row, cell_rect.cols, cell_rect.rows));
    }

    // ─── Render Right Pane ───────────────────────────────────────────────────────
    if is_spread && vis.len() > 1 {
        let right_page = vis[1];
        let view = reader.page_view(right_page);
        let (pt_w, pt_h) = reader.page_size_pt(right_page);
        let ar = (pt_w / pt_h).max(0.1);

        let (base_w_px, base_h_px) = match view.fit {
            ReaderFitMode::FitWidth => (slot_w_px, slot_w_px / ar),
            ReaderFitMode::FitPage => {
                let h = viewport_h_px.min(slot_w_px / ar);
                let w = h * ar;
                (w, h)
            }
        };

        let pane_w_px = base_w_px * view.zoom;
        let pane_h_px = base_h_px * view.zoom;

        let left_cols = placements[0].cell_rect.cols;
        let start_col = viewport_area.x + left_cols + gutter_cols;

        if start_col < viewport_area.right() {
            let max_avail_cols = viewport_area.right().saturating_sub(start_col);
            let full_cols = (pane_w_px / cell_w_px).round() as u16;
            let cols = full_cols.min(max_avail_cols).clamp(1, max_avail_cols);
            let rows = ((pane_h_px / cell_h_px).round() as u16).clamp(1, viewport_area.height);

            let cell_rect = CellRect {
                row: viewport_area.y,
                col: start_col,
                rows,
                cols,
            };

            let target_w = 2560u32;
            let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
            let page_target = kitty_pptx_core::RenderTargetSpec {
                width_px: target_w,
                height_px: target_h,
            };

            let scale = target_w as f32 / pane_w_px.max(1.0);
            let screen_w_px = (cell_rect.cols as f32) * cell_w_px;
            let screen_h_px = (cell_rect.rows as f32) * cell_h_px;
            let full_page_h_px = pane_w_px / ar;
            let max_scroll_x = (pane_w_px - screen_w_px).max(0.0) as u32;
            let max_scroll_y = (full_page_h_px - screen_h_px).max(0.0) as u32;

            let scroll_x = view.scroll_x.min(max_scroll_x);
            let scroll_y = view.scroll_y.min(max_scroll_y);

            let (src_w, src_x) = if max_scroll_x == 0 {
                (target_w, 0)
            } else {
                let w = ((screen_w_px * scale).round() as u32).min(target_w);
                let x = ((scroll_x as f32 * scale).round() as u32).min(target_w.saturating_sub(w));
                (w, x)
            };

            let (src_h, src_y) = if max_scroll_y == 0 {
                (target_h, 0)
            } else {
                let h = ((screen_h_px * scale).round() as u32).min(target_h);
                let y = ((scroll_y as f32 * scale).round() as u32).min(target_h.saturating_sub(h));
                (h, y)
            };

            let pixel_src = PixelRect {
                x: src_x,
                y: src_y,
                width: src_w.max(1),
                height: src_h.max(1),
            };

            let right_id = if (right_page / 2).is_multiple_of(2) {
                ImageId(2)
            } else {
                ImageId(4)
            };

            placements.push(ReaderSlotPlacement {
                page: right_page,
                id: right_id,
                cell_rect,
                pixel_src,
                page_target,
            });

            let is_focused = reader.focus == Slot::Right;
            let border_style = if is_focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let block = Block::default()
                .borders(if is_focused { Borders::BOTTOM } else { Borders::NONE })
                .border_style(border_style);
            f.render_widget(block, Rect::new(cell_rect.col, cell_rect.row, cell_rect.cols, cell_rect.rows));
        }
    }

    state.update_reader_placements(placements);

    // ─── Status Line ─────────────────────────────────────────────────────────────
    let status_text = reader.status_line(state.presentation_title().unwrap_or("document.pdf"));
    let status_style = Style::default().fg(Color::White).bg(Color::DarkGray);
    let status_paragraph = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(status_text, status_style),
    ]))
    .style(status_style);
    f.render_widget(status_paragraph, status_area);
}
