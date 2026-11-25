use crate::compiler::parse_hex_color;
use ooxml_common::blip::BlipEffect;

/// Compute BT.709 relative luminance from 8-bit RGB components.
/// Formula: Y = 0.2126 * R + 0.7152 * G + 0.0722 * B.
#[inline]
pub fn bt709_luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * (r as f32) + 0.7152 * (g as f32) + 0.0722 * (b as f32)
}

/// Compute BT.709 relative luminance rounded and clamped to 0..=255.
#[inline]
pub fn bt709_luma_u8(r: u8, g: u8, b: u8) -> u8 {
    bt709_luma(r, g, b).round().clamp(0.0, 255.0) as u8
}

/// Convert straight RGBA8 buffer to grayscale in-place using BT.709 coefficients.
/// Alpha channel is untouched.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_grayscale(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let y = bt709_luma_u8(chunk[0], chunk[1], chunk[2]);
        chunk[0] = y;
        chunk[1] = y;
        chunk[2] = y;
    }
}

/// Apply bi-level black-and-white thresholding in-place to straight RGBA8 pixels.
/// If BT.709 luminance > threshold, white (255, 255, 255); else black (0, 0, 0).
/// Alpha channel is untouched.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_bilevel(pixels: &mut [u8], thresh: f64) {
    let thresh_val = if thresh > 1.0 {
        thresh as f32
    } else {
        (thresh * 255.0) as f32
    };

    for chunk in pixels.chunks_exact_mut(4) {
        let y = bt709_luma(chunk[0], chunk[1], chunk[2]);
        let val = if y > thresh_val { 255 } else { 0 };
        chunk[0] = val;
        chunk[1] = val;
        chunk[2] = val;
    }
}

/// Apply duotone tinting in-place to straight RGBA8 pixels.
/// Linearly interpolates between `clr1` (dark endpoint, luma 0.0) and
/// `clr2` (light endpoint, luma 1.0) based on BT.709 luma.
/// Alpha channel is untouched.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_duotone(pixels: &mut [u8], clr1: &str, clr2: &str) {
    let [r1, g1, b1, _] = parse_hex_color(clr1);
    let [r2, g2, b2, _] = parse_hex_color(clr2);

    for chunk in pixels.chunks_exact_mut(4) {
        let y_norm = (bt709_luma(chunk[0], chunk[1], chunk[2]) / 255.0).clamp(0.0, 1.0);
        let r_out = (r1 as f32 + y_norm * (r2 as f32 - r1 as f32)).round().clamp(0.0, 255.0) as u8;
        let g_out = (g1 as f32 + y_norm * (g2 as f32 - g1 as f32)).round().clamp(0.0, 255.0) as u8;
        let b_out = (b1 as f32 + y_norm * (b2 as f32 - b1 as f32)).round().clamp(0.0, 255.0) as u8;

        chunk[0] = r_out;
        chunk[1] = g_out;
        chunk[2] = b_out;
    }
}

/// Apply alpha modulation in-place to straight RGBA8 pixels.
/// Multiplies pixel alpha by factor (0.0..=1.0).
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_alpha_mod(pixels: &mut [u8], alpha_factor: f64) {
    let factor = if alpha_factor > 1.0 {
        (alpha_factor / 100000.0).clamp(0.0, 1.0) as f32
    } else {
        alpha_factor.clamp(0.0, 1.0) as f32
    };

    for chunk in pixels.chunks_exact_mut(4) {
        let a = (chunk[3] as f32 * factor).round().clamp(0.0, 255.0) as u8;
        chunk[3] = a;
    }
}

/// Apply color change in-place to straight RGBA8 pixels.
/// Matches pixels whose RGB (and optionally alpha) matches `from`, and maps them to `to` and `to_alpha`.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_color_change(pixels: &mut [u8], from: &str, from_alpha: f64, to: &str, to_alpha: f64, use_alpha: bool) {
    let [f_r, f_g, f_b, _] = parse_hex_color(from);
    let [t_r, t_g, t_b, _] = parse_hex_color(to);
    let target_alpha = if to_alpha > 1.0 { to_alpha / 100000.0 } else { to_alpha };

    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];

        let matches_rgb = r == f_r && g == f_g && b == f_b;
        let matches_alpha = if use_alpha {
            let a_norm = a as f64 / 255.0;
            (a_norm - from_alpha).abs() < 0.02
        } else {
            true
        };

        if matches_rgb && matches_alpha {
            chunk[0] = t_r;
            chunk[1] = t_g;
            chunk[2] = t_b;
            if use_alpha {
                chunk[3] = (target_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            } else {
                let existing_norm = a as f64 / 255.0;
                chunk[3] = (existing_norm * target_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

/// Replace RGB of non-transparent pixels with `color`, keeping alpha intact.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_color_replace(pixels: &mut [u8], color: &str) {
    let [t_r, t_g, t_b, _] = parse_hex_color(color);
    for chunk in pixels.chunks_exact_mut(4) {
        if chunk[3] > 0 {
            chunk[0] = t_r;
            chunk[1] = t_g;
            chunk[2] = t_b;
        }
    }
}

/// Replace alpha of pixels with `alpha_val` (0.0..=1.0).
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn apply_alpha_repl(pixels: &mut [u8], alpha_val: f64) {
    let factor = if alpha_val > 1.0 {
        (alpha_val / 100000.0).clamp(0.0, 1.0)
    } else {
        alpha_val.clamp(0.0, 1.0)
    };
    let a = (factor * 255.0).round().clamp(0.0, 255.0) as u8;
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[3] = a;
    }
}

/// Apply a sequence of DrawingML BlipEffect transformations to straight RGBA8 pixels in order.
pub fn apply_blip_effects(pixels: &mut [u8], effects: &[BlipEffect]) {
    for effect in effects {
        match effect {
            BlipEffect::Grayscale => {
                apply_grayscale(pixels);
            }
            BlipEffect::BiLevel { thresh } => {
                apply_bilevel(pixels, *thresh);
            }
            BlipEffect::ColorChange {
                from,
                from_alpha,
                to,
                to_alpha,
                use_alpha,
            } => {
                apply_color_change(pixels, from, *from_alpha, to, *to_alpha, *use_alpha);
            }
            BlipEffect::ColorReplace { color } => {
                apply_color_replace(pixels, color);
            }
            BlipEffect::Duotone { clr1, clr2 } => {
                apply_duotone(pixels, clr1, clr2);
            }
            BlipEffect::AlphaModFix { amt } => {
                apply_alpha_mod(pixels, *amt);
            }
            BlipEffect::AlphaMod { container } => {
                for inner in &container.effects {
                    if let BlipEffect::AlphaModFix { amt } = inner {
                        apply_alpha_mod(pixels, *amt);
                    }
                }
            }
            BlipEffect::AlphaRepl { alpha } => {
                apply_alpha_repl(pixels, *alpha);
            }
            _ => {
                // Other blip effects are omitted or non-visual diagnostics
            }
        }
    }
}
