//! SVG `<filter>` / `<mask>` graph emission for shape effects (PRD 08 §5).
//!
//! The serializer stays purely declarative: shadows, glows, inner shadows, and
//! soft edges become standard W3C filter primitives; reflections become a
//! mirrored `<use>` clone attenuated by a linear-gradient mask. Every id is
//! derived from the draw-command index, so exports are deterministic.

use std::fmt::Write;

use crate::contracts::{AffineTransform, PathSegment, ShapeEffects, VectorPath};
use crate::scene3d;

/// Per-path effect references produced while emitting the `<defs>` block.
pub struct EffectRefs {
    pub path_id: String,
    pub needs_path_id: bool,
    pub filter_id: Option<String>,
    /// Complete `<use .../>` element for the mirrored reflection clone.
    pub reflection_use: Option<String>,
}

/// Emit all effect definitions for one `DrawPath` command and return the ids
/// the path element must link to.
pub fn emit_effect_defs(
    index: usize,
    path: &VectorPath,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    slide_width_emu: i64,
    defs: &mut String,
) -> Option<EffectRefs> {
    if effects.is_empty() {
        return None;
    }
    let effective = projected_transform(path, transform, effects, slide_width_emu);
    let mut refs = EffectRefs {
        path_id: format!("shape-{index}"),
        needs_path_id: false,
        filter_id: None,
        reflection_use: None,
    };

    if let Some(id) = emit_filter(index, effects, defs) {
        refs.filter_id = Some(id);
    }

    if let Some(reflection) = effects.reflection.as_ref() {
        if let Some(bounds) = path_bounds(path, &effective) {
            if reflection.start_opacity > 0.0 {
                let mask_id = format!("refl-mask-{index}");
                let grad_id = format!("refl-grad-{index}");
                emit_reflection_defs(&mask_id, &grad_id, reflection, defs);
                let mirror = 2.0 * bounds.3 + reflection.distance_emu as f32;
                refs.reflection_use = Some(format!(
                    r##"<use href="#{}" transform="translate(0, {}) scale(1, -1)" mask="url(#{})"/>"##,
                    refs.path_id,
                    fmt_num(mirror),
                    mask_id
                ));
                refs.needs_path_id = true;
            }
        }
    }

    Some(refs)
}

