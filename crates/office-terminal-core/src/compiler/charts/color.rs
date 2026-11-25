use crate::compiler::style::parse_hex_color;
use ooxml_common::chart::ChartSeries;

pub const DEFAULT_THEME_PALETTE: [&str; 8] = [
    "4472C4", // Accent 1: Blue
    "ED7D31", // Accent 2: Orange
    "A5A5A5", // Accent 3: Medium Gray
    "FFC000", // Accent 4: Gold / Yellow
    "5B9BD5", // Accent 5: Light Blue
    "70AD47", // Accent 6: Green
    "264478", // Dark Blue
    "9E480E", // Dark Orange
];

pub fn resolve_series_color(series_idx: usize, series: &ChartSeries) -> [u8; 4] {
    if let Some(ref hex) = series.color {
        return parse_hex_color(hex);
    }
    let fallback_hex = DEFAULT_THEME_PALETTE[series_idx % DEFAULT_THEME_PALETTE.len()];
    parse_hex_color(fallback_hex)
}

pub fn resolve_series_line_color(series_idx: usize, series: &ChartSeries) -> [u8; 4] {
    if let Some(ref hex) = series.line_color {
        return parse_hex_color(hex);
    }
    resolve_series_color(series_idx, series)
}

pub fn resolve_slice_color(slice_idx: usize, series: Option<&ChartSeries>) -> [u8; 4] {
    if let Some(s) = series {
        if let Some(ref point_colors) = s.data_point_colors {
            if let Some(Some(hex)) = point_colors.get(slice_idx) {
                return parse_hex_color(hex);
            }
        }
    }
    let hex = DEFAULT_THEME_PALETTE[slice_idx % DEFAULT_THEME_PALETTE.len()];
    parse_hex_color(hex)
}

pub fn with_alpha(mut color: [u8; 4], alpha: f32) -> [u8; 4] {
    color[3] = (color[3] as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
    color
}
