mod image;
mod path;
mod text;

use std::fmt::Write;
use crate::contracts::{
    SvgTextMode, VectorBackground, VectorCommand, VectorPaint, VectorSlide,
};
use crate::error::RenderError;
use crate::loader::AssetStore;

pub trait SlideSvgSerializer: Send + Sync {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError>;
}

pub struct DefaultSlideSvgSerializer;

impl DefaultSlideSvgSerializer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultSlideSvgSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl SlideSvgSerializer for DefaultSlideSvgSerializer {
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError> {
        let mut svg = String::new();
        let w = scene.width_emu;
        let h = scene.height_emu;

        writeln!(&mut svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        writeln!(
            &mut svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"#,
            w, h
        )
        .unwrap();

        // 1. Defs section for gradients and clip paths
        let mut defs = String::new();
        let mut grad_id_counter = 0usize;
        let mut path_grad_ids: Vec<Option<String>> = Vec::new();

        // Background gradient
        let bg_grad_id = match scene.background {
            Some(VectorBackground::Gradient(ref grad)) => {
                let id = "bg_grad".to_string();
                emit_linear_gradient_def(&mut defs, &id, grad);
                Some(id)
            }
            _ => None,
        };

        // Command gradients
        for cmd in &scene.commands {
            if let VectorCommand::DrawPath { fill, .. } = cmd {
                match fill {
                    Some(VectorPaint::LinearGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("grad_{}", grad_id_counter);
                        emit_linear_gradient_def(&mut defs, &id, grad);
                        path_grad_ids.push(Some(id));
                    }
                    Some(VectorPaint::RadialGradient(ref grad)) => {
                        grad_id_counter += 1;
                        let id = format!("grad_{}", grad_id_counter);
                        emit_radial_gradient_def(&mut defs, &id, grad);
                        path_grad_ids.push(Some(id));
                    }
                    _ => path_grad_ids.push(None),
                }
            } else {
                path_grad_ids.push(None);
            }
        }

        if !defs.is_empty() {
            writeln!(&mut svg, "  <defs>").unwrap();
            svg.push_str(&defs);
            writeln!(&mut svg, "  </defs>").unwrap();
        }

        // 2. Background rect
        if let Some(ref bg) = scene.background {
            match bg {
                VectorBackground::Solid([r, g, b, a]) => {
                    let opacity = *a as f32 / 255.0;
                    writeln!(
                        &mut svg,
                        r#"  <rect width="{}" height="{}" fill="rgb({},{},{})" fill-opacity="{:.2}"/>"#,
                        w, h, r, g, b, opacity
                    )
                    .unwrap();
                }
                VectorBackground::Gradient(_) => {
                    if let Some(ref id) = bg_grad_id {
                        writeln!(
                            &mut svg,
                            r#"  <rect width="{}" height="{}" fill="url(#{})"/>"#,
                            w, h, id
                        )
                        .unwrap();
                    }
                }
                VectorBackground::Image { rel_id } => {
                    let dest = crate::contracts::RectF32::new(0.0, 0.0, w as f32, h as f32);
                    let img_svg = image::serialize_svg_image(
                        assets,
                        rel_id,
                        &crate::contracts::NormalizedRect::default(),
                        &dest,
                        1.0,
                        &crate::contracts::AffineTransform::default(),
                        &[],
                    );
                    if !img_svg.is_empty() {
                        svg.push_str(&img_svg);
                        svg.push('\n');
                    }
                }
            }
        } else {
            writeln!(
                &mut svg,
                r##"  <rect width="{}" height="{}" fill="#ffffff"/>"##,
                w, h
            )
            .unwrap();
        }

        // 3. Render vector commands
        for (cmd_idx, cmd) in scene.commands.iter().enumerate() {
            match cmd {
                VectorCommand::DrawPath {
                    path: v_path,
                    fill,
                    stroke,
                    transform,
                    ..
                } => {
                    let d = path::path_to_svg_d(v_path);
                    let grad_id = path_grad_ids.get(cmd_idx).and_then(|id| id.as_deref());
                    let fill_attr = path::format_fill_attr(fill.as_ref(), grad_id);
                    let stroke_attr = path::format_stroke_attr(stroke.as_ref());
                    let transform_attr = path::format_transform_attr(transform);
                    let fill_rule_attr = path::format_fill_rule_attr(v_path.fill_rule);

                    writeln!(
                        &mut svg,
                        r#"  <path d="{}" {} {}{}{}/>"#,
                        d, fill_attr, stroke_attr, fill_rule_attr, transform_attr
                    )
                    .unwrap();
                }
                VectorCommand::DrawImage {
                    rel_id,
                    src_rect,
                    dest_rect,
                    opacity,
                    transform,
                    effects,
                } => {
                    let img_svg = image::serialize_svg_image(
                        assets,
                        rel_id,
                        src_rect,
                        dest_rect,
                        *opacity,
                        transform,
                        effects,
                    );
                    if !img_svg.is_empty() {
                        svg.push_str(&img_svg);
                        svg.push('\n');
                    }
                }
                VectorCommand::DrawText { runs, transform } => {
                    let text_svg = text::serialize_svg_text(runs, transform, text_mode);
                    svg.push_str(&text_svg);
                }
                VectorCommand::PushClip(_) | VectorCommand::PopClip => {}
            }
        }

        writeln!(&mut svg, "</svg>").unwrap();
        Ok(svg)
    }
}

fn emit_linear_gradient_def(defs: &mut String, id: &str, grad: &crate::contracts::VectorGradient) {
    let rad = grad.angle_deg.to_radians();
    let x1 = 50.0 - 50.0 * rad.cos();
    let y1 = 50.0 - 50.0 * rad.sin();
    let x2 = 50.0 + 50.0 * rad.cos();
    let y2 = 50.0 + 50.0 * rad.sin();

    let _ = writeln!(
        defs,
        r#"    <linearGradient id="{}" x1="{:.1}%" y1="{:.1}%" x2="{:.1}%" y2="{:.1}%">"#,
        id, x1, y1, x2, y2
    );
    for (pos, [r, g, b, a]) in &grad.stops {
        let opacity = *a as f32 / 255.0;
        let _ = writeln!(
            defs,
            r#"      <stop offset="{:.1}%" stop-color="rgb({},{},{})" stop-opacity="{:.2}"/>"#,
            pos * 100.0,
            r,
            g,
            b,
            opacity
        );
    }
    let _ = writeln!(defs, "    </linearGradient>");
}

fn emit_radial_gradient_def(defs: &mut String, id: &str, grad: &crate::contracts::VectorGradient) {
    let _ = writeln!(
        defs,
        r#"    <radialGradient id="{}" cx="50%" cy="50%" r="50%">"#,
        id
    );
    for (pos, [r, g, b, a]) in &grad.stops {
        let opacity = *a as f32 / 255.0;
        let _ = writeln!(
            defs,
            r#"      <stop offset="{:.1}%" stop-color="rgb({},{},{})" stop-opacity="{:.2}"/>"#,
            pos * 100.0,
            r,
            g,
            b,
            opacity
        );
    }
    let _ = writeln!(defs, "    </radialGradient>");
}
