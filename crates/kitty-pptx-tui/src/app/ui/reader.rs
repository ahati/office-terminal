use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub use crate::app::reader_scroll::VERTICAL_GUTTER_ROWS;
pub use crate::app::ui::reader_layout::{
    compute_horizontal_pane_metrics, compute_pane_metrics_cropped, horizontal_rest_inset_px,
    plan_row_columns, PaneCellArea, PaneCropSpan, PaneLayoutContext, GUTTER_COLS, MIN_PANE_COLS,
};
use crate::app::bindings::FrameBindings;
use crate::app::reader_state::ReaderState;
use crate::app::reader_types::{
    page_image_id, reader_viewport_metrics, ReaderLayout, ReaderSlotPlacement,
};
use crate::app::state::PresentationState;

/// Renders the current reader viewport (Single, Spread, or Horizontal mode).
///
/// The exact pane geometry this frame places is recorded into `bindings`
/// instead of being written back into state during the draw.
pub fn render_reader(
    state: &PresentationState,
    f: &mut Frame<'_>,
    area: Rect,
    bindings: &mut FrameBindings,
) {
    if area.width < MIN_PANE_COLS || area.height < 3 {
        return;
    }

    let Some(reader) = state.reader_state() else {
        return;
    };

    // One animation tick per frame: the ~16 ms event loop keeps calling the
    // renderer, so panes glide toward the scroll target instead of jumping.
    reader.advance_display();

    let viewport_height = area.height.saturating_sub(1);
    let viewport_area = Rect::new(area.x, area.y, area.width, viewport_height);
    let status_area = Rect::new(area.x, area.y + viewport_height, area.width, 1);

    let (viewport_w_px, viewport_h_px, cell_w_px, cell_h_px) =
        reader_viewport_metrics(area.width, area.height);

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
        // Same post-wrap lag compensation as the vertical branch, with the
        // wrapping page's width as the stride: while the smoothed slide
        // lags a just-wrapped anchor, the previous page is still on screen.
        let mut displayed = reader.displayed_scroll_px();
        let mut render_anchor = reader.anchor;
        while displayed < 0.0 && render_anchor > 0 {
            let prev = render_anchor - 1;
            let (pt_w, pt_h) = reader.page_size_pt(prev);
            let ar = (pt_w / pt_h).max(0.1);
            let page_w = viewport_h_px * reader.page_view(prev).zoom * ar;
            if page_w <= 0.0 {
                displayed = 0.0;
                break;
            }
            render_anchor = prev;
            displayed += page_w;
        }
        if displayed < 0.0 {
            displayed = 0.0;
        }
        // Document-rest centering: the stream's first (and, past the last
        // flip, final) page rests centered when narrower than the viewport.
        // The inset unwinds linearly with the slide, so the frame where the
        // anchor flips is geometrically identical to the flush-left stream.
        let rest_inset_px = horizontal_rest_inset_px(
            reader,
            render_anchor,
            displayed,
            viewport_w_px,
            viewport_h_px,
        );
        let mut col_x = viewport_area.x;
        let mut p = render_anchor;
        // The smoothed value drives placement so horizontal slides glide. A
        // negative anchor scroll is the active rest inset: the pane shifts
        // right instead of its left edge being cropped.
        let anchor_scroll_x = (displayed - rest_inset_px) as i64;

        while p < reader.total_pages() && col_x < viewport_area.right() {
            let is_anchor_pane = p == render_anchor;
            let scroll_x = if is_anchor_pane { anchor_scroll_x } else { 0 };
            let pane_col_x = col_x.saturating_add(if is_anchor_pane {
                ((-(scroll_x)).max(0) as f32 / cell_w_px).round() as u16
            } else {
                0
            });
            let avail_cols = viewport_area.right().saturating_sub(pane_col_x);
            if avail_cols < MIN_PANE_COLS && p > reader.anchor {
                break;
            }
            if let Some(m) = compute_horizontal_pane_metrics(
                &ctx,
                p,
                pane_col_x,
                avail_cols,
                viewport_area.y,
                viewport_area.height,
                scroll_x,
            ) {
                // ImageId tied to page index so horizontal sliding produces 0 re-stages (P0-4)
                let id = page_image_id(p);
                placements.push(ReaderSlotPlacement {
                    page: p,
                    id,
                    cell_rect: m.cell_rect,
                    pixel_src: m.pixel_src,
                    page_target: m.page_target,
                    max_scroll_x: m.max_scroll_x,
                    is_primary: true,
                });

                col_x = pane_col_x
                    .saturating_add(m.cell_rect.cols)
                    .saturating_add(gutter_cols);
                p += 1;
            } else {
                break;
            }
        }

        let placed_cols = placements.len().max(1);
        bindings.reader_columns = placed_cols;
        bindings.reader = placements;

        let status_text = reader.status_line(
            state.presentation_title().unwrap_or("document.pdf"),
            &bindings.reader,
            bindings.reader_columns,
        );
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
    // Column count from the document's DOMINANT page at the current zoom;
    // the lag-compensated render anchor below only moves pane geometry, not
    // the column plan. Recomputed here every frame, so zoom steps reflow
    // the row (PRD 14a §6.3), and the majority basis keeps the grid stable
    // across mixed-orientation documents (§6.4): a landscape page pairs with
    // its neighbor instead of claiming a row because it happens to lead.
    let planned_cols = plan_row_columns(
        viewport_area.width,
        viewport_h_px,
        cell_w_px,
        reader.dominant_page_pt(),
        reader.default_zoom,
    );

    let num_cols = match reader.layout {
        ReaderLayout::Single => 1,
        ReaderLayout::Spread => {
            if reader.total_pages() <= 1 {
                1
            } else {
                planned_cols as usize
            }
        }
        ReaderLayout::Horizontal => 1,
    };

    // Record the computed column count for this frame (P0-2).
    bindings.reader_columns = num_cols;

    // Fit slot by layout and fit mode for THIS frame's column plan — the
    // stored count trails by one publish, and the render must never mix
    // the two (PRD 14a §6.1).
    let slot_w_px = reader.slot_width_for(viewport_w_px, viewport_h_px, cell_w_px, num_cols);

    // Post-wrap lag compensation. The anchor wraps when the scroll TARGET
    // crosses the row stride, and the wrap shifts the smoothed display by
    // the same stride; under continuous scrolling the display still lags the
    // target by the remaining glide distance, so right after the flip it is
    // NEGATIVE under the new anchor. The layout cannot render a negative
    // offset — it would clamp the new row to the viewport top and the whole
    // page would jump up by the lag (the boundary flicker). Render the
    // previous row instead, with the lag added back: (anchor - step,
    // displayed + stride) is geometrically identical to the pre-wrap frame,
    // so the flip is invisible and the new row slides exactly as far as the
    // display has actually scrolled.
    let row_stride_px = reader.row_stride_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px);
    let mut displayed = reader.displayed_scroll_px();
    let mut render_anchor = reader.anchor;
    while displayed < 0.0 {
        let prev = prev_row_anchor(reader, render_anchor);
        if prev == render_anchor {
            displayed = 0.0;
            break;
        }
        render_anchor = prev;
        displayed += row_stride_px;
    }

    let ctx = PaneLayoutContext {
        state,
        fit_slot_w_px: slot_w_px,
        cell_w_px,
        cell_h_px,
        viewport_h_px,
    };

    // ─── 1. Primary (Anchor) Row ─────────────────────────────────────────────────
    // `render_anchor` (not `reader.anchor`): while the smoothed scroll lags
    // a just-wrapped anchor, the previous row is still what is on screen.
    let primary_pages: Vec<usize> = if reader.cover_alone && render_anchor == 0 {
        vec![0]
    } else {
        (0..num_cols)
            .map(|i| render_anchor + i)
            .filter(|&p| p < reader.total_pages())
            .collect()
    };

    // Compute natural widths and total row width for horizontal sliding
    let mut pane_widths = Vec::with_capacity(primary_pages.len());
    for &page in &primary_pages {
        let (base_w_px, _) = reader.pane_fit(page, viewport_h_px, slot_w_px);
        pane_widths.push(base_w_px * reader.page_view(page).zoom);
    }

    let total_panes_w: f32 = pane_widths.iter().sum();
    let total_gutters_w: f32 = primary_pages.len().saturating_sub(1) as f32 * gutter_px;
    let total_row_w_px = total_panes_w + total_gutters_w;
    let is_zoomed = primary_pages.iter().any(|&p| reader.page_view(p).zoom > 1.0);
    let overflow_row_w = total_row_w_px - viewport_w_px;
    let max_row_scroll_x = if is_zoomed && overflow_row_w >= cell_w_px {
        overflow_row_w.round() as i64
    } else {
        0
    };
    reader.set_max_row_scroll_x(max_row_scroll_x);

    let primary_row = RowRender {
        reader,
        ctx: &ctx,
        viewport_area,
        viewport_w_px,
        slot_w_px,
        cell_w_px,
        gutter_px,
        pages: &primary_pages,
        row_y: viewport_area.y,
        avail_rows: viewport_area.height,
        // The smoothed offset drives placement so vertical scrolls glide.
        scroll_px: displayed as i64,
        row_scroll_x: reader.row_scroll_x(),
        is_primary: true,
    };
    let row_occupied_cells_h = render_row(&primary_row, &mut placements);

    // ─── 2. Next Row (Continuous Scroll Transition with Vertical Gutter) ──────────
    let next_anchor = if reader.cover_alone && render_anchor == 0 {
        1
    } else {
        render_anchor + num_cols
    };

    if next_anchor < reader.total_pages() {
        let (next_row_y, avail_next_rows, next_row_scroll_px) = if row_occupied_cells_h > 0 {
            let next_y = viewport_area.y + row_occupied_cells_h + VERTICAL_GUTTER_ROWS;
            let avail = (viewport_area.y + viewport_area.height).saturating_sub(next_y);
            (next_y, avail, 0i64)
        } else {
            let row_0_stride_px =
                reader.row_stride_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px);
            let y1_px = row_0_stride_px - displayed;
            let offset_rows = (y1_px / cell_h_px).round() as i16;
            if offset_rows > 0 {
                let next_y = viewport_area.y + offset_rows as u16;
                let avail = (viewport_area.y + viewport_area.height).saturating_sub(next_y);
                (next_y, avail, 0i64)
            } else {
                let scroll = (-y1_px).max(0.0).round() as i64;
                (viewport_area.y, viewport_area.height, scroll)
            }
        };

        if avail_next_rows > 0 {
            let next_pages: Vec<usize> = (0..num_cols)
                .map(|i| next_anchor + i)
                .filter(|&p| p < reader.total_pages())
                .collect();

            let next_row = RowRender {
                reader,
                ctx: &ctx,
                viewport_area,
                viewport_w_px,
                slot_w_px,
                cell_w_px,
                gutter_px,
                pages: &next_pages,
                row_y: next_row_y,
                avail_rows: avail_next_rows,
                scroll_px: next_row_scroll_px,
                row_scroll_x: reader.row_scroll_x(),
                is_primary: false,
            };
            render_row(&next_row, &mut placements);
        }
    }

    bindings.reader = placements;

    // ─── Status Line ─────────────────────────────────────────────────────────────
    let status_text = reader.status_line(
        state.presentation_title().unwrap_or("document.pdf"),
        &bindings.reader,
        bindings.reader_columns,
    );
    let status_style = Style::default().fg(Color::White).bg(Color::DarkGray);
    let status_paragraph = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(status_text, status_style),
    ]))
    .style(status_style);
    f.render_widget(status_paragraph, status_area);
}

