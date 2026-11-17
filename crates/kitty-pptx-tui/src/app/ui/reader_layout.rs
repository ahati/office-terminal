//! Spatial layout and pane geometry calculations for Reader mode.

use kitty_pptx_core::RenderTargetSpec;
use kitty_pptx_display::{CellRect, ImageId, PixelRect};

use crate::app::reader_types::ReaderFitMode;
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

    let target_w = 2560u32;
    let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
    let page_target = RenderTargetSpec {
        width_px: target_w,
        height_px: target_h,
    };

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
    let (pt_w, pt_h) = reader.page_size_pt(page);
    let ar = (pt_w / pt_h).max(0.1);

    let (_base_w_px, base_h_px) = match view.fit {
        ReaderFitMode::FitWidth => (ctx.fit_slot_w_px, ctx.fit_slot_w_px / ar),
        ReaderFitMode::FitPage => {
            let h = ctx.viewport_h_px.min(ctx.fit_slot_w_px / ar);
            let w = h * ar;
            (w, h)
        }
    };
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

    let target_w = 2560u32;
    let target_h = ((target_w as f32 / ar).round() as u32).clamp(1024, 8192);
    let page_target = RenderTargetSpec {
        width_px: target_w,
        height_px: target_h,
    };

    let scale = target_w as f32 / crop.pane_w_px.max(1.0);
    let screen_w_px = (cell_rect.cols as f32) * ctx.cell_w_px;
    let max_scroll_x = (crop.pane_w_px - screen_w_px).max(0.0) as u32;

    let src_x = (((crop.crop_left_px + view.scroll_x as f32) * scale).round() as u32).min(target_w);
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

pub fn select_row_image_id(is_even_row: bool, slot_idx: usize, num_cols: usize) -> ImageId {
    let base_id = if is_even_row {
        slot_idx as u32 + 1
    } else {
        slot_idx as u32 + 1 + (num_cols as u32).max(1)
    };
    ImageId(base_id.clamp(1, 12))
}

pub fn select_spread_image_id(page: usize, slot_idx: usize, num_cols: usize) -> ImageId {
    let row_parity = (page / num_cols.max(1)).is_multiple_of(2);
    select_row_image_id(row_parity, slot_idx, num_cols)
}
