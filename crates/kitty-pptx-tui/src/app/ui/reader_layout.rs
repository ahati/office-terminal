//! Spatial layout and pane geometry calculations for Reader mode.

use kitty_pptx_core::RenderTargetSpec;
use kitty_pptx_display::{CellRect, PixelRect};

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
