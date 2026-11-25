//! Resolve run underline/strike geometry from the drawing face (T7).
//!
//! ECMA-376 names the underline and strike styles but not their position or
//! thickness. The reference uses the face's OpenType/OS/2 rule when the pinned
//! catalogue has it, then falls back to its library policy
//! (`packages/core/src/text/underline.ts`, `renderer.ts:809-872`). The result is
//! stored in the vector IR so the rasterizer and both SVG modes share one rule.

use crate::contracts::{DecorationLine, DecorationStyle, TextDecorations};
use crate::text::emit::StyleDef;
use crate::text::faces::{power_point_face_metrics, DecorationGeometry};

/// One CSS/device pixel at 96 DPI, used by the reference's 1 px minimum
/// (the shared contracts constant `EMU_PER_PX`, aliased for the local name).
use crate::contracts::EMU_PER_PX as PX_EMU;

pub(super) fn resolve(def: &StyleDef) -> TextDecorations {
    resolve_raw(
        &def.family,
        def.bold,
        def.italic,
        def.draw_size_emu(),
        def.color,
        def.underline,
        def.underline_style.as_deref(),
        def.underline_color,
        def.strikethrough,
        def.strike_double,
    )
}

/// Resolve from raw run fields; the warp compiler does not build a `StyleDef`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_raw(
    family: &str,
    bold: bool,
    italic: bool,
    size_emu: f32,
    color: [u8; 4],
    underline: bool,
    underline_style: Option<&str>,
    underline_color: Option<[u8; 4]>,
    strikethrough: bool,
    strike_double: bool,
) -> TextDecorations {
    let face = power_point_face_metrics(family, bold, italic).and_then(|m| m.decoration);
    TextDecorations {
        underline: underline.then(|| underline_line(size_emu, face, underline_style, underline_color.unwrap_or(color))),
        strike: strikethrough.then(|| strike_line(size_emu, face, color, strike_double)),
    }
}

fn underline_line(
    size_emu: f32,
    face: Option<DecorationGeometry>,
    style: Option<&str>,
    color: [u8; 4],
) -> DecorationLine {
    let (pattern, heavy, double, wavy) = underline_kind(style);
    let base = face
        .map(|d| d.underline_thickness_em * size_emu)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or((size_emu * 0.05).max(PX_EMU));
    let thickness = if heavy {
        base * if face.is_some() { 1.5 } else { 1.8 }
    } else {
        base
    };
    // The face rule positions the top of the single stroke; the fallback puts
    // its centre `max(2, lineW)` below the baseline (`underline.ts:97-103`).
    let centre = match face {
        Some(d) => d.underline_top_em * size_emu + base * 0.5,
        None => thickness.max(2.0 * PX_EMU),
    };
    let gap = if double {
        thickness * if wavy { 2.5 } else { 1.4 }
    } else {
        0.0
    };
    DecorationLine {
        color,
        top_emu: centre - thickness * 0.5,
        thickness_emu: thickness,
        gap_emu: gap,
        style: pattern,
    }
}

fn strike_line(size_emu: f32, face: Option<DecorationGeometry>, color: [u8; 4], double: bool) -> DecorationLine {
    let thickness = face
        .map(|d| d.strike_thickness_em * size_emu)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or((size_emu * 0.05).max(PX_EMU));
    // A catalogued face gives the top of the strike; the fallback centres it
    // 0.32 em above the baseline (`renderer.ts:838-853`).
    let top = match face {
        Some(d) => d.strike_top_em * size_emu,
        None => -size_emu * 0.32 - thickness * 0.5,
    };
    let gap = if double {
        if face.is_some() {
            thickness
        } else {
            thickness * 0.9
        }
    } else {
        0.0
    };
    DecorationLine {
        color,
        top_emu: top,
        thickness_emu: thickness,
        gap_emu: gap,
        style: DecorationStyle::Solid,
    }
}

/// `ST_TextUnderlineType` → pattern + heavy + double flags.
fn underline_kind(value: Option<&str>) -> (DecorationStyle, bool, bool, bool) {
    let value = value.unwrap_or("sng");
    match value {
        "dbl" => (DecorationStyle::Solid, false, true, false),
        "heavy" => (DecorationStyle::Solid, true, false, false),
        "dotted" => (DecorationStyle::Dotted, false, false, false),
        "dottedHeavy" => (DecorationStyle::Dotted, true, false, false),
        "dash" => (DecorationStyle::Dash, false, false, false),
        "dashHeavy" => (DecorationStyle::Dash, true, false, false),
        "dashLong" => (DecorationStyle::DashLong, false, false, false),
        "dashLongHeavy" => (DecorationStyle::DashLong, true, false, false),
        "dotDash" => (DecorationStyle::DotDash, false, false, false),
        "dotDashHeavy" => (DecorationStyle::DotDash, true, false, false),
        "dotDotDash" => (DecorationStyle::DotDotDash, false, false, false),
        "dotDotDashHeavy" => (DecorationStyle::DotDotDash, true, false, false),
        "wavy" => (DecorationStyle::Wavy, false, false, true),
        "wavyHeavy" => (DecorationStyle::Wavy, true, false, true),
        "wavyDbl" => (DecorationStyle::Wavy, false, true, true),
        _ => (DecorationStyle::Solid, false, false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heavy_is_thicker_than_single() {
        let single = underline_line(24.0 * 12_700.0, None, Some("sng"), [0, 0, 0, 255]);
        let heavy = underline_line(24.0 * 12_700.0, None, Some("heavy"), [0, 0, 0, 255]);
        assert!(heavy.thickness_emu > single.thickness_emu);
        assert_eq!(heavy.gap_emu, 0.0);
    }

    #[test]
    fn double_rules_are_one_point_four_line_widths_apart() {
        let double = underline_line(24.0 * 12_700.0, None, Some("dbl"), [0, 0, 0, 255]);
        assert!((double.gap_emu - double.thickness_emu * 1.4).abs() < 1e-3);
    }

    #[test]
    fn strike_double_uses_the_reference_gap() {
        let single = strike_line(24.0 * 12_700.0, None, [0, 0, 0, 255], false);
        let double = strike_line(24.0 * 12_700.0, None, [0, 0, 0, 255], true);
        assert!((double.gap_emu - single.thickness_emu * 0.9).abs() < 1e-3);
    }
}
