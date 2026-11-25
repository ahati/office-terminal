//! Extrusion layers and contour rim painting.

use crate::contracts::{
    AffineTransform, LineCap, LineJoin, ShapeEffects, VectorPaint, VectorPath, VectorShape3d, VectorStroke,
};
use crate::rasterizer::bevel::apply_bevel_shading;
use crate::rasterizer::path;

pub(super) fn resolve_extrusion_rgb(shape_3d: Option<&VectorShape3d>, fill: Option<&VectorPaint>) -> [u8; 3] {
    if let Some(shape_3d) = shape_3d {
        if let Some(c) = shape_3d.extrusion_color {
            return [c[0], c[1], c[2]];
        }
    }
    match fill {
        Some(VectorPaint::Solid([r, g, b, _])) => [
            (*r as f32 * 0.7) as u8,
            (*g as f32 * 0.7) as u8,
            (*b as f32 * 0.7) as u8,
        ],
        _ => [96, 96, 96],
    }
}

#[allow(clippy::chunks_exact_to_as_chunks)]
pub(super) fn sweep_extrusion(layer: &mut tiny_skia::Pixmap, offset_x: f32, offset_y: f32, rgb: [u8; 3]) {
    let width = layer.width() as usize;
    let height = layer.height() as usize;
    if width == 0 || height == 0 {
        return;
    }
    let dist = (offset_x * offset_x + offset_y * offset_y).sqrt();
    if dist < 0.75 {
        return;
    }
    let steps = (dist.ceil() as usize).max(1);
    let data = layer.data();
    let mask: Vec<u8> = data.chunks_exact(4).map(|c| c[3]).collect();
    let data_mut = layer.data_mut();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if mask[idx] > 0 {
                continue;
            }
            let mut hit = false;
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                let sx = (x as f32 - offset_x * t).round() as isize;
                let sy = (y as f32 - offset_y * t).round() as isize;
                if sx >= 0
                    && sy >= 0
                    && (sx as usize) < width
                    && (sy as usize) < height
                    && mask[sy as usize * width + sx as usize] > 0
                {
                    hit = true;
                    break;
                }
            }
            if hit {
                let base = idx * 4;
                data_mut[base..base + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_wireframe_solid(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    stroke: Option<&VectorStroke>,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    depth_ox: f32,
    depth_oy: f32,
) {
    let wire_stroke = stroke.cloned().unwrap_or(VectorStroke {
        color: [0, 0, 0, 255],
        width_emu: 12_700,
        dash_segments: None,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        paint: None,
        ..VectorStroke::default()
    });
    // Front rim
    path::render_path(
        layer,
        assets,
        path,
        None,
        Some(&wire_stroke),
        transform,
        scale,
        offset_x,
        offset_y,
    );
    let dist = (depth_ox * depth_ox + depth_oy * depth_oy).sqrt();
    if dist >= 0.75 {
        let steps = ((dist / 2.0).ceil() as usize).clamp(1, 24);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            path::render_path(
                layer,
                assets,
                path,
                None,
                Some(&wire_stroke),
                transform,
                scale,
                offset_x + depth_ox * t,
                offset_y + depth_oy * t,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_projected_solid(
    layer: &mut tiny_skia::Pixmap,
    assets: &dyn crate::loader::AssetStore,
    path: &VectorPath,
    fill: Option<&VectorPaint>,
    stroke: Option<&VectorStroke>,
    effects: &ShapeEffects,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    depth_ox: f32,
    depth_oy: f32,
    back_facing: bool,
    has_back_depth: bool,
) {
    let wireframe = effects
        .shape_3d
        .as_ref()
        .is_some_and(|shape_3d| shape_3d.prst_material.eq_ignore_ascii_case("legacyWireframe"));

    let (ox, oy) = if back_facing && has_back_depth {
        (-depth_ox, -depth_oy)
    } else {
        (depth_ox, depth_oy)
    };
    let depth_dist = (ox * ox + oy * oy).sqrt();
    let has_extrusion = depth_dist >= 0.75;

    if wireframe {
        draw_wireframe_solid(
            layer,
            assets,
            path,
            stroke,
            &AffineTransform::identity(),
            scale,
            offset_x,
            offset_y,
            ox,
            oy,
        );
        return;
    }

    let Some(shape_3d) = effects.shape_3d.as_ref() else {
        path::render_path(
            layer,
            assets,
            path,
            fill,
            stroke,
            &AffineTransform::identity(),
            scale,
            offset_x,
            offset_y,
        );
        return;
    };

    // 1. Authored body face
    path::render_path(
        layer,
        assets,
        path,
        fill,
        stroke,
        &AffineTransform::identity(),
        scale,
        offset_x,
        offset_y,
    );

    let extrusion_rgb = resolve_extrusion_rgb(Some(shape_3d), fill);
    let order_rear_first = back_facing && shape_3d.bevel_bottom.is_some();
    let width = layer.width() as usize;
    let height = layer.height() as usize;

    if order_rear_first {
        apply_bevel_shading(layer.data_mut(), width, height, shape_3d, effects, scale);
        if has_extrusion {
            sweep_extrusion(layer, ox, oy, extrusion_rgb);
        }
    } else {
        if has_extrusion {
            sweep_extrusion(layer, ox, oy, extrusion_rgb);
        }
        apply_bevel_shading(layer.data_mut(), width, height, shape_3d, effects, scale);
    }

    draw_contour(
        layer,
        assets,
        path,
        stroke,
        shape_3d,
        &AffineTransform::identity(),
        scale,
        offset_x,
        offset_y,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_extrusion(
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
pub(super) fn draw_contour(
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
        ..VectorStroke::default()
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
