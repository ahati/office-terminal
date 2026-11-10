use crate::driver::CellRect;

/// Default terminal character cell aspect ratio (width / height).
/// Monospace fonts in terminal emulators are conventionally roughly twice as tall as they are wide (1:2 ratio, 0.5).
pub const DEFAULT_CELL_ASPECT_RATIO: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    /// Scale image to fit inside bounds while preserving aspect ratio.
    Contain,
    /// Scale image to fill the available width, adjusting height accordingly.
    FillWidth,
}

/// Fits image dimensions (cols, rows) within (max_cols, max_rows) preserving image aspect ratio.
pub fn fit_aspect_ratio(
    img_w: u32,
    img_h: u32,
    max_cols: u16,
    max_rows: u16,
    cell_aspect_ratio: f64,
) -> (u16, u16) {
    if img_w == 0 || img_h == 0 || max_cols == 0 || max_rows == 0 {
        return (max_cols.max(1), max_rows.max(1));
    }

    let aspect_ratio = cell_aspect_ratio.max(0.01);
    let img_ar = img_w as f64 / img_h as f64;

    // Character width when constrained by max_rows:
    let cols_for_max_rows = ((max_rows as f64) * (img_ar / aspect_ratio)).round() as u16;

    if cols_for_max_rows <= max_cols && cols_for_max_rows > 0 {
        (cols_for_max_rows, max_rows)
    } else {
        // Character height when constrained by max_cols:
        let rows_for_max_cols = ((max_cols as f64) * (aspect_ratio / img_ar)).round() as u16;
        (max_cols, rows_for_max_cols.clamp(1, max_rows))
    }
}

/// Calculates a centered CellRect within a container area while preserving image aspect ratio.
pub fn calculate_centered_placement(
    img_w: u32,
    img_h: u32,
    container: CellRect,
    cell_aspect_ratio: f64,
) -> CellRect {
    fit_cell_aspect(img_w, img_h, container, cell_aspect_ratio, FitMode::Contain)
}

/// Fits an image into a bounding CellRect according to the specified FitMode.
pub fn fit_cell_aspect(
    image_width: u32,
    image_height: u32,
    bounds: CellRect,
    cell_aspect_ratio: f64,
    mode: FitMode,
) -> CellRect {
    if image_width == 0 || image_height == 0 || bounds.cols == 0 || bounds.rows == 0 {
        return bounds;
    }

    let aspect_ratio = cell_aspect_ratio.max(0.01);
    let img_ar = image_width as f64 / image_height as f64;

    let (cols, rows) = match mode {
        FitMode::Contain => {
            fit_aspect_ratio(image_width, image_height, bounds.cols, bounds.rows, cell_aspect_ratio)
        }
        FitMode::FillWidth => {
            let cols = bounds.cols;
            let rows = ((cols as f64) * (aspect_ratio / img_ar)).round() as u16;
            (cols, rows.max(1))
        }
    };

    let col = bounds.col + bounds.cols.saturating_sub(cols) / 2;
    let row = bounds.row + bounds.rows.saturating_sub(rows) / 2;

    CellRect { row, col, rows, cols }
}
