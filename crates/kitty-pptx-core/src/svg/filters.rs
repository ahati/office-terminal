//! SVG `<filter>` / `<mask>` graph emission for shape effects (PRD 08 §5).
//!
//! The serializer stays purely declarative: shadows, glows, inner shadows, and
//! soft edges become standard W3C filter primitives; reflections become a
//! mirrored `<use>` clone attenuated by a linear-gradient mask. Every id is
//! derived from the draw-command index, so exports are deterministic.

use std::fmt::Write;

use crate::contracts::{
    AffineTransform, PathSegment, ShapeEffects, VectorPath, VectorReflection,
};
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
            // The raster pass skips only when both ends of the band are fully
            // transparent (ref effects.ts:598-624).
            let ink = reflection.start_opacity.clamp(0.0, 1.0) > 0.0
                || reflection.end_opacity.clamp(0.0, 1.0) > 0.0;
            if ink {
                let mask_id = format!("refl-mask-{index}");
                let grad_id = format!("refl-grad-{index}");
                let mirror = mirror_geometry(reflection, bounds);
                let region = reflection_region(reflection, bounds, &mirror);
                emit_reflection_defs(index, &mask_id, &grad_id, reflection, &mirror, region, defs);
                let blur_attr = if reflection.blur_radius_emu > 0 {
                    format!(r#" filter="url(#refl-blur-{index})""#)
                } else {
                    String::new()
                };
                // The mirror lives in a plain `<g>` transform: SVG filter
                // primitives cannot rotate/reflect a graphic. `dist` is the
                // outer `<feOffset>` along `dir` (ref effects.ts:626-639).
                refs.reflection_use = Some(format!(
                    r##"<g filter="url(#refl-fx-{index})"><g {}><use href="#{}" mask="url(#{})"{}/></g></g>"##,
                    mirror_transform_attr(&mirror),
                    refs.path_id,
                    mask_id,
                    blur_attr
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

/// Authored reflection geometry in slide (EMU) space.
///
/// Port of `rasterizer/shape_effect/reflection.rs::mirror_matrix`
/// (ref packages/core/src/shape/effects.ts:626-639): the mirrored copy is
/// `R(dir - 90°) · diag(sx, -sy)` about the `algn` pivot, pushed `dist` along
/// `dir`. The contract has no skew terms (OOXML `kx`/`ky` are dropped at parse
/// time), so every combination is expressible and no vertical fallback exists.
pub(crate) struct MirrorGeometry {
    pivot: (f32, f32),
    rotate_deg: f32,
    scale_x: f32,
    scale_y: f32,
    offset: (f32, f32),
}

pub(crate) fn mirror_geometry(
    reflection: &VectorReflection,
    bounds: (f32, f32, f32, f32),
) -> MirrorGeometry {
    let pivot = align_point(bounds, &reflection.align);
    let dir = if reflection.direction_deg.is_finite() {
        reflection.direction_deg
    } else {
        90.0
    };
    let offset = directional_offset(dir, reflection.distance_emu as f32);
    MirrorGeometry {
        pivot,
        rotate_deg: dir as f32 - 90.0,
        scale_x: scale_magnitude(reflection.scale_x),
        scale_y: scale_magnitude(reflection.scale_y),
        offset,
    }
}

/// `algn` pivot (`ST_RectAlignment`), defaulting to the bottom centre.
/// ref effects.ts:98-110 `shadowAlignmentPoint`.
fn align_point(bounds: (f32, f32, f32, f32), align: &str) -> (f32, f32) {
    let (x, y, max_x, max_y) = bounds;
    let w = max_x - x;
    let h = max_y - y;
    let ax = match align {
        "tl" | "l" | "bl" => x,
        "tr" | "r" | "br" => max_x,
        _ => x + w * 0.5,
    };
    let ay = match align {
        "tl" | "t" | "tr" => y,
        "l" | "ctr" | "r" => y + h * 0.5,
        _ => max_y,
    };
    (ax, ay)
}

/// Authored `sx`/`sy` as a positive magnitude; ECMA defaults `sy` to −100 %.
fn scale_magnitude(v: f32) -> f32 {
    if v.is_finite() {
        v.abs().clamp(1e-3, 64.0)
    } else {
        1.0
    }
}

pub(crate) fn mirror_transform_attr(m: &MirrorGeometry) -> String {
    format!(
        r#"transform="translate({}, {}) rotate({}) scale({}, {}) translate({}, {})""#,
        fmt_num(m.pivot.0),
        fmt_num(m.pivot.1),
        fmt_num(m.rotate_deg),
        fmt_num(m.scale_x),
        fmt_num(-m.scale_y),
        fmt_num(-m.pivot.0),
        fmt_num(-m.pivot.1)
    )
}

/// Filter region covering the mirrored copy, its `dist` shift, and the 3σ blur
/// tail. `filterUnits="userSpaceOnUse"` keeps the numbers in slide EMU.
pub(crate) fn reflection_region(
    reflection: &VectorReflection,
    bounds: (f32, f32, f32, f32),
    m: &MirrorGeometry,
) -> (f32, f32, f32, f32) {
    let (sin, cos) = m.rotate_deg.to_radians().sin_cos();
    let a = cos * m.scale_x;
    let b = sin * m.scale_x;
    let c = sin * m.scale_y;
    let d = -cos * m.scale_y;
    let (px, py) = m.pivot;
    let tx = px - (a * px + c * py);
    let ty = py - (b * px + d * py);
    let (x, y, max_x, max_y) = bounds;
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    for (cx, cy) in [(x, y), (max_x, y), (max_x, max_y), (x, max_y)] {
        let (mx, my) = (a * cx + c * cy + tx, b * cx + d * cy + ty);
        // Union the pre- and post-`dist` boxes so the region grows with the
        // separation as well as with the mirror scale.
        for (ux, uy) in [(mx, my), (mx + m.offset.0, my + m.offset.1)] {
            min = (min.0.min(ux), min.1.min(uy));
            max = (max.0.max(ux), max.1.max(uy));
        }
    }
    let sigma = reflection.blur_radius_emu.max(0) as f32;
    let pad = 3.0 * sigma * m.scale_x.max(m.scale_y) + 2.0;
    (
        min.0 - pad,
        min.1 - pad,
        (max.0 - min.0 + 2.0 * pad).max(1.0),
        (max.1 - min.1 + 2.0 * pad).max(1.0),
    )
}

/// Alpha at band offset `t` (0 = the edge touching the shape, 1 = the far
/// edge). Rebuilds the reference stops verbatim (ref effects.ts:612-622): `stA`
/// holds up to `stPos`, a linear ramp reaches `endA` at `endPos`, then holds.
fn band_alpha(t: f32, r: &VectorReflection) -> f32 {
    let st_a = r.start_opacity.clamp(0.0, 1.0);
    let end_a = r.end_opacity.clamp(0.0, 1.0);
    let st_pos = r.start_pos.clamp(0.0, 1.0);
    let t = t.clamp(0.0, 1.0);
    if t <= st_pos {
        return st_a;
    }
    let stop = if r.end_pos > st_pos {
        r.end_pos.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if t >= stop {
        return end_a;
    }
    st_a + (end_a - st_a) * (t - st_pos) / (stop - st_pos).max(1e-6)
}

/// `feFuncA` table sampling the authored band at 1 % steps. The gradient below
/// encodes the band position as alpha 0→1, so the table turns that position
/// ramp into `stA…endA` with the `stPos`/`endPos` plateaus.
fn ramp_table_values(r: &VectorReflection) -> String {
    let mut out = String::new();
    for i in 0..=100 {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&fmt_alpha(band_alpha(i as f32 / 100.0, r)));
    }
    out
}

pub(crate) fn emit_reflection_defs(
    index: usize,
    mask_id: &str,
    grad_id: &str,
    reflection: &VectorReflection,
    mirror: &MirrorGeometry,
    region: (f32, f32, f32, f32),
    defs: &mut String,
) {
    // `dist` along `dir`: the offset runs in slide space (outside the mirror
    // transform), so its dx/dy are the rotated direction components.
    let _ = writeln!(
        defs,
        r#"    <filter id="refl-fx-{index}" filterUnits="userSpaceOnUse" x="{}" y="{}" width="{}" height="{}" color-interpolation-filters="sRGB">"#,
        fmt_num(region.0),
        fmt_num(region.1),
        fmt_num(region.2),
        fmt_num(region.3)
    );
    let _ = writeln!(
        defs,
        r#"      <feOffset in="SourceGraphic" dx="{}" dy="{}" result="reflOffset"/>"#,
        fmt_num(mirror.offset.0),
        fmt_num(mirror.offset.1)
    );
    defs.push_str("    </filter>\n");

    // Blur runs on the source side of the mirror group so `sx`/`sy` scale it
    // exactly like the raster blit; the percentage region leaves room for a
    // blur larger than the shape itself.
    let sigma = reflection.blur_radius_emu.max(0) as f32;
    if sigma > 0.0 {
        let _ = writeln!(
            defs,
            r#"    <filter id="refl-blur-{index}" x="-200%" y="-200%" width="500%" height="500%" color-interpolation-filters="sRGB">
      <feGaussianBlur in="SourceGraphic" stdDeviation="{}" result="reflBlur"/>
    </filter>"#,
            fmt_num(sigma)
        );
    }

    let _ = writeln!(
        defs,
        r#"    <filter id="refl-ramp-{index}" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB">
      <feComponentTransfer in="SourceGraphic"><feFuncA type="table" tableValues="{}"/></feComponentTransfer>
    </filter>"#,
        ramp_table_values(reflection)
    );

    // Position ramp: alpha 0 at the source bottom (`t = 0`) to alpha 1 at the
    // top (`t = 1`). `userSpaceOnUse` keeps the ramp on the shape bbox while
    // the mask rect extends past it so blur can bleed; the table's end stops
    // clamp there (`stA` below, `endA` above). The mirror group flips the ramp
    // so `stA` touches the shape.
    let _ = writeln!(
        defs,
        r##"    <linearGradient id="{grad_id}" gradientUnits="userSpaceOnUse" x1="0" y1="1" x2="0" y2="0">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="1"/>
    </linearGradient>"##
    );
    let _ = writeln!(
        defs,
        r#"    <mask id="{mask_id}" maskUnits="objectBoundingBox" x="-1" y="-1" width="3" height="3" maskContentUnits="objectBoundingBox">
      <rect x="-1" y="-1" width="3" height="3" fill="url(#{grad_id})" filter="url(#refl-ramp-{index})"/>
    </mask>"#
    );
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

fn fmt_alpha(v: f32) -> String {
    let s = format!("{:.4}", v.clamp(0.0, 1.0));
    s.trim_end_matches('0').trim_end_matches('.').to_string()
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
