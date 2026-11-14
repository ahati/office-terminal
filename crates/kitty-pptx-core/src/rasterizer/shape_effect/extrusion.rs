//! Extrusion layers and contour rim painting.

use crate::contracts::{
    AffineTransform, LineCap, LineJoin, VectorPaint, VectorPath, VectorShape3d, VectorStroke,
};
use crate::rasterizer::path;

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

