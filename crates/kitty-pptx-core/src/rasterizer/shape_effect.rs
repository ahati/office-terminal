//! Multi-pass shape-effect compositing for the `tiny-skia` backend (PRD 08 §4).
//!
//! A path carrying [`ShapeEffects`] is rendered into a scratch layer so the
//! silhouette alpha can drive shadow, glow, soft-edge, reflection, and inner
//! shadow passes. Passes composite in the DrawingML order from PRD 08 §1.1:
//! reflection → outer shadow → glow → extrusion/bevel → fill/stroke → inner
//! shadow → soft edges.

use crate::contracts::{
    AffineTransform, LineCap, LineJoin, ShapeEffects, VectorInnerShadow, VectorOuterShadow,
    VectorPaint, VectorPath, VectorShape3d, VectorStroke,
};
use crate::rasterizer::bevel::apply_bevel_shading;
use crate::rasterizer::filters;
use crate::rasterizer::mask::{
    alpha_mask, pixmap_from_premultiplied, shift_mask, tint_mask, warp_mask,
};
use crate::rasterizer::path;
use crate::scene3d;

/// Safety cap for authored blur radii (PRD 08 §6.1).
pub const MAX_BLUR_PX: f32 = 64.0;
/// Threshold used by the soft-edge erosion remap (PRD 08 §3.4).
const SOFT_EDGE_THRESHOLD: f32 = 0.1587;
/// Upper bound on scratch-layer pixels to keep memory pressure predictable.
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
        path::render_path(pixmap, path, fill, stroke, &effective, scale, offset_x, offset_y);
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
    let shape_h_px = (max_px.1 - min_px.1).max(1.0);

    let padding = effect_padding(effects, scale, shape_h_px);
    let canvas_w = pixmap.width() as f32;
    let canvas_h = pixmap.height() as f32;
    let x0 = (min_px.0 - padding.0).floor().max(0.0) as i32;
    let y0 = (min_px.1 - padding.1).floor().max(0.0) as i32;
    let x1 = (max_px.0 + padding.2).ceil().min(canvas_w) as i32;
    let y1 = (max_px.1 + padding.3).ceil().min(canvas_h) as i32;
    if x1 <= x0 || y1 <= y0 {
        path::render_path(pixmap, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    }

    let layer_w = (x1 - x0) as usize;
    let layer_h = (y1 - y0) as usize;
    if layer_w * layer_h > MAX_LAYER_PIXELS {
        // PRD 08 §7.2: degrade gracefully rather than exhausting memory.
        path::render_path(pixmap, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    }
    let Some(mut layer) = tiny_skia::Pixmap::new(layer_w as u32, layer_h as u32) else {
        path::render_path(pixmap, path, fill, stroke, &effective, scale, offset_x, offset_y);
        return;
    };

    draw_shape_layer(
        &mut layer,
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
    let layer_origin = (x0 as f32, y0 as f32);

    if let Some(reflection) = effects.reflection.as_ref() {
        composite_reflection(
            pixmap,
            &layer,
            reflection,
            scale,
            min_px,
            max_px,
            layer_origin,
            shape_h_px,
        );
    }
    if let Some(shadow) = effects.outer_shadow.as_ref() {
        composite_outer_shadow(pixmap, &base_mask, layer_w, layer_h, shadow, scale, layer_origin);
    }
    if let Some(glow) = effects.glow.as_ref() {
        composite_glow(pixmap, &base_mask, layer_w, layer_h, glow, scale, layer_origin);
    }

    // Inner shadow is composited inside the layer so the following soft-edge
    // pass feathers it together with the shape silhouette (PRD 08 §1.1).
    if let Some(inner) = effects.inner_shadow.as_ref() {
        composite_inner_shadow(&mut layer, &base_mask, layer_w, layer_h, inner, scale, (0.0, 0.0));
    }

    if let Some(soft) = effects.soft_edge.as_ref() {
        let radius_px = clamp_radius_px(soft.radius_emu, scale);
        if radius_px > 0.0 {
            apply_soft_edge(&mut layer, &base_mask, layer_w, layer_h, radius_px);
        }
    }

    pixmap.draw_pixmap(
        x0,
        y0,
        layer.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_shape_layer(
    layer: &mut tiny_skia::Pixmap,
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
        draw_extrusion(layer, path, fill, shape_3d, transform, scale, offset_x, offset_y);
    }

    path::render_path(layer, path, fill, stroke, transform, scale, offset_x, offset_y);

    if let Some(shape_3d) = effects.shape_3d.as_ref() {
        draw_contour(layer, path, stroke, shape_3d, transform, scale, offset_x, offset_y);
        let width = layer.width() as usize;
        let height = layer.height() as usize;
        apply_bevel_shading(layer.data_mut(), width, height, shape_3d, effects, scale);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_extrusion(
    layer: &mut tiny_skia::Pixmap,
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
        path::render_path(layer, path, Some(&paint), None, transform, scale, offset_x, offset_y + dy);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_contour(
    layer: &mut tiny_skia::Pixmap,
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
    };
    path::render_path(layer, path, None, Some(&contour), transform, scale, offset_x, offset_y);
}


fn composite_outer_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    shadow: &VectorOuterShadow,
    scale: f32,
    origin: (f32, f32),
) {
    let radius_px = clamp_radius_px(shadow.blur_radius_emu, scale);
    let distance_px = shadow.distance_emu as f32 * scale;
    if radius_px <= 0.0 && distance_px <= 0.0 {
        return;
    }

    let mut mask = base_mask.to_vec();
    filters::gaussian_blur_alpha(&mut mask, width, height, radius_px / 2.0);

    if (shadow.scale_x - 1.0).abs() > 1e-4
        || (shadow.scale_y - 1.0).abs() > 1e-4
        || shadow.skew_x_deg.abs() > 1e-4
        || shadow.skew_y_deg.abs() > 1e-4
    {
        mask = warp_mask(&mask, width, height, shadow);
    }

    let (dx, dy) = filters::directional_offset(shadow.direction_deg, distance_px);
    if let Some(tinted) = tint_mask(&mask, width, height, shadow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0.round() as i32 + dx.round() as i32,
            origin.1.round() as i32 + dy.round() as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

fn composite_glow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    glow: &crate::contracts::VectorGlow,
    scale: f32,
    origin: (f32, f32),
) {
    let radius_px = clamp_radius_px(glow.radius_emu, scale);
    if radius_px <= 0.0 {
        return;
    }
    let mut mask = base_mask.to_vec();
    filters::gaussian_blur_alpha(&mut mask, width, height, radius_px / 2.5);

    // Perceptual non-linear falloff: A' = alpha * (1 - (1 - A)^2).
    let falloff: Vec<u8> = mask
        .iter()
        .map(|&a| {
            let norm = a as f32 / 255.0;
            let expanded = 1.0 - (1.0 - norm) * (1.0 - norm);
            (expanded * 255.0).round() as u8
        })
        .collect();

    if let Some(tinted) = tint_mask(&falloff, width, height, glow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0.round() as i32,
            origin.1.round() as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

fn composite_inner_shadow(
    pixmap: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    shadow: &VectorInnerShadow,
    scale: f32,
    origin: (f32, f32),
) {
    let radius_px = clamp_radius_px(shadow.blur_radius_emu, scale);
    let distance_px = shadow.distance_emu as f32 * scale;
    if radius_px <= 0.0 && distance_px <= 0.0 {
        return;
    }

    let (dx, dy) = filters::directional_offset(shadow.direction_deg, distance_px);
    let inverted: Vec<u8> = base_mask.iter().map(|&a| 255 - a).collect();
    let shifted = shift_mask(&inverted, width, height, dx.round() as i32, dy.round() as i32);
    let mut blurred = shifted;
    filters::gaussian_blur_alpha(&mut blurred, width, height, radius_px / 2.0);

    // Clip the blurred inverted mask to the shape interior.
    let clipped: Vec<u8> = blurred
        .iter()
        .zip(base_mask.iter())
        .map(|(&b, &a)| ((b as u32 * a as u32 + 127) / 255) as u8)
        .collect();

    if let Some(tinted) = tint_mask(&clipped, width, height, shadow.color, |a| a) {
        pixmap.draw_pixmap(
            origin.0.round() as i32,
            origin.1.round() as i32,
            tinted.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn composite_reflection(
    pixmap: &mut tiny_skia::Pixmap,
    layer: &tiny_skia::Pixmap,
    reflection: &crate::contracts::VectorReflection,
    scale: f32,
    min_px: (f32, f32),
    max_px: (f32, f32),
    origin: (f32, f32),
    shape_h_px: f32,
) {
    if reflection.start_opacity <= 0.0 {
        return;
    }
    let lw = layer.width() as usize;
    let lh = layer.height() as usize;
    let cx0 = (min_px.0 - origin.0).floor().clamp(0.0, lw as f32) as usize;
    let cx1 = (max_px.0 - origin.0).ceil().clamp(0.0, lw as f32) as usize;
    let cy0 = (min_px.1 - origin.1).floor().clamp(0.0, lh as f32) as usize;
    let cy1 = (max_px.1 - origin.1).ceil().clamp(0.0, lh as f32) as usize;
    let (tw, th) = (cx1.saturating_sub(cx0), cy1.saturating_sub(cy0));
    if tw == 0 || th == 0 {
        return;
    }

    let src = layer.data();
    let mut tile = vec![0u8; tw * th * 4];
    for row in 0..th {
        let src_row = cy1 - 1 - row; // vertical mirror
        let src_start = (src_row * lw + cx0) * 4;
        let dst_start = row * tw * 4;
        tile[dst_start..dst_start + tw * 4].copy_from_slice(&src[src_start..src_start + tw * 4]);
    }

    let radius_px = clamp_radius_px(reflection.blur_radius_emu, scale);
    if radius_px > 0.0 {
        filters::gaussian_blur_premultiplied(&mut tile, tw, th, radius_px / 2.0);
    }

    // Linear opacity ramp from the shape boundary down the reflection.
    for row in 0..th {
        let t = (row as f32 / shape_h_px.max(1.0)).clamp(0.0, 1.0);
        let alpha = reflection.start_opacity
            + (reflection.end_opacity - reflection.start_opacity) * t;
        let factor = alpha.clamp(0.0, 1.0);
        for x in 0..tw {
            let base = (row * tw + x) * 4;
            for channel in 0..4 {
                tile[base + channel] = (tile[base + channel] as f32 * factor).round() as u8;
            }
        }
    }

    let Some(tile_pixmap) = pixmap_from_premultiplied(&tile, tw, th) else {
        return;
    };
    let distance_px = reflection.distance_emu as f32 * scale;
    let dest_x = origin.0.round() as i32 + cx0 as i32;
    let dest_y = (max_px.1 + distance_px).round() as i32;
    pixmap.draw_pixmap(dest_x, dest_y, tile_pixmap.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
}

/// Feather the shape layer's own alpha (PRD 08 §3.4).
fn apply_soft_edge(
    layer: &mut tiny_skia::Pixmap,
    base_mask: &[u8],
    width: usize,
    height: usize,
    radius_px: f32,
) {
    let mut softened = base_mask.to_vec();
    filters::gaussian_blur_alpha(&mut softened, width, height, radius_px / 2.0);
    let span = (1.0 - SOFT_EDGE_THRESHOLD).max(1e-4);
    for value in &mut softened {
        let norm = *value as f32 / 255.0;
        let remapped = ((norm - SOFT_EDGE_THRESHOLD) / span).clamp(0.0, 1.0);
        *value = (remapped * 255.0).round() as u8;
    }

    let data = layer.data_mut();
    for i in 0..width * height {
        let original = base_mask[i] as f32;
        if original <= 0.0 {
            continue;
        }
        let ratio = softened[i] as f32 / original;
        for channel in 0..4 {
            data[i * 4 + channel] = (data[i * 4 + channel] as f32 * ratio)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

fn clamp_radius_px(radius_emu: i64, scale: f32) -> f32 {
    (radius_emu as f32 * scale).clamp(0.0, MAX_BLUR_PX)
}

fn emu_px(emu: i64, scale: f32) -> f32 {
    emu as f32 * scale
}

fn effect_padding(effects: &ShapeEffects, scale: f32, shape_h_px: f32) -> (f32, f32, f32, f32) {
    let mut left = 0.0f32;
    let mut top = 0.0f32;
    let mut right = 0.0f32;
    let mut bottom = 0.0f32;
    if let Some(shadow) = effects.outer_shadow.as_ref() {
        let pad = clamp_radius_px(shadow.blur_radius_emu, scale)
            + emu_px(shadow.distance_emu, scale).abs()
            + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(glow) = effects.glow.as_ref() {
        let pad = clamp_radius_px(glow.radius_emu, scale) + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(soft) = effects.soft_edge.as_ref() {
        // The feather needs transparent pixels around the silhouette so the
        // blur can erode inwards (PRD 08 §3.4).
        let pad = clamp_radius_px(soft.radius_emu, scale) + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(inner) = effects.inner_shadow.as_ref() {
        let pad = clamp_radius_px(inner.blur_radius_emu, scale)
            + emu_px(inner.distance_emu, scale).abs()
            + 2.0;
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
    }
    if let Some(reflection) = effects.reflection.as_ref() {
        bottom = bottom.max(emu_px(reflection.distance_emu, scale) + shape_h_px + 2.0);
    }
    if let Some(shape_3d) = effects.shape_3d.as_ref() {
        let bevel = (shape_3d.bevel_top_width_emu.max(shape_3d.bevel_top_height_emu) as f32
            * scale)
            .clamp(0.0, 48.0)
            + 2.0;
        let contour = emu_px(shape_3d.contour_width_emu, scale).clamp(0.0, 24.0) + 1.0;
        let pad = bevel.max(contour);
        left = left.max(pad);
        top = top.max(pad);
        right = right.max(pad);
        bottom = bottom.max(pad);
        bottom = bottom.max(emu_px(shape_3d.extrusion_depth_emu, scale).clamp(0.0, 120.0) + 2.0);
    }
    (left, top, right, bottom)
}
