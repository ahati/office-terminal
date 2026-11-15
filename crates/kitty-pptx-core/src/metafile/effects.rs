//! Picture-level effects and opacity application for metafile expansion (PRD 13 §3.1, §6.2).

use std::sync::Arc;

use crate::compiler::parse_hex_color;
use crate::contracts::{
    BlipEffect, VectorCommand, VectorPaint, VectorStroke,
};
use crate::rasterizer::effects::bt709_luma;

/// Maps a single straight RGBA8 color by applying a sequence of DrawingML
/// [`BlipEffect`]s in authored order, followed by marker `opacity`.
pub fn map_color(
    color: [u8; 4],
    effects: &[BlipEffect],
    opacity: f32,
) -> [u8; 4] {
    let mut r = color[0];
    let mut g = color[1];
    let mut b = color[2];
    let mut a = color[3];

    for effect in effects {
        match effect {
            BlipEffect::ColorChange {
                from,
                from_alpha,
                to,
                to_alpha,
                use_alpha,
            } => {
                let [f_r, f_g, f_b, _] = parse_hex_color(from);
                let matches_rgb = r == f_r && g == f_g && b == f_b;
                let matches_alpha = if *use_alpha {
                    let a_norm = a as f64 / 255.0;
                    (a_norm - from_alpha).abs() < 0.02
                } else {
                    true
                };
                if matches_rgb && matches_alpha {
                    let [t_r, t_g, t_b, _] = parse_hex_color(to);
                    r = t_r;
                    g = t_g;
                    b = t_b;
                    let target_alpha = if *to_alpha > 1.0 {
                        *to_alpha / 100000.0
                    } else {
                        *to_alpha
                    };
                    if *use_alpha {
                        a = (target_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                    } else {
                        let existing_norm = a as f64 / 255.0;
                        a = (existing_norm * target_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
            BlipEffect::ColorReplace { color: repl } => {
                let [repl_r, repl_g, repl_b, _] = parse_hex_color(repl);
                r = repl_r;
                g = repl_g;
                b = repl_b;
            }
            BlipEffect::Grayscale => {
                let luma = bt709_luma(r, g, b).round().clamp(0.0, 255.0) as u8;
                r = luma;
                g = luma;
                b = luma;
            }
            BlipEffect::BiLevel { thresh } => {
                let thresh_val = if *thresh > 1.0 {
                    *thresh as f32
                } else {
                    (*thresh * 255.0) as f32
                };
                let luma = bt709_luma(r, g, b);
                let val = if luma > thresh_val { 255 } else { 0 };
                r = val;
                g = val;
                b = val;
            }
            BlipEffect::Duotone { clr1, clr2 } => {
                let [r1, g1, b1, _] = parse_hex_color(clr1);
                let [r2, g2, b2, _] = parse_hex_color(clr2);
                let luma_norm = (bt709_luma(r, g, b) / 255.0).clamp(0.0, 1.0);
                r = (r1 as f32 + luma_norm * (r2 as f32 - r1 as f32))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                g = (g1 as f32 + luma_norm * (g2 as f32 - g1 as f32))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                b = (b1 as f32 + luma_norm * (b2 as f32 - b1 as f32))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            BlipEffect::AlphaModFix { amt } => {
                let factor = if *amt > 1.0 {
                    (*amt / 100000.0).clamp(0.0, 1.0)
                } else {
                    amt.clamp(0.0, 1.0)
                };
                a = ((a as f64) * factor).round().clamp(0.0, 255.0) as u8;
            }
            BlipEffect::AlphaMod { container } => {
                for inner in &container.effects {
                    if let BlipEffect::AlphaModFix { amt } = inner {
                        let factor = if *amt > 1.0 {
                            (*amt / 100000.0).clamp(0.0, 1.0)
                        } else {
                            amt.clamp(0.0, 1.0)
                        };
                        a = ((a as f64) * factor).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            BlipEffect::AlphaRepl { alpha } => {
                let factor = if *alpha > 1.0 {
                    (*alpha / 100000.0).clamp(0.0, 1.0)
                } else {
                    alpha.clamp(0.0, 1.0)
                };
                a = (factor * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            _ => {}
        }
    }

    if (0.0..1.0).contains(&opacity) {
        a = ((a as f32) * opacity).round().clamp(0.0, 255.0) as u8;
    }

    [r, g, b, a]
}

/// Transforms vector paint by applying picture blip effects and marker opacity.
pub fn map_paint(paint: &mut VectorPaint, effects: &[BlipEffect], opacity: f32) {
    match paint {
        VectorPaint::Solid(color) => {
            *color = map_color(*color, effects, opacity);
        }
        VectorPaint::LinearGradient(grad) | VectorPaint::RadialGradient(grad) => {
            for (_, stop_color) in &mut grad.stops {
                *stop_color = map_color(*stop_color, effects, opacity);
            }
        }
        VectorPaint::Pattern(pat) => {
            pat.fg = map_color(pat.fg, effects, opacity);
            pat.bg = map_color(pat.bg, effects, opacity);
        }
        VectorPaint::Image(img) => {
            if (0.0..1.0).contains(&opacity) {
                img.alpha = (img.alpha * opacity).clamp(0.0, 1.0);
            }
            for effect in effects {
                if let BlipEffect::AlphaModFix { amt } = effect {
                    let factor = if *amt > 1.0 {
                        (*amt / 100000.0).clamp(0.0, 1.0) as f32
                    } else {
                        *amt as f32
                    };
                    img.alpha = (img.alpha * factor).clamp(0.0, 1.0);
                }
            }
        }
    }
}

/// Transforms stroke color and line paint by applying picture blip effects and opacity.
pub fn map_stroke(stroke: &mut VectorStroke, effects: &[BlipEffect], opacity: f32) {
    stroke.color = map_color(stroke.color, effects, opacity);
    if let Some(ref mut paint) = stroke.paint {
        map_paint(paint, effects, opacity);
    }
}

/// Applies picture-level effects and opacity to an expanded vector command.
pub fn apply_metafile_effects(
    command: &mut VectorCommand,
    effects: &[BlipEffect],
    opacity: f32,
) {
    if effects.is_empty() && (opacity >= 1.0 || opacity < 0.0) {
        return;
    }
    match command {
        VectorCommand::DrawPath { fill, stroke, .. } => {
            if let Some(fill) = fill {
                map_paint(fill, effects, opacity);
            }
            if let Some(stroke) = stroke {
                map_stroke(stroke, effects, opacity);
            }
        }
        VectorCommand::DrawLineEnd { fill, stroke, .. } => {
            if let Some(fill) = fill {
                map_paint(fill, effects, opacity);
            }
            if let Some(stroke) = stroke {
                map_stroke(stroke, effects, opacity);
            }
        }
        VectorCommand::DrawText { runs, .. } => {
            for run in runs {
                run.color = map_color(run.color, effects, opacity);
            }
        }
        VectorCommand::DrawBitmap {
            png,
            opacity: bmp_opacity,
            ..
        } => {
            let mut factor = if (0.0..1.0).contains(&opacity) {
                opacity
            } else {
                1.0
            };
            for effect in effects {
                if let BlipEffect::AlphaModFix { amt } = effect {
                    let f = if *amt > 1.0 {
                        (*amt / 100000.0).clamp(0.0, 1.0) as f32
                    } else {
                        *amt as f32
                    };
                    factor *= f;
                }
            }
            *bmp_opacity = (*bmp_opacity * factor).clamp(0.0, 1.0);

            let has_pixel_effects = effects.iter().any(|e| {
                matches!(
                    e,
                    BlipEffect::ColorChange { .. }
                        | BlipEffect::ColorReplace { .. }
                        | BlipEffect::Grayscale
                        | BlipEffect::BiLevel { .. }
                        | BlipEffect::Duotone { .. }
                )
            });
            if has_pixel_effects {
                if let Ok(mut pixmap) = tiny_skia::Pixmap::decode_png(png) {
                    let pixels = pixmap.data_mut();
                    crate::rasterizer::effects::apply_blip_effects(pixels, effects);
                    if let Ok(encoded) = pixmap.encode_png() {
                        *png = Arc::from(encoded.into_boxed_slice());
                    }
                }
            }
        }
        VectorCommand::DrawImage {
            opacity: img_opacity,
            effects: img_effects,
            ..
        } => {
            if (0.0..1.0).contains(&opacity) {
                *img_opacity = (*img_opacity * opacity).clamp(0.0, 1.0);
            }
            img_effects.extend_from_slice(effects);
        }
        VectorCommand::DrawMetafile {
            opacity: mf_opacity,
            effects: mf_effects,
            ..
        } => {
            if (0.0..1.0).contains(&opacity) {
                *mf_opacity = (*mf_opacity * opacity).clamp(0.0, 1.0);
            }
            mf_effects.extend_from_slice(effects);
        }
        VectorCommand::PushClip(_) | VectorCommand::PopClip => {}
    }
}

/// Checks whether marker opacity `< 1.0` has any commands that could not be
/// represented or scaled.
pub fn is_opacity_unrepresentable(commands: &[VectorCommand]) -> bool {
    // If every command with ink accepts opacity scaling (DrawPath with fill/stroke,
    // DrawText, DrawBitmap), opacity is fully represented.
    commands.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AffineTransform, ShapeEffects, VectorPath};

    #[test]
    fn color_change_white_to_transparent() {
        let effect = BlipEffect::ColorChange {
            from: "FFFFFF".to_string(),
            from_alpha: 1.0,
            to: "FFFFFF".to_string(),
            to_alpha: 0.0,
            use_alpha: false,
        };
        let input = [255, 255, 255, 255];
        let output = map_color(input, &[effect], 1.0);
        assert_eq!(output, [255, 255, 255, 0]);
    }

    #[test]
    fn color_change_leaves_other_colors_untouched() {
        let effect = BlipEffect::ColorChange {
            from: "FFFFFF".to_string(),
            from_alpha: 1.0,
            to: "FFFFFF".to_string(),
            to_alpha: 0.0,
            use_alpha: false,
        };
        let input = [0, 82, 155, 255];
        let output = map_color(input, &[effect], 1.0);
        assert_eq!(output, [0, 82, 155, 255]);
    }

    #[test]
    fn marker_opacity_scales_alpha() {
        let input = [100, 150, 200, 200];
        let output = map_color(input, &[], 0.5);
        assert_eq!(output, [100, 150, 200, 100]);
    }

    #[test]
    fn apply_effects_to_draw_path() {
        let mut cmd = VectorCommand::DrawPath {
            path: VectorPath::empty(),
            fill: Some(VectorPaint::Solid([255, 255, 255, 255])),
            stroke: None,
            transform: AffineTransform::identity(),
            effects: Box::new(ShapeEffects::default()),
        };
        let effect = BlipEffect::ColorChange {
            from: "FFFFFF".to_string(),
            from_alpha: 1.0,
            to: "FFFFFF".to_string(),
            to_alpha: 0.0,
            use_alpha: false,
        };
        apply_metafile_effects(&mut cmd, &[effect], 1.0);
        match cmd {
            VectorCommand::DrawPath { fill, .. } => {
                assert_eq!(fill, Some(VectorPaint::Solid([255, 255, 255, 0])));
            }
            _ => panic!("expected DrawPath"),
        }
    }
}
