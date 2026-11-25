//! EMF+ texture brush rasterization.

use crate::contracts::{AffineTransform, VectorTextureFill};

/// Paint an inline texture fill inside `device_path` on the canvas pixmap.
#[allow(clippy::too_many_arguments)]
pub fn fill_path_with_texture(
    pixmap: &mut tiny_skia::Pixmap,
    texture: &VectorTextureFill,
    device_path: &tiny_skia::Path,
    fill_rule: tiny_skia::FillRule,
    command_transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let Ok(src) = tiny_skia::Pixmap::decode_png(&texture.png) else {
        return;
    };
    let spread = if texture.repeat {
        tiny_skia::SpreadMode::Repeat
    } else {
        tiny_skia::SpreadMode::Pad
    };
    let effective = command_transform.post_concat(&texture.transform);
    let ts_transform = tiny_skia::Transform::from_row(
        effective.sx * scale,
        effective.ky * scale,
        effective.kx * scale,
        effective.sy * scale,
        offset_x + effective.tx * scale,
        offset_y + effective.ty * scale,
    );
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.shader = tiny_skia::Pattern::new(
        src.as_ref(),
        spread,
        tiny_skia::FilterQuality::Bilinear,
        texture.opacity.clamp(0.0, 1.0),
        ts_transform,
    );
    pixmap.fill_path(device_path, &paint, fill_rule, tiny_skia::Transform::identity(), None);
}