/// Emit a single combined filter graph covering every filter-based effect
/// attached to the path. Returns the filter id.
fn emit_filter(index: usize, effects: &ShapeEffects, defs: &mut String) -> Option<String> {
    let filter_id = format!("fx-{index}");
    let mut body = String::new();
    let mut under_layers: Vec<&str> = Vec::new();

    if let Some(shadow) = effects.outer_shadow.as_ref() {
        let sigma = shadow.blur_radius_emu as f32 / 2.0;
        let (dx, dy) = directional_offset(shadow.direction_deg, shadow.distance_emu as f32);
        if sigma > 0.0 {
            let _ = writeln!(
                body,
                r#"      <feGaussianBlur in="SourceAlpha" stdDeviation="{}" result="fxShadowBlur"/>"#,
                fmt_num(sigma)
            );
        }
        let blur_result = if sigma > 0.0 { "fxShadowBlur" } else { "SourceAlpha" };
        let _ = writeln!(
            body,
            r#"      <feOffset in="{}" dx="{}" dy="{}" result="fxShadowOffset"/>"#,
            blur_result,
            fmt_num(dx),
            fmt_num(dy)
        );
        let [r, g, b, a] = shadow.color;
        let _ = writeln!(
            body,
            r##"      <feFlood flood-color="#{:02x}{:02x}{:02x}" flood-opacity="{:.2}" result="fxShadowColor"/>"##,
            r,
            g,
            b,
            a as f32 / 255.0
        );
        body.push_str(
            "      <feComposite in=\"fxShadowColor\" in2=\"fxShadowOffset\" operator=\"in\" result=\"fxShadow\"/>\n",
        );
        under_layers.push("fxShadow");
    }

    if let Some(glow) = effects.glow.as_ref() {
        let sigma = glow.radius_emu as f32 / 2.5;
        if sigma > 0.0 {
            let _ = writeln!(
                body,
                r#"      <feGaussianBlur in="SourceAlpha" stdDeviation="{}" result="fxGlowBlur"/>"#,
                fmt_num(sigma)
            );
            body.push_str(
                "      <feComponentTransfer in=\"fxGlowBlur\" result=\"fxGlowExpand\">\n        <feFuncA type=\"table\" tableValues=\"0 0.234 0.444 0.610 0.750 0.859 0.938 0.984 1\"/>\n      </feComponentTransfer>\n",
            );
            let [r, g, b, a] = glow.color;
            let _ = writeln!(
                body,
                r##"      <feFlood flood-color="#{:02x}{:02x}{:02x}" flood-opacity="{:.2}" result="fxGlowColor"/>"##,
                r,
                g,
                b,
                a as f32 / 255.0
            );
            body.push_str(
                "      <feComposite in=\"fxGlowColor\" in2=\"fxGlowExpand\" operator=\"in\" result=\"fxGlow\"/>\n",
            );
            under_layers.push("fxGlow");
        }
    }

    let mut current = if under_layers.is_empty() {
        "SourceGraphic".to_string()
    } else {
        body.push_str("      <feMerge result=\"fxBase\">\n");
        for layer in &under_layers {
            let _ = writeln!(body, r#"        <feMergeNode in="{}"/>"#, layer);
        }
        body.push_str("        <feMergeNode in=\"SourceGraphic\"/>\n      </feMerge>\n");
        "fxBase".to_string()
    };

    let soft_sigma = effects
        .soft_edge
        .as_ref()
        .map(|soft| soft.radius_emu as f32 / 2.0)
        .unwrap_or(0.0);

    if let Some(inner) = effects.inner_shadow.as_ref() {
        let sigma = inner.blur_radius_emu as f32 / 2.0;
        let (dx, dy) = directional_offset(inner.direction_deg, inner.distance_emu as f32);
        let _ = writeln!(
            body,
            r#"      <feOffset in="SourceAlpha" dx="{}" dy="{}" result="fxInnerOffset"/>"#,
            fmt_num(dx),
            fmt_num(dy)
        );
        body.push_str(
            "      <feComposite in=\"SourceAlpha\" in2=\"fxInnerOffset\" operator=\"out\" result=\"fxInnerInverse\"/>\n",
        );
        if sigma > 0.0 {
            let _ = writeln!(
                body,
                r#"      <feGaussianBlur in="fxInnerInverse" stdDeviation="{}" result="fxInnerBlur"/>"#,
                fmt_num(sigma)
            );
        }
        let blurred = if sigma > 0.0 { "fxInnerBlur" } else { "fxInnerInverse" };
        let [r, g, b, a] = inner.color;
        let _ = writeln!(
            body,
            r##"      <feFlood flood-color="#{:02x}{:02x}{:02x}" flood-opacity="{:.2}" result="fxInnerColor"/>"##,
            r,
            g,
            b,
            a as f32 / 255.0
        );
        let _ = writeln!(
            body,
            r#"      <feComposite in="fxInnerColor" in2="{}" operator="in" result="fxInnerTint"/>"#,
            blurred
        );
        body.push_str(
            "      <feComposite in=\"fxInnerTint\" in2=\"SourceAlpha\" operator=\"in\" result=\"fxInner\"/>\n",
        );
        let _ = writeln!(body, r#"      <feMerge result="fxWithInner"><feMergeNode in="{}"/><feMergeNode in="fxInner"/></feMerge>"#, current);
        if soft_sigma > 0.0 {
            current = "fxWithInner".to_string();
        }
    }

    if soft_sigma > 0.0 {
        let _ = writeln!(
            body,
            r#"      <feGaussianBlur in="SourceAlpha" stdDeviation="{}" result="fxSoftMask"/>"#,
            fmt_num(soft_sigma)
        );
        let _ = writeln!(
            body,
            r#"      <feComposite in="{}" in2="fxSoftMask" operator="in" result="fxSoft"/>"#,
            current
        );
    }

    if body.is_empty() {
        return None;
    }

    let _ = writeln!(
        defs,
        r#"    <filter id="{}" x="-50%" y="-50%" width="200%" height="200%" color-interpolation-filters="sRGB">"#,
        filter_id
    );
    defs.push_str(&body);
    defs.push_str("    </filter>\n");
    Some(filter_id)
}

fn emit_reflection_defs(
    mask_id: &str,
    grad_id: &str,
    reflection: &crate::contracts::VectorReflection,
    defs: &mut String,
) {
    let _ = writeln!(
        defs,
        r#"    <linearGradient id="{}" x1="0" y1="0" x2="0" y2="1">"#,
        grad_id
    );
    let _ = writeln!(
        defs,
        r##"      <stop offset="0%" stop-color="#ffffff" stop-opacity="{:.2}"/>"##,
        reflection.start_opacity.clamp(0.0, 1.0)
    );
    let _ = writeln!(
        defs,
        r##"      <stop offset="100%" stop-color="#ffffff" stop-opacity="{:.2}"/>"##,
        reflection.end_opacity.clamp(0.0, 1.0)
    );
    defs.push_str("    </linearGradient>\n");
    let _ = writeln!(defs, r#"    <mask id="{}" maskUnits="objectBoundingBox">"#, mask_id);
    let _ = writeln!(
        defs,
        r##"      <rect x="0" y="0" width="1" height="1" fill="url(#{})"/>"##,
        grad_id
    );
    defs.push_str("    </mask>\n");
}

/// SVG transform attribute for a path, including the 3D camera projection.
pub fn format_path_transform(
    path: &VectorPath,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    slide_width_emu: i64,
) -> String {
    let effective = projected_transform(path, transform, effects, slide_width_emu);
    crate::svg::path::format_transform_attr(&effective)
}

/// Affine approximation of the scene camera, applied in the path's local space.
fn projected_transform(
    path: &VectorPath,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    slide_width_emu: i64,
) -> AffineTransform {
    let Some(scene) = effects.scene_3d.as_ref() else {
        return *transform;
    };
    let Some((min_x, min_y, max_x, max_y)) = local_bounds(path) else {
        return *transform;
    };
    let width = (max_x - min_x).max(1.0);
    let height = (max_y - min_y).max(1.0);
    let camera = scene3d::camera_affine(scene, width, height, slide_width_emu);
    let adjusted = AffineTransform {
        tx: camera.tx + min_x - (camera.sx * min_x + camera.kx * min_y),
        ty: camera.ty + min_y - (camera.ky * min_x + camera.sy * min_y),
        ..camera
    };
    transform.post_concat(&adjusted)
}

fn local_bounds(path: &VectorPath) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    let mut add = |x: f32, y: f32| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for seg in &path.segments {
        match seg {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                add(*x, *y);
                any = true;
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                add(*x1, *y1);
                add(*x, *y);
                any = true;
            }
            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                add(*x1, *y1);
                add(*x2, *y2);
                add(*x, *y);
                any = true;
            }
            PathSegment::Close => {}
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

fn path_bounds(path: &VectorPath, transform: &AffineTransform) -> Option<(f32, f32, f32, f32)> {
    let bounds = local_bounds(path)?;
    let p0 = transform.transform_point(bounds.0, bounds.1);
    let p1 = transform.transform_point(bounds.2, bounds.1);
    let p2 = transform.transform_point(bounds.0, bounds.3);
    let p3 = transform.transform_point(bounds.2, bounds.3);
    let vals = [p0, p1, p2, p3];
    let min_x = vals.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let min_y = vals.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_x = vals.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let max_y = vals.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    Some((min_x, min_y, max_x, max_y))
}

fn directional_offset(direction_deg: f64, distance: f32) -> (f32, f32) {
    let rad = direction_deg.to_radians();
    (distance * rad.cos() as f32, distance * rad.sin() as f32)
}

fn fmt_num(v: f32) -> String {
    if !v.is_finite() {
        "0".to_string()
    } else if (v - v.round()).abs() < 1e-3 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}
