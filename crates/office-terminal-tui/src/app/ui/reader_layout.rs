//! Spatial layout and pane geometry calculations for Reader mode.

use office_terminal_core::RenderTargetSpec;
use office_terminal_display::{CellRect, PixelRect};

use crate::app::reader_state::ReaderState;
use crate::app::state::PresentationState;

pub const MIN_PANE_COLS: u16 = 10;
pub const GUTTER_COLS: u16 = 2;

/// Computed spatial metrics and image rects for a single reader pane.
#[derive(Clone, Debug)]
pub struct PaneMetrics {
    pub cell_rect: CellRect,
    pub pixel_src: PixelRect,
    pub page_target: RenderTargetSpec,
    pub max_scroll_x: u32,
}

/// Context for computing layout of individual reader panes.
pub struct PaneLayoutContext<'a> {
    pub state: &'a PresentationState,
    pub fit_slot_w_px: f32,
    pub cell_w_px: f32,
    pub cell_h_px: f32,
    pub viewport_h_px: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PaneCellArea {
    pub col_x: u16,
    pub cols: u16,
    pub row_y: u16,
    pub max_avail_rows: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct PaneCropSpan {
    pub crop_left_px: f32,
    pub crop_w_px: f32,
    pub pane_w_px: f32,
}

/// Column plan for the vertical reflow: how many dominant-page panes fit
/// across the viewport at the document's current zoom (PRD 14a §6.1/§6.3).
///
/// Recomputed every frame from the *zoomed* dominant width, so every zoom
/// step re-evaluates the decision — zooming in reflows to fewer columns,
/// zooming out packs more pages per row. The dominant (majority) basis
/// keeps the plan stable across mixed-orientation documents (§6.4): a
/// landscape page never re-plans its row just because it happens to lead.
/// Clamped to `1..=6` columns; `Single` and `Horizontal` layouts ignore it
/// (they always show one pane per row lead).
pub fn plan_row_columns(viewport_cols: u16, viewport_h_px: f32, cell_w_px: f32, page_pt: (f32, f32), zoom: f32) -> u16 {
    let (pt_w, pt_h) = page_pt;
    let ar = (pt_w / pt_h).max(0.1);
    let page_w_px = viewport_h_px * ar * zoom;
    let ideal_cols = ((page_w_px / cell_w_px).round() as u16).max(MIN_PANE_COLS);
    let cols_per_pane = ideal_cols.saturating_add(GUTTER_COLS);
    (viewport_cols.saturating_add(GUTTER_COLS) / cols_per_pane).clamp(1, 6)
}

/// Lead-in/lead-out inset (px) that centers the horizontal stream's resting
/// pages when they are narrower than the viewport.
///
/// Horizontal mode tiles pages edge to edge, so mid-stream anchors never
/// need an inset — the stream already fills the viewport. Only the two
/// document-rest anchors do:
///
/// - **Start** (`render_anchor == 0`): the first page rests centered and the
///   inset unwinds *linearly to zero across the page's own width*, so the
///   frame in which the anchor flips to page 1 is bit-identical to the
///   flush-left stream (no boundary jump).
/// - **End** (`render_anchor == last`): the mirror image — the inset winds up
///   from zero across the final page's scroll range, leaving the page
///   centered exactly at the end clamp (`scroll_px == page_w - 1`).
///
/// `displayed_px` is the smoothed scroll offset relative to `render_anchor`
/// (already lag-compensated and clamped `>= 0` by the caller). A negative
/// result never occurs; anchors whose page fills the viewport get `0.0`.
pub fn horizontal_rest_inset_px(
    reader: &ReaderState,
    render_anchor: usize,
    displayed_px: f32,
    viewport_w_px: f32,
    viewport_h_px: f32,
) -> f32 {
    let total = reader.total_pages();
    if total == 0 {
        return 0.0;
    }
    let zoom = reader.page_view(render_anchor).zoom;
    let (pt_w, pt_h) = reader.page_size_pt(render_anchor);
    let ar = (pt_w / pt_h).max(0.1);
    let pane_w_px = (viewport_h_px * zoom * ar).max(1.0);
    let center_px = ((viewport_w_px - pane_w_px) / 2.0).max(0.0);
    if center_px <= 0.0 {
        return 0.0;
    }

    if render_anchor == 0 {
        // Single-page documents use start semantics: the end clamp is just
        // the last stretch of the same (only) page.
        let unwind = (displayed_px / pane_w_px).clamp(0.0, 1.0);
        center_px * (1.0 - unwind)
    } else if render_anchor == total - 1 {
        // Winds from 0 at the flip-in to `center + pane_w` at the clamp, so
        // the pane's left edge lands at `center_px` when fully scrolled.
        let wind = (displayed_px / pane_w_px).clamp(0.0, 1.0);
        (center_px + pane_w_px) * wind
    } else {
        0.0
    }
}

pub fn compute_horizontal_pane_metrics(
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

    let page_target = reader.page_render_target(page, pane_w_px);
    let target_w = page_target.width_px;
    let target_h = page_target.height_px;

    let scale = target_w as f32 / pane_w_px.max(1.0);
    let src_x = ((scroll_x_px.max(0) as f32 * scale).round() as u32).min(target_w);
    let screen_w_px = (cell_rect.cols as f32) * ctx.cell_w_px;
    let src_w = ((screen_w_px * scale).round() as u32).min(target_w.saturating_sub(src_x));

    if cell_rect.cols == 0 || cell_rect.rows == 0 || src_w == 0 || target_h == 0 {
        return None;
    }

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

pub fn compute_pane_metrics_cropped(
    ctx: &PaneLayoutContext<'_>,
    page: usize,
    area: PaneCellArea,
    scroll_px: i64,
    crop: PaneCropSpan,
) -> Option<PaneMetrics> {
    let reader = ctx.state.reader_state()?;
    let view = reader.page_view(page);

    let (_base_w_px, base_h_px) = reader.pane_fit(page, ctx.viewport_h_px, ctx.fit_slot_w_px);
    let pane_h_px = base_h_px * view.zoom;

    let visible_h_px = (pane_h_px - scroll_px as f32).max(0.0);
    let full_rows = (visible_h_px / ctx.cell_h_px).round() as u16;
    let rows = full_rows.clamp(1, area.max_avail_rows);

    let cell_rect = CellRect {
        row: area.row_y,
        col: area.col_x,
        rows,
        cols: area.cols,
    };

    let page_target = reader.page_render_target(page, crop.pane_w_px);
    let target_w = page_target.width_px;
    let target_h = page_target.height_px;

    let scale = target_w as f32 / crop.pane_w_px.max(1.0);
    let screen_w_px = (cell_rect.cols as f32) * ctx.cell_w_px;
    let overflow_x = crop.pane_w_px - screen_w_px;
    let max_scroll_x = if view.zoom > 1.0 && overflow_x >= ctx.cell_w_px {
        overflow_x.round() as u32
    } else {
        0
    };

    let scroll_x = if max_scroll_x > 0 {
        (view.scroll_x as f32).min(max_scroll_x as f32)
    } else {
        0.0
    };
    let src_x = (((crop.crop_left_px + scroll_x) * scale).round() as u32).min(target_w);
    let src_w = ((crop.crop_w_px * scale).round() as u32).min(target_w.saturating_sub(src_x));

    let total_scroll_y = (scroll_px as f32 + view.scroll_y as f32).max(0.0);
    let src_y = ((total_scroll_y * scale).round() as u32).min(target_h);
    let screen_h_px = (cell_rect.rows as f32) * ctx.cell_h_px;
    let src_h = ((screen_h_px * scale).round() as u32).min(target_h.saturating_sub(src_y));

    if cell_rect.cols == 0 || cell_rect.rows == 0 || src_w == 0 || src_h == 0 {
        return None;
    }

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
