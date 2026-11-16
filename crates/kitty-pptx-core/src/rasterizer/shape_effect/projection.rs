//! Reference scene3d projection for the rasterizer.
//!
//! Port of `packages/pptx/src/renderer.ts:6400-6640` (`projectScene3dPaint`):
//! the authored face is painted into a shape-local, device-resolution buffer,
//! then warped onto the slide through the camera quad
//! (`computeScene3dQuad` + `unitSquareToQuad`, `core/src/shape/scene3d-*`).
//! Effect bounds and anchors come from `projectedEffectGeometry`
//! (`crate::scene3d::projection`).
//!
//! Perspective quads have no affine canvas transform, so the warp inverts the
//! unit-square homography and bilinearly samples the local buffer per
//! destination pixel (the reference's piecewise-affine mesh has the same
//! limit). Affine quads go through the same sampler, which keeps one code path
//! and the reference's single projection.

use super::extrusion::draw_projected_solid;
use super::{effect_padding, painted_padding, MAX_LAYER_PIXELS};
use crate::contracts::{AffineTransform, ShapeEffects, VectorPaint, VectorPath, VectorStroke};
use crate::scene3d::{self, CameraInput, EffectTransform, ProjectedSurface};

/// A body layer already warped into destination space.
pub(super) struct ProjectedLayer {
    pub layer: tiny_skia::Pixmap,
    /// Device-pixel top-left of the layer inside the target pixmap.
    pub origin: (f32, f32),
    /// Painted projected body box relative to `origin`.
    pub bbox: (f32, f32, f32, f32),
}

/// Project and paint a scene3d body into a destination-space layer.
///
/// Returns `None` when the camera is an identity for this face (the caller then
/// keeps the flat path) or when no projection buffer can be allocated.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_body_layer(
    pixmap: &tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    effects: &ShapeEffects,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    reach_cap: f32,
) -> Option<ProjectedLayer> {
    let scene = effects.scene_3d.as_ref()?;
    let (min_x, min_y, max_x, max_y) = local_bounds(path)?;
    let w = (max_x - min_x) as f64;
    let h = (max_y - min_y) as f64;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    // The reference paints the offscreen at the live transform's device scale
    // and warps it under that transform (ref `projectScene3dPaint`).
    let transform_scale = EffectTransform::from_affine(transform).device_scale();
    let local_scale = (scale as f64) * transform_scale;
    if !local_scale.is_finite() || local_scale <= 0.0 {
        return None;
    }
    let w_dev = w * local_scale;
    let h_dev = h * local_scale;

    let extrusion_dev = effects
        .shape_3d
        .as_ref()
        .map(|shape| shape.extrusion_depth_emu as f64 * local_scale)
        .unwrap_or(0.0)
        .max(0.0);
    // MS-OI29500 §2.1.1268 places bevelB at the extrusion base; only a bottom
    // lip (or the wireframe synthesised rim) selects the rear plane.
    let has_bottom = effects.shape_3d.as_ref().is_some_and(|shape| {
        shape.bevel_bottom.is_some() || shape.prst_material == "legacyWireframe"
    });
    let back_depth = if has_bottom { extrusion_dev } else { 0.0 };

    let camera = CameraInput::from_scene(scene)
        .scaled(local_scale)
        .with_scene_depth(extrusion_dev);
    let (depth_ox, depth_oy) = scene3d::compute_depth_offset(&camera, w_dev, h_dev, extrusion_dev);
    let depth_reach = (depth_ox * depth_ox + depth_oy * depth_oy).sqrt() as f32;
    // ref renderer.ts:3944 — identity/front cameras keep the flat path.
    if !scene3d::is_scene3d_non_identity(&camera, Some(w_dev), Some(h_dev), back_depth)
        && depth_reach < 0.75
    {
        return None;
    }

    // Local margin: the bevel/extrusion raster plus the painted stroke/contour
    // overhang (ref `edgePadCss`, `renderer.ts:4013-4018`).
    let painted_pad = painted_padding(stroke, effects, local_scale as f32) + depth_reach + 2.0;
    let mut pad = painted_pad;
    let padding = effect_padding(effects, local_scale as f32, (0.0, 0.0, w_dev as f32, h_dev as f32), reach_cap);
    pad = pad.max(padding.0).max(padding.1).max(padding.2).max(padding.3);

    let surface = scene3d::projected_surface(
        &camera,
        w_dev,
        h_dev,
        back_depth,
        pad as f64,
        1.0,
    );
    let surface_pad = surface.pad_dev as f32;
    let ow = (w_dev as f32 + 2.0 * surface_pad).ceil().max(1.0) as u32;
    let oh = (h_dev as f32 + 2.0 * surface_pad).ceil().max(1.0) as u32;
    if (ow as usize) * (oh as usize) > MAX_LAYER_PIXELS {
        return None;
    }
    let mut local = tiny_skia::Pixmap::new(ow, oh)?;

    // Paint the authored face and extruded side walls in local space, ahead of
    // the shape's 2D placement.
    draw_projected_solid(
        &mut local,
        assets,
        path,
        fill,
        stroke,
        effects,
        local_scale as f32,
        surface_pad - min_x * local_scale as f32,
        surface_pad - min_y * local_scale as f32,
        depth_ox as f32,
        depth_oy as f32,
        surface.back_facing,
        back_depth > 0.0,
    );

    let dst = destination_corners(transform, scale, offset_x, offset_y, local_scale, (min_x, min_y), &surface);
    let quad = quad_bounds(&dst);
    let outer_pad = effect_padding(effects, scale, quad, reach_cap);
    let canvas_w = pixmap.width() as f32;
    let canvas_h = pixmap.height() as f32;
    let x0 = (quad.0 - outer_pad.0 - painted_pad).floor().max(0.0);
    let y0 = (quad.1 - outer_pad.1 - painted_pad).floor().max(0.0);
    let x1 = (quad.0 + quad.2 + outer_pad.2 + painted_pad).ceil().min(canvas_w);
    let y1 = (quad.1 + quad.3 + outer_pad.3 + painted_pad).ceil().min(canvas_h);
    if x1 <= x0 || y1 <= y0 {
        // Fully off-canvas: keep the projection path with an empty layer so the
        // effect passes still consume the projected (empty) silhouette.
        return Some(ProjectedLayer {
            layer: tiny_skia::Pixmap::new(1, 1)?,
            origin: (x0, y0),
            bbox: (0.0, 0.0, 0.0, 0.0),
        });
    }
    let lw = (x1 - x0) as usize;
    let lh = (y1 - y0) as usize;
    if lw * lh > MAX_LAYER_PIXELS {
        return None;
    }
    let mut layer = tiny_skia::Pixmap::new(lw as u32, lh as u32)?;
    let local_dst = dst.map(|(x, y)| (x - x0 as f64, y - y0 as f64));
    warp_into(&local, &mut layer, local_dst);

    Some(ProjectedLayer {
        layer,
        origin: (x0, y0),
        bbox: (quad.0 - x0, quad.1 - y0, quad.2, quad.3),
    })
}

