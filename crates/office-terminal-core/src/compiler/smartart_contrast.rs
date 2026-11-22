//! Contrast-aware default text colour for the synthetic SmartArt fallback.
//!
//! When a deck has no prebaked SmartArt drawing part, the parser synthesizes a
//! list shape from the diagram data model. Its runs carry no colour, so they
//! fall to the theme default (usually dark) and disappear on a dark slide.
//! This module derives a default only for that synthetic shape: light
//! background keeps the theme default, dark background gets white. Explicit
//! run colours and style-derived shape colours are never touched.
//!
//! ref `packages/pptx/src/smartart-fallback-contrast.ts:1-112`.

use pptx_model::Fill;

/// Rec. 601 luma (0–1) of a 6- or 8-digit hex colour, compositing
/// `RRGGBBAA` over white first. Returns `None` for malformed input.
pub fn hex_luma(hex: &str) -> Option<f32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32;
    let base = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
    if hex.len() == 6 {
        return Some(base);
    }
    let alpha = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
    Some(alpha * base + (1.0 - alpha))
}

/// Mean luminance of a slide background fill, or `None` when it cannot be
/// derived statically (image/pattern/none/absent).
pub fn background_luminance(fill: Option<&Fill>) -> Option<f32> {
    match fill? {
        Fill::Solid { color } => hex_luma(color),
        Fill::Gradient { stops, .. } => {
            let mut stops: Vec<(f32, f32)> = stops
                .iter()
                .filter_map(|stop| {
                    hex_luma(&stop.color).map(|luma| (stop.position.clamp(0.0, 1.0) as f32, luma))
                })
                .collect();
            if stops.is_empty() {
                return None;
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            let first = stops[0];
            let last = *stops.last()?;
            // Piecewise-linear integral of the stop luma over [0, 1]; end
            // stops extend to the edges exactly as canvas gradients clamp.
            let mut area = first.1 * first.0 + last.1 * (1.0 - last.0);
            for pair in stops.windows(2) {
                let (p0, l0) = pair[0];
                let (p1, l1) = pair[1];
                area += (l0 + l1) * 0.5 * (p1 - p0);
            }
            Some(area)
        }
        Fill::None | Fill::Pattern { .. } | Fill::Image { .. } => None,
    }
}

/// True only for the parser-synthesized fallback: the name is `SmartArt` and
/// no `<p:cNvPr @id>` exists (real shapes always carry the schema-required id).
pub fn is_smart_art_fallback_shape(id: Option<&str>, name: Option<&str>) -> bool {
    name == Some("SmartArt") && id.is_none()
}

/// White when the background is dark and the theme default is also dark;
/// `None` keeps the ordinary theme default.
pub fn smart_art_fallback_text_color(
    background: Option<&Fill>,
    theme_default_color: &str,
) -> Option<String> {
    let bg = background_luminance(background)?;
    if bg >= 0.5 {
        return None;
    }
    let theme_luma = hex_luma(theme_default_color);
    if theme_luma.is_some_and(|luma| luma >= 0.5) {
        return None;
    }
    Some("FFFFFF".to_string())
}
