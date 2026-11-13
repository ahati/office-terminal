//! Multi-pass shape-effect compositing for the `tiny-skia` backend (PRD 08 §4).
//!
//! A path carrying [`ShapeEffects`] is rendered into a scratch layer so the
//! silhouette alpha can drive the outer shadow, reflection, glow, soft-edge and
//! inner-shadow passes. The pass order mirrors the reference dispatcher
//! (ref packages/pptx/src/renderer.ts:3789-3860 `paintWithRasterEffects`):
//! outer shadow → reflection → glow → body (softEdge REPLACES the body) →
//! inner shadow. Everything except the body itself is painted behind it.

use crate::contracts::{
    AffineTransform, LineCap, LineJoin, ShapeEffects, VectorInnerShadow, VectorOuterShadow,
    VectorPaint, VectorPath, VectorShape3d, VectorStroke,
};
use crate::rasterizer::bevel::apply_bevel_shading;
use crate::rasterizer::filters;
use crate::rasterizer::mask::{alpha_mask, tint_mask};
use crate::rasterizer::path;
use crate::scene3d;

mod reflection;

/// Upper bound on scratch-layer pixels to keep memory pressure predictable.
/// This is the only blur guard kept: effect reaches are capped by the canvas
/// diagonal instead of a fixed constant (the reference has no fixed cap either).
const MAX_LAYER_PIXELS: usize = 16_000_000;

/// Resolve a path's effective transform, applying the 3D scene camera
/// projection (affine approximation, PRD 08 §3.6.1) when present.
pub fn projected_transform(
    path: &VectorPath,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    slide_width_emu: i64,
) -> AffineTransform {
    let Some(scene) = effects.scene_3d.as_ref() else {
        return *transform;
    };
    let Some((min_x, min_y, max_x, max_y)) = path::local_bounds(path) else {
        return *transform;
    };
    let width = (max_x - min_x).max(1.0);
    let height = (max_y - min_y).max(1.0);
    let camera = scene3d::camera_affine(scene, width, height, slide_width_emu);

    // `camera_affine` maps a 0-based box; rebase it onto the path's own local
    // origin so presets with non-zero path bounds still project correctly.
    let adjusted = AffineTransform {
        tx: camera.tx + min_x - (camera.sx * min_x + camera.kx * min_y),
        ty: camera.ty + min_y - (camera.ky * min_x + camera.sy * min_y),
        ..camera
    };
    transform.post_concat(&adjusted)
}

