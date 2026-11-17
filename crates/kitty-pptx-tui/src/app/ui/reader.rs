use kitty_pptx_display::{probe_cell_pixel_size, CellRect, ImageId, PixelRect};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::reader_state::{ReaderFitMode, ReaderLayout, ReaderSlotPlacement, Slot};
use crate::app::state::PresentationState;

pub const MIN_PANE_COLS: u16 = 10;
pub const GUTTER_COLS: u16 = 2;

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

fn select_image_id(page: usize, is_spread: bool, slot: Slot) -> ImageId {
    if is_spread {
        match slot {
            Slot::Left => {
                if (page / 2).is_multiple_of(2) {
                    ImageId(1)
                } else {
                    ImageId(3)
                }
            }
            Slot::Right => {
                if (page / 2).is_multiple_of(2) {
                    ImageId(2)
                } else {
                    ImageId(4)
                }
            }
        }
    } else {
        if page.is_multiple_of(2) {
            ImageId(1)
        } else {
            ImageId(3)
        }
    }
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

    let vis = reader.visible_pages();
    let min_spread_cols = 2 * MIN_PANE_COLS + gutter_cols;
    let is_spread = reader.layout == ReaderLayout::Spread
        && reader.total_pages() > 1
        && viewport_area.width >= min_spread_cols;

    let slot_w_px = if is_spread {
        ((viewport_w_px - gutter_px) / 2.0).max(10.0)
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

    let mut placements = Vec::new();

    // ─── 1. Primary (Anchor) Row ─────────────────────────────────────────────────
    if let Some(&left_page) = vis.first() {
        let left_m = compute_pane_metrics(
            &ctx,
            left_page,
            viewport_area.x,
            viewport_area.width,
            viewport_area.y,
            viewport_area.height,
            reader.scroll_px,
        );

        if let Some(lm) = left_m {
            let left_id = select_image_id(left_page, is_spread, Slot::Left);
            let row_occupied_cells_h = lm.cell_rect.rows;

            placements.push(ReaderSlotPlacement {
                page: left_page,
                id: left_id,
                cell_rect: lm.cell_rect,
                pixel_src: lm.pixel_src,
                page_target: lm.page_target,
                max_scroll_x: lm.max_scroll_x,
            });

            // Focus and border indicator
            let is_focused = reader.focus == Slot::Left;
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
                Rect::new(lm.cell_rect.col, lm.cell_rect.row, lm.cell_rect.cols, lm.cell_rect.rows),
            );

            // Partner pane in anchor row
            if is_spread && vis.len() > 1 {
                let right_page = vis[1];
                let start_col = viewport_area.x + lm.cell_rect.cols + gutter_cols;
                if start_col < viewport_area.right() {
                    let avail_partner_cols = viewport_area.right().saturating_sub(start_col);
                    if avail_partner_cols >= MIN_PANE_COLS {
                        if let Some(rm) = compute_pane_metrics(
                            &ctx,
                            right_page,
                            start_col,
                            avail_partner_cols,
                            viewport_area.y,
                            viewport_area.height,
                            reader.scroll_px,
                        ) {
                            let right_id = select_image_id(right_page, is_spread, Slot::Right);
                            placements.push(ReaderSlotPlacement {
                                page: right_page,
                                id: right_id,
                                cell_rect: rm.cell_rect,
                                pixel_src: rm.pixel_src,
                                page_target: rm.page_target,
                                max_scroll_x: rm.max_scroll_x,
                            });

                            let is_r_focused = reader.focus == Slot::Right;
                            let r_border_style = if is_r_focused {
                                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::DarkGray)
                            };
                            let r_block = Block::default()
                                .borders(if is_r_focused { Borders::BOTTOM } else { Borders::NONE })
                                .border_style(r_border_style);
                            f.render_widget(
                                r_block,
                                Rect::new(rm.cell_rect.col, rm.cell_rect.row, rm.cell_rect.cols, rm.cell_rect.rows),
                            );
                        }
                    }
                }
            }

            // ─── 2. Next Row (Continuous Scroll Transition) ──────────────────────────
            let next_row_y = viewport_area.y + row_occupied_cells_h;
            let avail_next_rows = (viewport_area.y + viewport_area.height).saturating_sub(next_row_y);

            if avail_next_rows > 0 {
                let next_anchor = match reader.layout {
                    ReaderLayout::Single => reader.anchor + 1,
                    ReaderLayout::Spread => {
                        if reader.cover_alone && reader.anchor == 0 {
                            1
                        } else {
                            reader.anchor + 2
                        }
                    }
                };

                if next_anchor < reader.total_pages() {
                    let next_left_m = compute_pane_metrics(
                        &ctx,
                        next_anchor,
                        viewport_area.x,
                        viewport_area.width,
                        next_row_y,
                        avail_next_rows,
                        0,
                    );

                    if let Some(nlm) = next_left_m {
                        let n_left_id = select_image_id(next_anchor, is_spread, Slot::Left);
                        placements.push(ReaderSlotPlacement {
                            page: next_anchor,
                            id: n_left_id,
                            cell_rect: nlm.cell_rect,
                            pixel_src: nlm.pixel_src,
                            page_target: nlm.page_target,
                            max_scroll_x: nlm.max_scroll_x,
                        });

                        // Partner pane in next row
                        if is_spread && next_anchor + 1 < reader.total_pages() {
                            let next_right = next_anchor + 1;
                            let n_start_col = viewport_area.x + nlm.cell_rect.cols + gutter_cols;
                            if n_start_col < viewport_area.right() {
                                let n_avail_cols = viewport_area.right().saturating_sub(n_start_col);
                                if n_avail_cols >= MIN_PANE_COLS {
                                    if let Some(nrm) = compute_pane_metrics(
                                        &ctx,
                                        next_right,
                                        n_start_col,
                                        n_avail_cols,
                                        next_row_y,
                                        avail_next_rows,
                                        0,
                                    ) {
                                        let n_right_id = select_image_id(next_right, is_spread, Slot::Right);
                                        placements.push(ReaderSlotPlacement {
                                            page: next_right,
                                            id: n_right_id,
                                            cell_rect: nrm.cell_rect,
                                            pixel_src: nrm.pixel_src,
                                            page_target: nrm.page_target,
                                            max_scroll_x: nrm.max_scroll_x,
                                        });
                                    }
                                }
                            }
                        }
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
