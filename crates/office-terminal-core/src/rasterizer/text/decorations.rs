//! Paint run underline/strike bands from the compiler's decoration IR.
//!
//! Geometry arrives in EMU relative to the run baseline, so both the upright
//! and affine text backends use the same face-derived rule. Pattern styles are
//! drawn as segmented bands; the rasterizer has no stroke-dash primitive.

use crate::contracts::{DecorationLine, DecorationStyle, PositionedGlyphRun};

/// Offsets from the single rule's centre for one or two bands.
pub(super) fn band_offsets(line: &DecorationLine) -> [Option<f32>; 2] {
    if line.gap_emu > 0.0 {
        [Some(-line.gap_emu * 0.5), Some(line.gap_emu * 0.5)]
    } else {
        [Some(0.0), None]
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_upright(
    pix_data: &mut [u8],
    pix_w: i32,
    pix_h: i32,
    run: &PositionedGlyphRun,
    draw_x: i32,
    baseline_y: f32,
    width_px: i32,
    scale: f32,
) {
    if width_px <= 0 {
        return;
    }
    for line in [run.decorations.underline, run.decorations.strike]
        .into_iter()
        .flatten()
    {
        let thickness = (line.thickness_emu * scale).max(1.0).round() as i32;
        let top = baseline_y + line.top_emu * scale;
        for offset in band_offsets(&line).into_iter().flatten() {
            let y = (top + offset * scale).round() as i32;
            paint_band(
                pix_data, pix_w, pix_h, draw_x, y, width_px, thickness, line.style, line.color,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_band(
    pix_data: &mut [u8],
    pix_w: i32,
    pix_h: i32,
    x: i32,
    y: i32,
    width: i32,
    thickness: i32,
    style: DecorationStyle,
    color: [u8; 4],
) {
    let [r, g, b, a] = color;
    match style {
        DecorationStyle::Solid => {
            super::draw_rect_fill(pix_data, pix_w, pix_h, x, y, width, thickness, r, g, b, a);
        }
        DecorationStyle::Wavy => {
            // Library approximation: one sine wavelength per six line widths,
            // amplitude one line width.
            let amp = thickness as f32;
            let wavelength = (amp * 6.0).max(1.0);
            for dx in 0..width {
                let yy = y + (amp + ((dx as f32 / wavelength) * std::f32::consts::TAU).sin() * amp) as i32;
                super::draw_rect_fill(pix_data, pix_w, pix_h, x + dx, yy, 1, thickness, r, g, b, a);
            }
        }
        _ => {
            // `ST_TextUnderlineType` dash families, in line-width units.
            let (on, off) = match style {
                DecorationStyle::Dotted => (1.0, 1.0),
                DecorationStyle::Dash => (3.0, 1.0),
                DecorationStyle::DashLong => (6.0, 2.0),
                DecorationStyle::DotDash => (1.0, 1.0),
                DecorationStyle::DotDotDash => (1.0, 1.0),
                _ => (1.0, 0.0),
            };
            let on = (on * thickness as f32).round().max(1.0) as i32;
            let off = (off * thickness as f32).round().max(0.0) as i32;
            let mut cursor = 0;
            while cursor < width {
                let len = on.min(width - cursor);
                super::draw_rect_fill(pix_data, pix_w, pix_h, x + cursor, y, len, thickness, r, g, b, a);
                cursor += on + off;
            }
        }
    }
}
