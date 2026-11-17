use kitty_pptx_display::{probe_cell_pixel_size, ImageId};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub use crate::app::reader_scroll::VERTICAL_GUTTER_ROWS;
pub use crate::app::ui::reader_layout::{
    compute_horizontal_pane_metrics, compute_pane_metrics_cropped, select_spread_image_id,
    PaneCellArea, PaneCropSpan, PaneLayoutContext, GUTTER_COLS, MIN_PANE_COLS,
};
use crate::app::reader_types::{ReaderFitMode, ReaderLayout, ReaderSlotPlacement};
use crate::app::state::PresentationState;

/// Renders the current reader viewport (Single, Spread, or Horizontal mode).
pub fn render_reader(state: &PresentationState, f: &mut Frame<'_>, area: Rect) {
    if area.width < MIN_PANE_COLS || area.height < 3 {
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

        while p < reader.total_pages() && col_x < viewport_area.right() {
            let avail_cols = viewport_area.right().saturating_sub(col_x);
            if avail_cols < MIN_PANE_COLS && p > reader.anchor {
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
                // ImageId tied to page index so horizontal sliding produces 0 re-stages (P0-4)
                let id = ImageId(((p as u32) % 12) + 1);
                placements.push(ReaderSlotPlacement {
                    page: p,
                    id,
                    cell_rect: m.cell_rect,
                    pixel_src: m.pixel_src,
                    page_target: m.page_target,
                    max_scroll_x: m.max_scroll_x,
                    is_primary: true,
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
                p += 1;
            } else {
                break;
            }
        }

        let placed_cols = placements.len().max(1);
        state.set_reader_num_columns(placed_cols);
        reader.set_num_columns(placed_cols);
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

    // Publish computed column count to state (P0-2)
    state.set_reader_num_columns(num_cols);
    reader.set_num_columns(num_cols);

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

    // Compute natural widths and total row width for horizontal sliding
    let mut pane_widths = Vec::with_capacity(primary_pages.len());
    for &page in &primary_pages {
        let (pt_w, pt_h) = reader.page_size_pt(page);
        let ar = (pt_w / pt_h).max(0.1);
        let view = reader.page_view(page);
        let (base_w_px, _) = match view.fit {
            ReaderFitMode::FitWidth => (slot_w_px, slot_w_px / ar),
            ReaderFitMode::FitPage => {
                let h = viewport_h_px.min(slot_w_px / ar);
                (h * ar, h)
            }
        };
        pane_widths.push(base_w_px * view.zoom);
    }

    let total_panes_w: f32 = pane_widths.iter().sum();
    let total_gutters_w: f32 = primary_pages.len().saturating_sub(1) as f32 * gutter_px;
    let total_row_w_px = total_panes_w + total_gutters_w;
    let max_row_scroll_x = (total_row_w_px - viewport_w_px).max(0.0).round() as i64;
    reader.set_max_row_scroll_x(max_row_scroll_x);

    let row_scroll_x = reader.row_scroll_x();
    let mut cur_row_px = -(row_scroll_x as f32);

    for (idx, &page) in primary_pages.iter().enumerate() {
        let pane_w_px = pane_widths[idx];
        let page_start_px = cur_row_px;
        let page_end_px = page_start_px + pane_w_px;
        cur_row_px = page_end_px + gutter_px;

        if page_end_px <= 0.0 || page_start_px >= viewport_w_px {
            continue;
        }

        let vis_start_px = page_start_px.max(0.0);
        let vis_end_px = page_end_px.min(viewport_w_px);
        let col_x = viewport_area.x + (vis_start_px / cell_w_px).round() as u16;
        let cols = ((vis_end_px - vis_start_px) / cell_w_px).round() as u16;
        if cols == 0 {
            continue;
        }

        let avail_cols = viewport_area.right().saturating_sub(col_x).min(cols);
        if avail_cols == 0 {
            continue;
        }

        let crop_left_px = (vis_start_px - page_start_px).max(0.0);
        let crop_w_px = vis_end_px - vis_start_px;

        if let Some(m) = compute_pane_metrics_cropped(
            &ctx,
            page,
            PaneCellArea {
                col_x,
                cols: avail_cols,
                row_y: viewport_area.y,
                max_avail_rows: viewport_area.height,
            },
            reader.scroll_px,
            PaneCropSpan {
                crop_left_px,
                crop_w_px,
                pane_w_px,
            },
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
                is_primary: true,
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

                let mut cur_next_row_px = -(row_scroll_x as f32);
                for (idx, &np) in next_pages.iter().enumerate() {
                    let (pt_w, pt_h) = reader.page_size_pt(np);
                    let ar = (pt_w / pt_h).max(0.1);
                    let view = reader.page_view(np);
                    let (base_w_px, _) = match view.fit {
                        ReaderFitMode::FitWidth => (slot_w_px, slot_w_px / ar),
                        ReaderFitMode::FitPage => {
                            let h = viewport_h_px.min(slot_w_px / ar);
                            (h * ar, h)
                        }
                    };
                    let pane_w_px = base_w_px * view.zoom;

                    let page_start_px = cur_next_row_px;
                    let page_end_px = page_start_px + pane_w_px;
                    cur_next_row_px = page_end_px + gutter_px;

                    if page_end_px <= 0.0 || page_start_px >= viewport_w_px {
                        continue;
                    }

                    let vis_start_px = page_start_px.max(0.0);
                    let vis_end_px = page_end_px.min(viewport_w_px);
                    let col_x = viewport_area.x + (vis_start_px / cell_w_px).round() as u16;
                    let cols = ((vis_end_px - vis_start_px) / cell_w_px).round() as u16;
                    if cols == 0 {
                        continue;
                    }

                    let avail_cols = viewport_area.right().saturating_sub(col_x).min(cols);
                    if avail_cols == 0 {
                        continue;
                    }

                    let crop_left_px = (vis_start_px - page_start_px).max(0.0);
                    let crop_w_px = vis_end_px - vis_start_px;

                    if let Some(nm) = compute_pane_metrics_cropped(
                        &ctx,
                        np,
                        PaneCellArea {
                            col_x,
                            cols: avail_cols,
                            row_y: next_row_y,
                            max_avail_rows: avail_next_rows,
                        },
                        0,
                        PaneCropSpan {
                            crop_left_px,
                            crop_w_px,
                            pane_w_px,
                        },
                    ) {
                        let id = select_spread_image_id(np, idx, num_cols);
                        placements.push(ReaderSlotPlacement {
                            page: np,
                            id,
                            cell_rect: nm.cell_rect,
                            pixel_src: nm.pixel_src,
                            page_target: nm.page_target,
                            max_scroll_x: nm.max_scroll_x,
                            is_primary: false,
                        });
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