/// Map the padded camera quad through the element's 2D placement and the
/// target scale into device pixels (ref `renderer.ts:6628-6632`).
fn destination_corners(
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    local_scale: f64,
    local_origin: (f32, f32),
    surface: &ProjectedSurface,
) -> [(f64, f64); 4] {
    surface.corners.map(|(x, y)| {
        // Camera units are local device px; recover the path-space point first
        // so an outer 2D rotation/scale lands on the authored coordinates.
        let local_x = local_origin.0 + (x / local_scale) as f32;
        let local_y = local_origin.1 + (y / local_scale) as f32;
        let (px, py) = transform.transform_point(local_x, local_y);
        (
            (offset_x + px * scale) as f64,
            (offset_y + py * scale) as f64,
        )
    })
}

fn quad_bounds(corners: &[(f64, f64); 4]) -> (f32, f32, f32, f32) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in corners {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    (
        min_x as f32,
        min_y as f32,
        (max_x - min_x) as f32,
        (max_y - min_y) as f32,
    )
}

/// Warp `src` onto `dst` through the homography that maps the unit square to
/// `corners` (ref `drawProjected`, `scene3d-draw.ts:338-415`).
fn warp_into(src: &tiny_skia::Pixmap, dst: &mut tiny_skia::Pixmap, corners: [(f64, f64); 4]) {
    let Some(h) = scene3d::unit_square_to_quad(corners) else {
        return;
    };
    let Some(inv) = scene3d::invert_h(&h) else {
        return;
    };
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let src_data = src.data();
    let out_w = dst.width() as usize;
    let out_h = dst.height() as usize;
    let out_data = dst.data_mut();
    let fx = sw as f64;
    let fy = sh as f64;
    for y in 0..out_h {
        for x in 0..out_w {
            let dx = x as f64 + 0.5;
            let dy = y as f64 + 0.5;
            let w = inv[6] * dx + inv[7] * dy + inv[8];
            if !(w.is_finite() && w.abs() > 1e-12) {
                continue;
            }
            let u = (inv[0] * dx + inv[1] * dy + inv[2]) / w;
            let v = (inv[3] * dx + inv[4] * dy + inv[5]) / w;
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let sample = bilinear(src_data, sw as usize, sh as usize, u * fx - 0.5, v * fy - 0.5);
            let alpha = sample[3];
            if alpha <= 0.0 {
                continue;
            }
            let base = (y * out_w + x) * 4;
            for channel in 0..4 {
                let value = sample[channel] + out_data[base + channel] as f64 * (1.0 - alpha / 255.0);
                out_data[base + channel] = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Bilinear sample of premultiplied RGBA bytes with edge clamping.
fn bilinear(data: &[u8], width: usize, height: usize, x: f64, y: f64) -> [f64; 4] {
    let x = x.clamp(0.0, (width.saturating_sub(1)) as f64);
    let y = y.clamp(0.0, (height.saturating_sub(1)) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width.saturating_sub(1));
    let y1 = (y0 + 1).min(height.saturating_sub(1));
    let tx = x - x0 as f64;
    let ty = y - y0 as f64;
    let at = |px: usize, py: usize| -> [f64; 4] {
        let base = (py * width + px) * 4;
        [
            data[base] as f64,
            data[base + 1] as f64,
            data[base + 2] as f64,
            data[base + 3] as f64,
        ]
    };
    let p00 = at(x0, y0);
    let p10 = at(x1, y0);
    let p01 = at(x0, y1);
    let p11 = at(x1, y1);
    let mut out = [0.0; 4];
    for channel in 0..4 {
        let top = p00[channel] + (p10[channel] - p00[channel]) * tx;
        let bottom = p01[channel] + (p11[channel] - p01[channel]) * tx;
        out[channel] = top + (bottom - top) * ty;
    }
    out
}

/// Local bounds of a path in authored coordinates.
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
    for segment in &path.segments {
        match segment {
            crate::contracts::PathSegment::MoveTo { x, y }
            | crate::contracts::PathSegment::LineTo { x, y } => {
                add(*x, *y);
                any = true;
            }
            crate::contracts::PathSegment::QuadTo { x1, y1, x, y } => {
                add(*x1, *y1);
                add(*x, *y);
                any = true;
            }
            crate::contracts::PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                add(*x1, *y1);
                add(*x2, *y2);
                add(*x, *y);
                any = true;
            }
            crate::contracts::PathSegment::Close => {}
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}
