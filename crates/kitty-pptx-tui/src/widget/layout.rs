use ratatui::layout::Rect;
use kitty_pptx_display::CellRect;

/// Standard aspect ratio constants
pub const ASPECT_16_9: f32 = 16.0 / 9.0;
pub const ASPECT_4_3: f32 = 4.0 / 3.0;

/// Default terminal cell aspect ratio (width / height).
/// In most monospace terminal fonts, cells are roughly twice as tall as they are wide (~1:2),
/// meaning width/height is ~0.5.
pub const DEFAULT_CELL_ASPECT_RATIO: f32 = 0.5;

/// Computes the slide layout `Rect` maintaining the slide's physical aspect ratio
/// inside the available terminal `area`, with centered letterboxing (vertical bars)
/// or pillarboxing (horizontal bars).
///
/// * `area` - Available terminal cell area
/// * `aspect_ratio` - Slide aspect ratio (width / height, e.g. 16/9 or 4/3)
/// * `cell_aspect_ratio` - Terminal font cell aspect ratio (cell_width / cell_height)
pub fn compute_slide_rect(area: Rect, aspect_ratio: f32, cell_aspect_ratio: f32) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }

    let aspect = if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio
    } else {
        ASPECT_16_9
    };

    let cell_aspect = if cell_aspect_ratio.is_finite() && cell_aspect_ratio > 0.0 {
        cell_aspect_ratio
    } else {
        DEFAULT_CELL_ASPECT_RATIO
    };

    // In terminal cell coordinates, the effective aspect ratio (cols / rows) is:
    // (cols * cell_w) / (rows * cell_h) = aspect => cols / rows = aspect / (cell_w / cell_h)
    let cell_ratio = aspect / cell_aspect;

    // Try fitting to width first
    let fit_w_by_width = area.width;
    let fit_h_by_width = ((fit_w_by_width as f32) / cell_ratio).round() as u16;

    let (fitted_w, fitted_h) = if fit_h_by_width <= area.height && fit_h_by_width > 0 {
        (fit_w_by_width, fit_h_by_width)
    } else {
        // Fit to height instead (pillarbox)
        let fit_h = area.height;
        let fit_w = ((fit_h as f32) * cell_ratio).round() as u16;
        (fit_w.min(area.width).max(1), fit_h)
    };

    let offset_x = area.x + area.width.saturating_sub(fitted_w) / 2;
    let offset_y = area.y + area.height.saturating_sub(fitted_h) / 2;

    Rect {
        x: offset_x,
        y: offset_y,
        width: fitted_w,
        height: fitted_h,
    }
}

/// Convert a Ratatui `Rect` to `kitty_pptx_display::CellRect`.
pub fn rect_to_cell_rect(rect: Rect) -> CellRect {
    CellRect {
        row: rect.y,
        col: rect.x,
        rows: rect.height,
        cols: rect.width,
    }
}

/// Convert a `kitty_pptx_display::CellRect` to Ratatui `Rect`.
pub fn cell_rect_to_rect(cell: CellRect) -> Rect {
    Rect {
        x: cell.col,
        y: cell.row,
        width: cell.cols,
        height: cell.rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_area() {
        let area = Rect::new(0, 0, 0, 0);
        let res = compute_slide_rect(area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        assert_eq!(res.width, 0);
        assert_eq!(res.height, 0);

        let area_zero_h = Rect::new(10, 10, 80, 0);
        let res_zh = compute_slide_rect(area_zero_h, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        assert_eq!(res_zh.height, 0);
    }

    #[test]
    fn test_16_9_fitting_and_centering() {
        // 80 cols by 40 rows terminal.
        // cell_ratio = (16/9) / 0.5 = 32/9 ≈ 3.5556
        // If fit by width: 80 / 3.5556 ≈ 22.5 -> 23 rows (<= 40 rows)
        // Fitted width: 80, height: 23 (letterboxed)
        // offset_y = (40 - 23) / 2 = 8
        let area = Rect::new(0, 0, 80, 40);
        let res = compute_slide_rect(area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        assert_eq!(res.width, 80);
        assert_eq!(res.height, 23);
        assert_eq!(res.x, 0);
        assert_eq!(res.y, 8);
    }

    #[test]
    fn test_pillarbox_fitting() {
        // Very tall narrow terminal: 40 cols by 80 rows
        // If fit by width: 40 / 3.5556 ≈ 11 rows
        // Letterboxed vertically with offset_y = (80 - 11) / 2 = 34
        let area = Rect::new(5, 5, 40, 80);
        let res = compute_slide_rect(area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        assert_eq!(res.width, 40);
        assert_eq!(res.height, 11);
        assert_eq!(res.x, 5);
        assert_eq!(res.y, 5 + (80 - 11) / 2);

        // Very wide short terminal: 120 cols by 10 rows
        // Fit by width would require 120 / 3.5556 ≈ 34 rows > 10
        // So fits by height: height = 10, width = 10 * 3.5556 ≈ 36 cols <= 120
        // Pillarboxed horizontally: offset_x = (120 - 36) / 2 = 42
        let wide_area = Rect::new(0, 0, 120, 10);
        let wide_res = compute_slide_rect(wide_area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        assert_eq!(wide_res.height, 10);
        assert_eq!(wide_res.width, 36);
        assert_eq!(wide_res.x, 42);
        assert_eq!(wide_res.y, 0);
    }

    #[test]
    fn test_rect_conversions() {
        let r = Rect::new(10, 5, 50, 20);
        let c = rect_to_cell_rect(r);
        assert_eq!(c.col, 10);
        assert_eq!(c.row, 5);
        assert_eq!(c.cols, 50);
        assert_eq!(c.rows, 20);

        let r2 = cell_rect_to_rect(c);
        assert_eq!(r, r2);
    }
}
