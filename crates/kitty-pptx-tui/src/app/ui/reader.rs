use kitty_pptx_display::{probe_cell_pixel_size, CellRect, ImageId, PixelRect};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::reader_state::{ReaderFitMode, ReaderLayout, ReaderSlotPlacement};
use crate::app::state::PresentationState;

pub const MIN_PANE_COLS: u16 = 10;
pub const GUTTER_COLS: u16 = 2;
pub const VERTICAL_GUTTER_ROWS: u16 = 1;

/// Computed spatial metrics and image rects for a single reader pane.
struct PaneMetrics {
    cell_rect: CellRect,
    pixel_src: PixelRect,
    page_target: kitty_pptx_core::RenderTargetSpec,
    max_scroll_x: u32,
}

/// Context for computing layout of individual reader panes.
struct PaneLayoutContext<'a> {
    state: &'a PresentationState,
    fit_slot_w_px: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    viewport_h_px: f32,
}

fn compute_horizontal_pane_metrics(
    ctx: &PaneLayoutContext<'_>,
    page: usize,
    col_x: u16,
    max_avail_cols: u16,
    row_y: u16,
    max_avail_rows: u16,
    scroll_x_px: i64,
) -> Option<PaneMetrics> {
    let reader = ctx.state.reader_state()?;
    let view = reader.page_view(page);
    let (pt_w, pt_h) = reader.page_size_pt(page);
    let ar = (pt_w / pt_h).max(0.1);

    let pane_h_px = ctx.viewport_h_px * view.zoom;
    let pane_w_px = pane_h_px * ar;

    let full_rows = (pane_h_px / ctx.cell_h_px).round() as u16;
    let rows = full_rows.clamp(1, max_avail_rows);

    let visible_w_px = (pane_w_px - scroll_x_px.max(0) as f32).max(0.0);
    let full_cols = (visible_w_px / ctx.cell_w_px).round() as u16;
    let cols = full_cols.clamp(1, max_avail_cols);

    let cell_rect = CellRect {
        row: row_y,
        col: col_x,
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
    let src_x = ((scroll_x_px.max(0) as f32 * scale).round() as u32).min(target_w);
    let screen_w_px = (cell_rect.cols as f32) * ctx.cell_w_px;
    let src_w = ((screen_w_px * scale).round() as u32).min(target_w.saturating_sub(src_x));

    let pixel_src = PixelRect {
        x: src_x,
        y: 0,
        width: src_w.max(1),
        height: target_h,
    };

    Some(PaneMetrics {
        cell_rect,
        pixel_src,
        page_target,
        max_scroll_x: 0,
    })
}

fn compute_pane_metrics(
    ctx: &PaneLayoutContext<'_>,
    page: usize,
    col_x: u16,
    max_avail_cols: u16,
    row_y: u16,
    max_avail_rows: u16,
    scroll_px: i64,
) -> Option<PaneMetrics> {
    let reader = ctx.state.reader_state()?;
    let view = reader.page_view(page);
    let (pt_w, pt_h) = reader.page_size_pt(page);
    let ar = (pt_w / pt_h).max(0.1);

    let (base_w_px, base_h_px) = match view.fit {
        ReaderFitMode::FitWidth => (ctx.fit_slot_w_px, ctx.fit_slot_w_px / ar),
        ReaderFitMode::FitPage => {
            let h = ctx.viewport_h_px.min(ctx.fit_slot_w_px / ar);
            let w = h * ar;
            (w, h)
        }
    };

    let pane_w_px = base_w_px * view.zoom;
    let pane_h_px = base_h_px * view.zoom;

    let full_cols = (pane_w_px / ctx.cell_w_px).round() as u16;
    let cols = full_cols.clamp(1, max_avail_cols);

    let visible_h_px = (pane_h_px - scroll_px as f32).max(0.0);
    let full_rows = (visible_h_px / ctx.cell_h_px).round() as u16;
    let rows = full_rows.clamp(1, max_avail_rows);

    let cell_rect = CellRect {
        row: row_y,
        col: col_x,
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
    let screen_w_px = (cell_rect.cols as f32) * ctx.cell_w_px;
    let max_scroll_x = (pane_w_px - screen_w_px).max(0.0) as u32;

    let (src_w, src_x) = if max_scroll_x == 0 {
        (target_w, 0)
    } else {
        let scroll_x = view.scroll_x.min(max_scroll_x);
        let w = ((screen_w_px * scale).round() as u32).min(target_w);
        let x = ((scroll_x as f32 * scale).round() as u32).min(target_w.saturating_sub(w));
        (w, x)
    };

    let total_scroll_y = (scroll_px as f32 + view.scroll_y as f32).max(0.0);
    let src_y = ((total_scroll_y * scale).round() as u32).min(target_h);
    let screen_h_px = (cell_rect.rows as f32) * ctx.cell_h_px;
    let src_h = ((screen_h_px * scale).round() as u32).min(target_h.saturating_sub(src_y));

    let pixel_src = PixelRect {
        x: src_x,
        y: src_y,
        width: src_w.max(1),
        height: src_h.max(1),
    };

    Some(PaneMetrics {
        cell_rect,
        pixel_src,
        page_target,
        max_scroll_x,
    })
}

fn select_spread_image_id(page: usize, slot_idx: usize, num_cols: usize) -> ImageId {
    let row_parity = (page / num_cols.max(1)).is_multiple_of(2);
    let base_id = if row_parity {
        slot_idx as u32 + 1
    } else {
        slot_idx as u32 + 1 + (num_cols as u32).max(1)
    };
    ImageId(base_id.clamp(1, 12))
}

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

    let gutter_cols = GUTTER_COLS;
    let gutter_px = (gutter_cols as f32) * cell_w_px;

    let ctx = PaneLayoutContext {
        state,
        fit_slot_w_px: viewport_w_px,
        cell_w_px,
        cell_h_px,
        viewport_h_px,
    };

    let mut placements = Vec::new();

    // ─── Horizontal Continuous Mode ──────────────────────────────────────────────
    if reader.layout == ReaderLayout::Horizontal {
        let mut col_x = viewport_area.x;
        let mut p = reader.anchor;
        let mut slot_idx = 0;

        while p < reader.total_pages() && col_x < viewport_area.right() {
            let avail_cols = viewport_area.right().saturating_sub(col_x);
            if avail_cols < MIN_PANE_COLS && slot_idx > 0 {
                break;
            }
            let scroll_x = if p == reader.anchor { reader.scroll_px } else { 0 };
            if let Some(m) = compute_horizontal_pane_metrics(
                &ctx,
                p,
                col_x,
                avail_cols,
                viewport_area.y,
                viewport_area.height,
                scroll_x,
            ) {
                let id = ImageId(((slot_idx as u32) % 12) + 1);
                placements.push(ReaderSlotPlacement {
                    page: p,
                    id,
                    cell_rect: m.cell_rect,
                    pixel_src: m.pixel_src,
                    page_target: m.page_target,
                    max_scroll_x: m.max_scroll_x,
                });

                let is_focused = p == reader.focused_page();
                let border_style = if is_focused {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let block = Block::default()
                    .borders(if is_focused { Borders::BOTTOM } else { Borders::NONE })
                    .border_style(border_style);
                f.render_widget(
                    block,
                    Rect::new(m.cell_rect.col, m.cell_rect.row, m.cell_rect.cols, m.cell_rect.rows),
                );

                col_x = col_x.saturating_add(m.cell_rect.cols).saturating_add(gutter_cols);
                slot_idx += 1;
                p += 1;
            } else {
                break;
            }
        }

        state.update_reader_placements(placements);

        let status_text = reader.status_line(state.presentation_title().unwrap_or("document.pdf"));
        let status_style = Style::default().fg(Color::White).bg(Color::DarkGray);
        let status_paragraph = Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(status_text, status_style),
        ]))
        .style(status_style);
        f.render_widget(status_paragraph, status_area);
        return;
    }

    // ─── Vertical Reflow / Spread Mode ───────────────────────────────────────────
    let (pt_w, pt_h) = reader.page_size_pt(reader.anchor);
    let ar = (pt_w / pt_h).max(0.1);
    let page_w_px = viewport_h_px * ar;
    let ideal_cols = ((page_w_px / cell_w_px).round() as u16).max(MIN_PANE_COLS);

    let num_cols = match reader.layout {
        ReaderLayout::Single => 1,
        ReaderLayout::Spread => {
            if reader.total_pages() <= 1 {
                1
            } else {
                ((viewport_area.width + gutter_cols) / (ideal_cols + gutter_cols)).clamp(1, 6) as usize
            }
        }
        ReaderLayout::Horizontal => 1,
    };

    let is_multi = num_cols > 1;
    let slot_w_px = if is_multi {
        ((viewport_w_px - ((num_cols - 1) as f32 * gutter_px)) / num_cols as f32).max(10.0)
    } else {
        viewport_w_px
    };

    let ctx = PaneLayoutContext {
        state,
        fit_slot_w_px: slot_w_px,
        cell_w_px,
        cell_h_px,
        viewport_h_px,
    };

    let mut row_occupied_cells_h = 0u16;

    // ─── 1. Primary (Anchor) Row ─────────────────────────────────────────────────
    let primary_pages: Vec<usize> = if reader.cover_alone && reader.anchor == 0 {
        vec![0]
    } else {
        (0..num_cols)
            .map(|i| reader.anchor + i)
            .filter(|&p| p < reader.total_pages())
            .collect()
    };

    let mut cur_col = viewport_area.x;
    for (idx, &page) in primary_pages.iter().enumerate() {
        if cur_col >= viewport_area.right() {
            break;
        }
        let avail_cols = viewport_area.right().saturating_sub(cur_col);
        if avail_cols < MIN_PANE_COLS && idx > 0 {
            break;
        }

        if let Some(m) = compute_pane_metrics(
            &ctx,
            page,
            cur_col,
            avail_cols,
            viewport_area.y,
            viewport_area.height,
            reader.scroll_px,
        ) {
            row_occupied_cells_h = row_occupied_cells_h.max(m.cell_rect.rows);
            let id = select_spread_image_id(page, idx, num_cols);

            placements.push(ReaderSlotPlacement {
                page,
                id,
                cell_rect: m.cell_rect,
                pixel_src: m.pixel_src,
                page_target: m.page_target,
                max_scroll_x: m.max_scroll_x,
            });

            let is_focused = page == reader.focused_page();
            let border_style = if is_focused {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let block = Block::default()
                .borders(if is_focused { Borders::BOTTOM } else { Borders::NONE })
                .border_style(border_style);
            f.render_widget(
                block,
                Rect::new(m.cell_rect.col, m.cell_rect.row, m.cell_rect.cols, m.cell_rect.rows),
            );

            cur_col = cur_col.saturating_add(m.cell_rect.cols).saturating_add(gutter_cols);
        }
    }

    // ─── 2. Next Row (Continuous Scroll Transition with Vertical Gutter) ──────────
    if row_occupied_cells_h > 0 {
        let next_row_y = viewport_area.y + row_occupied_cells_h + VERTICAL_GUTTER_ROWS;
        let avail_next_rows = (viewport_area.y + viewport_area.height).saturating_sub(next_row_y);

        if avail_next_rows > 0 {
            let next_anchor = if reader.cover_alone && reader.anchor == 0 {
                1
            } else {
                reader.anchor + num_cols
            };

            if next_anchor < reader.total_pages() {
                let next_pages: Vec<usize> = (0..num_cols)
                    .map(|i| next_anchor + i)
                    .filter(|&p| p < reader.total_pages())
                    .collect();

                let mut next_col = viewport_area.x;
                for (idx, &np) in next_pages.iter().enumerate() {
                    if next_col >= viewport_area.right() {
                        break;
                    }
                    let avail_cols = viewport_area.right().saturating_sub(next_col);
                    if avail_cols < MIN_PANE_COLS && idx > 0 {
                        break;
                    }

                    if let Some(nm) = compute_pane_metrics(
                        &ctx,
                        np,
                        next_col,
                        avail_cols,
                        next_row_y,
                        avail_next_rows,
                        0,
                    ) {
                        let id = select_spread_image_id(np, idx, num_cols);
                        placements.push(ReaderSlotPlacement {
                            page: np,
                            id,
                            cell_rect: nm.cell_rect,
                            pixel_src: nm.pixel_src,
                            page_target: nm.page_target,
                            max_scroll_x: nm.max_scroll_x,
                        });

                        next_col = next_col.saturating_add(nm.cell_rect.cols).saturating_add(gutter_cols);
                    }
                }
            }
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