/// Render a path, executing every attached effect pass.
#[allow(clippy::too_many_arguments)]
pub fn render_path_with_effects(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    slide_width_emu: i64,
) {
    if path.segments.is_empty() {
        return;
    }
    let effective = projected_transform(path, transform, effects, slide_width_emu);

    if effects.is_empty() {
        path::render_path(pixmap, assets, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    }

    let Some(bounds) = path::transformed_bounds(path, &effective) else {
        return;
    };
    let min_px = (offset_x + bounds.0 * scale, offset_y + bounds.1 * scale);
    let max_px = (offset_x + bounds.2 * scale, offset_y + bounds.3 * scale);
    if !(min_px.0.is_finite() && max_px.1.is_finite()) {
        return;
    }
    let canvas_w = pixmap.width() as f32;
    let canvas_h = pixmap.height() as f32;
    let reach_cap = (canvas_w * canvas_w + canvas_h * canvas_h).sqrt();

    // ref renderer.ts:4385-4400 — effect bounds start from the painted geometry
    // grown by the stroke half width / contour extent so outlines are never
    // clipped by the effect crop.
    let painted_pad = painted_padding(stroke, effects, scale) + 2.0;
    let shape_box = (min_px.0, min_px.1, max_px.0 - min_px.0, max_px.1 - min_px.1);
    let padding = effect_padding(effects, scale, shape_box, reach_cap);
    let x0 = (min_px.0 - padding.0 - painted_pad).floor().max(0.0) as i32;
    let y0 = (min_px.1 - padding.1 - painted_pad).floor().max(0.0) as i32;
    let x1 = (max_px.0 + padding.2 + painted_pad).ceil().min(canvas_w) as i32;
    let y1 = (max_px.1 + padding.3 + painted_pad).ceil().min(canvas_h) as i32;
    if x1 <= x0 || y1 <= y0 {
        path::render_path(pixmap, assets, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    }

    let layer_w = (x1 - x0) as usize;
    let layer_h = (y1 - y0) as usize;
    if layer_w * layer_h > MAX_LAYER_PIXELS {
        // PRD 08 §7.2: degrade gracefully rather than exhausting memory.
        path::render_path(pixmap, assets, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    }
    let Some(mut layer) = tiny_skia::Pixmap::new(layer_w as u32, layer_h as u32) else {
        path::render_path(pixmap, assets, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    };

    draw_shape_layer(
        &mut layer,
        assets,
        path,
        fill,
        stroke,
        &effective,
        effects,
        scale,
        offset_x - x0 as f32,
        offset_y - y0 as f32,
    );

    let base_mask = alpha_mask(layer.data());
    let origin = (x0 as f32, y0 as f32);
    // The painted body box in layer-local pixels.
    let bbox = (
        min_px.0 - origin.0,
        min_px.1 - origin.1,
        max_px.0 - min_px.0,
        max_px.1 - min_px.1,
    );
    // ref renderer.ts:3806 — shadow rotations follow the live transform.
    let rotation_deg = effective.ky.atan2(effective.sx).to_degrees();

    // 1. Outer shadow (ref renderer.ts:3794-3810): painted first, bottom-most.
    if let Some(shadow) = effects.outer_shadow.as_ref() {
        composite_outer_shadow(
            pixmap,
            &base_mask,
            layer_w,
            layer_h,
            shadow,
            scale,
            reach_cap,
            origin,
            bbox,
            rotation_deg,
        );
    }
    // 2. Reflection (ref renderer.ts:3811-3817): above the shadow.
    if let Some(reflection) = effects.reflection.as_ref() {
        reflection::composite(pixmap, &layer, reflection, scale, reach_cap, origin, bbox);
    }
    // 3. Glow (ref renderer.ts:3818-3819): still behind the body.
    if let Some(glow) = effects.glow.as_ref() {
        composite_glow(pixmap, &base_mask, layer_w, layer_h, glow, scale, reach_cap, origin);
    }

    // 4. Body — `softEdge` replaces the body paint instead of layering on it.
    if let Some(soft) = effects.soft_edge.as_ref() {
        apply_soft_edge(&mut layer, &base_mask, layer_w, layer_h, soft, scale, reach_cap, bbox);
    }
    pixmap.draw_pixmap(
        x0,
        y0,
        layer.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    // 5. Inner shadow (ref renderer.ts:3838-3848): painted over the body,
    // clipped to its interior.
    if let Some(inner) = effects.inner_shadow.as_ref() {
        composite_inner_shadow(pixmap, &base_mask, layer_w, layer_h, inner, scale, reach_cap, origin);
    }
}

/// Painted extent outside the geometry: stroke half width, contour width
/// (ref renderer.ts:4361-4389 `paintedPad`). Line-end extents are not in the IR.
fn painted_padding(stroke: Option<&VectorStroke>, effects: &ShapeEffects, scale: f32) -> f32 {
    let stroke_half = stroke
        .map(|s| (s.width_emu as f32 * scale).max(0.5) / 2.0)
        .unwrap_or(0.0);
    let contour = effects
        .shape_3d
        .as_ref()
        .map(|s| s.contour_width_emu as f32 * scale)
        .unwrap_or(0.0);
    stroke_half.max(contour)
}

#[allow(clippy::too_many_arguments)]
fn draw_shape_layer(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    if let Some(shape_3d) = effects.shape_3d.as_ref() {
        draw_extrusion(layer, assets, path, fill, shape_3d, transform, scale, offset_x, offset_y);
    }

    path::render_path(layer, assets, path, fill, stroke, transform, scale, offset_x, offset_y);

    if let Some(shape_3d) = effects.shape_3d.as_ref() {
        draw_contour(layer, assets, path, stroke, shape_3d, transform, scale, offset_x, offset_y);
        let width = layer.width() as usize;
        let height = layer.height() as usize;
        apply_bevel_shading(layer.data_mut(), width, height, shape_3d, effects, scale);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_extrusion(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    shape_3d: &VectorShape3d,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let depth_px = (shape_3d.extrusion_depth_emu as f32 * scale).clamp(0.0, 120.0);
    if depth_px <= 0.0 {
        return;
    }
    let color = shape_3d
        .extrusion_color
        .or(match fill {
            Some(VectorPaint::Solid([r, g, b, a])) => Some([
                (*r as f32 * 0.7) as u8,
                (*g as f32 * 0.7) as u8,
                (*b as f32 * 0.7) as u8,
                *a,
            ]),
            _ => None,
        })
        .unwrap_or([96, 96, 96, 255]);
    let paint = VectorPaint::Solid(color);
    let steps = ((depth_px / 2.0).ceil() as usize).clamp(1, 24);

    // Farthest silhouette first, nearest last: a layered planar extrusion.
    for step in (1..=steps).rev() {
        let dy = depth_px * step as f32 / steps as f32;
        path::render_path(
            layer,
            assets,
            path,
            Some(&paint),
            None,
            transform,
            scale,
            offset_x,
            offset_y + dy,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_contour(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    stroke: Option<&VectorStroke>,
    shape_3d: &VectorShape3d,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    if shape_3d.contour_width_emu <= 0 {
        return;
    }
    let color = shape_3d
        .contour_color
        .or_else(|| stroke.map(|s| s.color))
        .unwrap_or([180, 180, 180, 200]);
    let contour = VectorStroke {
        color,
        width_emu: shape_3d.contour_width_emu,
        dash_segments: None,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        paint: None,
    };
    path::render_path(
        layer,
        assets,
        path,
        None,
        Some(&contour),
        transform,
        scale,
        offset_x,
        offset_y,
    );
}

#[allow(clippy::too_many_arguments)]
fn composite_outer_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    shadow: &VectorOuterShadow,
    scale: f32,
    reach_cap: f32,
    origin: (f32, f32),
    bbox: (f32, f32, f32, f32),
    shape_rotation_deg: f32,
) {
    // ref effects.ts:applyOuterShadow — `ctx.filter='blur(r)'` takes the
    // Gaussian standard deviation, i.e. sigma = the authored blur radius.
    let sigma = filters::clamp_reach_px(shadow.blur_radius_emu, scale, reach_cap);
    let distance_px = shadow.distance_emu as f32 * scale;
    if sigma <= 0.0 && distance_px == 0.0 {
        return;
    }

    // rotWithShape defaults to true (§20.1.8.45). A false value keeps the
    // effect in page space while the silhouette stays rotated.
    let effect_rotation = if shadow.rotate_with_shape { shape_rotation_deg } else { 0.0 };
    let (dx, dy) =
        filters::directional_offset(shadow.direction_deg + effect_rotation as f64, distance_px);

    let mut mask = base_mask.to_vec();
    filters::gaussian_blur_alpha(&mut mask, width, height, sigma);
    // ref effects.ts:shadowAlignmentPoint — `algn` is not in the IR, so the
    // spec default `b` (bottom centre of the shape box) anchors the transform.
    let anchor = (bbox.0 + bbox.2 * 0.5, bbox.1 + bbox.3);
    let mask = filters::warp_shadow_mask(
        &mask,
        width,
        height,
        shadow,
        anchor,
        effect_rotation,
        (dx, dy),
    );

    if let Some(tinted) = tint_mask(&mask, width, height, shadow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0 as i32,
            origin.1 as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn composite_glow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    glow: &crate::contracts::VectorGlow,
    scale: f32,
    reach_cap: f32,
    origin: (f32, f32),
) {
    let radius_px = filters::clamp_reach_px(glow.radius_emu, scale, reach_cap);
    if radius_px <= 0.0 {
        return;
    }
    // ref renderer.ts:2701 `applyGlow` — canvas `shadowBlur = rad` blurs with
    // sigma = rad / 2, and there is no additional intensity curve.
    let mut mask = base_mask.to_vec();
    filters::gaussian_blur_alpha(&mut mask, width, height, radius_px / 2.0);

    if let Some(tinted) = tint_mask(&mask, width, height, glow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0 as i32,
            origin.1 as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn composite_inner_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    shadow: &VectorInnerShadow,
    scale: f32,
    reach_cap: f32,
    origin: (f32, f32),
) {
    let sigma = filters::clamp_reach_px(shadow.blur_radius_emu, scale, reach_cap);
    let distance_px = shadow.distance_emu as f32 * scale;
    if sigma <= 0.0 && distance_px == 0.0 {
        return;
    }

    let (dx, dy) = filters::directional_offset(shadow.direction_deg, distance_px);
    let inverted: Vec<u8> = base_mask.iter().map(|&a| 255 - a).collect();
    // ref effects.ts:applyInnerShadow — offset the mask, blur it, then clip the
    // surviving crescent back to the un-offset interior.
    let shifted = filters::shift_mask_subpixel(&inverted, width, height, dx, dy);
    let mut blurred = shifted;
    filters::gaussian_blur_alpha(&mut blurred, width, height, sigma);

    let clipped: Vec<u8> = blurred
        .iter()
        .zip(base_mask.iter())
        .map(|(&b, &a)| ((b as u32 * a as u32 + 127) / 255) as u8)
        .collect();

    if let Some(tinted) = tint_mask(&clipped, width, height, shadow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0 as i32,
            origin.1 as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

/// Feather the body symmetrically about the geometry
/// (ref effects.ts:applySoftEdge, sigma = radius / 3, outward bleed of radius).
#[allow(clippy::too_many_arguments)]
fn apply_soft_edge(
    layer: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    soft: &crate::contracts::VectorSoftEdge,
    scale: f32,
    reach_cap: f32,
    bbox: (f32, f32, f32, f32),
) {
    let radius_px = filters::clamp_reach_px(soft.radius_emu, scale, reach_cap);
    if radius_px <= 0.0 {
        return;
    }
    filters::feather_alpha_bleed(layer.data_mut(), width, height, base_mask, bbox, radius_px);
}

fn effect_padding(
    effects: &ShapeEffects,
    scale: f32,
    shape_box: (f32, f32, f32, f32),
    reach_cap: f32,
) -> (f32, f32, f32, f32) {
    let mut left = 0.0f32;
    let mut top = 0.0f32;
    let mut right = 0.0f32;
    let mut bottom = 0.0f32;
    if let Some(shadow) = effects.outer_shadow.as_ref() {
        // ref effects.ts:margin — three blur radii plus the offset reach.
        let pad = 3.0 * filters::clamp_reach_px(shadow.blur_radius_emu, scale, reach_cap)
            + (shadow.distance_emu as f32 * scale).abs()
            + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(glow) = effects.glow.as_ref() {
        // sigma = rad / 2, so 3σ = 1.5·rad.
        let pad = 1.5 * filters::clamp_reach_px(glow.radius_emu, scale, reach_cap) + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(soft) = effects.soft_edge.as_ref() {
        // The outward colour bleed reaches exactly `rad`.
        let pad = filters::clamp_reach_px(soft.radius_emu, scale, reach_cap) + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(inner) = effects.inner_shadow.as_ref() {
        // ref effects.ts:innerShadowMargin — |dist| + 3·blur, +2 px guard.
        let pad = 3.0 * filters::clamp_reach_px(inner.blur_radius_emu, scale, reach_cap)
            + (inner.distance_emu as f32 * scale).abs()
            + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(reflection) = effects.reflection.as_ref() {
        // The mirrored copy can leave the box on any side once `dir`/`dist`
        // point elsewhere (ref effects.ts:626-639).
        let blur = filters::clamp_reach_px(reflection.blur_radius_emu, scale, reach_cap);
        let (l, t, r, b) = reflection::padding(reflection, shape_box, scale, blur);
        left = left.max(l);
        top = top.max(t);
        right = right.max(r);
        bottom = bottom.max(b);
    }
    if let Some(shape_3d) = effects.shape_3d.as_ref() {
        let bevel = (shape_3d.bevel_top_width_emu.max(shape_3d.bevel_top_height_emu) as f32
            * scale)
            .clamp(0.0, 48.0)
            + 2.0;
        let contour = (shape_3d.contour_width_emu as f32 * scale).clamp(0.0, 24.0) + 1.0;
        let pad = bevel.max(contour);
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
        bottom = bottom
            .max((shape_3d.extrusion_depth_emu as f32 * scale).clamp(0.0, 120.0) + 2.0);
    }
    (left, top, right, bottom)
}