/// Inputs for rendering one row of reader panes (shared by the primary and
/// next-row passes so the two blocks cannot drift apart).
struct RowRender<'a> {
    reader: &'a ReaderState,
    ctx: &'a PaneLayoutContext<'a>,
    viewport_area: Rect,
    viewport_w_px: f32,
    slot_w_px: f32,
    cell_w_px: f32,
    gutter_px: f32,
    pages: &'a [usize],
    row_y: u16,
    avail_rows: u16,
    scroll_px: i64,
    row_scroll_x: i64,
    is_primary: bool,
}

/// Places one row of panes into `placements`. Returns the tallest cell
/// height the row occupies.
///
/// **Stable column grid (PRD 14a §6.4).** A row whose panes all fit their
/// column boxes (`slot_w_px`) is placed on the document grid: pane *i*
/// centers in box *i*, boxes are spaced by the gutter and the grid centers
/// in the viewport. Every row of a document — and the row peeking in from
/// below during a continuous scroll — therefore sits at identical x
/// positions, so pages glide straight up and nothing shuffles sideways at
/// an anchor flip. Uniform rows reduce to the centered row of §6.2.
///
/// Rows with a pane that overflows its box (fit-width zoomed past the
/// slot, per-page zoom with `sync` off) keep the flush-left content walk
/// and its horizontal pan window (`row_scroll_x`).
fn render_row(row: &RowRender<'_>, placements: &mut Vec<ReaderSlotPlacement>) -> u16 {
    let pane_widths: Vec<f32> = row
        .pages
        .iter()
        .map(|&page| {
            let (base_w_px, _) = row
                .reader
                .pane_fit(page, row.ctx.viewport_h_px, row.slot_w_px);
            base_w_px * row.reader.page_view(page).zoom
        })
        .collect();

    let on_grid = pane_widths.iter().all(|&w| w <= row.slot_w_px + 0.5);
    // Page start offsets per slot (row scroll applied later, in the tail).
    let starts: Vec<f32> = if on_grid {
        let boxes = row.pages.len() as f32;
        let grid_w = boxes * row.slot_w_px + (boxes - 1.0).max(0.0) * row.gutter_px;
        let grid_left = ((row.viewport_w_px - grid_w) / 2.0).max(0.0);
        (0..row.pages.len())
            .map(|slot| {
                let box_start = grid_left + slot as f32 * (row.slot_w_px + row.gutter_px);
                let pane_w = pane_widths[slot];
                box_start + ((row.slot_w_px - pane_w) / 2.0).max(0.0)
            })
            .collect()
    } else {
        // Flush-left content walk. A walk narrower than the viewport still
        // centers (PRD 14a §6.2); an overflowing one keeps the pan origin.
        let total_row_w: f32 = pane_widths.iter().sum::<f32>()
            + (row.pages.len().saturating_sub(1)) as f32 * row.gutter_px;
        let row_start = ((row.viewport_w_px - total_row_w) / 2.0).max(0.0);
        let mut starts = Vec::with_capacity(row.pages.len());
        let mut cursor = row_start;
        for (slot, &pane_w) in pane_widths.iter().enumerate() {
            starts.push(cursor);
            cursor += pane_w;
            if slot + 1 < row.pages.len() {
                cursor += row.gutter_px;
            }
        }
        starts
    };

    let mut occupied_rows = 0u16;

    for (slot, &page) in row.pages.iter().enumerate() {
        let pane_w_px = pane_widths[slot];

        let page_start_px = starts[slot] - row.row_scroll_x as f32;
        let page_end_px = page_start_px + pane_w_px;

        if page_end_px <= 0.0 || page_start_px >= row.viewport_w_px {
            continue;
        }

        let vis_start_px = page_start_px.max(0.0);
        let vis_end_px = page_end_px.min(row.viewport_w_px);
        let col_x = row.viewport_area.x + (vis_start_px / row.cell_w_px).round() as u16;
        let cols = ((vis_end_px - vis_start_px) / row.cell_w_px).round() as u16;
        if cols == 0 {
            continue;
        }

        let avail_cols = row.viewport_area.right().saturating_sub(col_x).min(cols);
        if avail_cols == 0 {
            continue;
        }

        let crop_left_px = (vis_start_px - page_start_px).max(0.0);
        let crop_w_px = vis_end_px - vis_start_px;

        let Some(m) = compute_pane_metrics_cropped(
            row.ctx,
            page,
            PaneCellArea {
                col_x,
                cols: avail_cols,
                row_y: row.row_y,
                max_avail_rows: row.avail_rows,
            },
            row.scroll_px,
            PaneCropSpan {
                crop_left_px,
                crop_w_px,
                pane_w_px,
            },
        ) else {
            continue;
        };

        occupied_rows = occupied_rows.max(m.cell_rect.rows);

        // Stable per-page ids: a pane that moves between rows or columns
        // keeps its id, so the graphics syncer re-places it instead of
        // re-encoding and re-transmitting the raster.
        placements.push(ReaderSlotPlacement {
            page,
            id: page_image_id(page),
            cell_rect: m.cell_rect,
            pixel_src: m.pixel_src,
            page_target: m.page_target,
            max_scroll_x: m.max_scroll_x,
            is_primary: row.is_primary,
        });

    }

    occupied_rows
}

/// Anchor of the row above `anchor` under the current layout — the mirror of
/// the retreat step in `scroll_column_detailed`, so lag-compensated rendering
/// backs up exactly the way scrolling down advanced.
fn prev_row_anchor(reader: &ReaderState, anchor: usize) -> usize {
    match reader.layout {
        ReaderLayout::Single | ReaderLayout::Horizontal => anchor.saturating_sub(1),
        ReaderLayout::Spread => {
            let step = reader.effective_columns();
            if reader.cover_alone && anchor <= step {
                0
            } else {
                anchor.saturating_sub(step)
            }
        }
    }
}
