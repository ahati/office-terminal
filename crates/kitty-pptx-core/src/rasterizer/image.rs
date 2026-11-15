//! The `<p:pic>` blit path.

use crate::contracts::{AffineTransform, BitmapSampling, NormalizedRect, RectF32};
use crate::loader::{AssetStore, SizeHint};

#[allow(clippy::too_many_arguments)]
pub fn render_image(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    rel_id: &str,
    src_rect: &NormalizedRect,
    dest_rect: &RectF32,
    opacity: f32,
    transform: &AffineTransform,
    effects: &[ooxml_common::blip::BlipEffect],
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    render_image_with_hint(
        pixmap,
        assets,
        rel_id,
        SizeHint::Native,
        src_rect,
        dest_rect,
        opacity,
        transform,
        effects,
        scale,
        offset_x,
        offset_y,
    );
}

/// [`render_image`] with an explicit decode hint
/// (`decode_plan::DecodePlan::size_hint`).
#[allow(clippy::too_many_arguments)]
pub fn render_image_with_hint(
    pixmap: &mut tiny_skia::Pixmap,
    assets: &dyn AssetStore,
    rel_id: &str,
    hint: SizeHint,
    src_rect: &NormalizedRect,
    dest_rect: &RectF32,
    opacity: f32,
    transform: &AffineTransform,
    effects: &[ooxml_common::blip::BlipEffect],
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let decoded = match assets.get(rel_id, hint) {
        Ok(img) => img,
        Err(_) => return, // Asset missing, fail-soft
    };

    if decoded.width == 0 || decoded.height == 0 {
        return;
    }

    // Handle src_rect crop
    let (crop_x, crop_y, crop_w, crop_h) = {
        let x = (src_rect.x * decoded.width as f32).round().max(0.0) as u32;
        let y = (src_rect.y * decoded.height as f32).round().max(0.0) as u32;
        let w = (src_rect.w * decoded.width as f32)
            .round()
            .clamp(1.0, (decoded.width - x.min(decoded.width - 1)) as f32) as u32;
        let h = (src_rect.h * decoded.height as f32)
            .round()
            .clamp(1.0, (decoded.height - y.min(decoded.height - 1)) as f32) as u32;
        (x, y, w, h)
    };

    let mut src_pixmap = match tiny_skia::Pixmap::new(crop_w, crop_h) {
        Some(p) => p,
        None => return,
    };

    // Extract cropped straight RGBA8 pixels
    let orig_pixels = &decoded.pixels;
    let orig_w = decoded.width as usize;
    let mut cropped_rgba = vec![0u8; (crop_w * crop_h * 4) as usize];

    for row in 0..crop_h as usize {
        let src_row_start = ((crop_y as usize + row) * orig_w + crop_x as usize) * 4;
        let dst_row_start = (row * crop_w as usize) * 4;

        if src_row_start + crop_w as usize * 4 <= orig_pixels.len() {
            cropped_rgba[dst_row_start..dst_row_start + crop_w as usize * 4]
                .copy_from_slice(&orig_pixels[src_row_start..src_row_start + crop_w as usize * 4]);
        }
    }

    // Apply blip effects if present
    if !effects.is_empty() {
        super::effects::apply_blip_effects(&mut cropped_rgba, effects);
    }

    let opacity_clamped = opacity.clamp(0.0, 1.0);
    let dst_data = src_pixmap.data_mut();

    #[allow(clippy::chunks_exact_to_as_chunks)]
    for (src_chunk, dst_chunk) in cropped_rgba.chunks_exact(4).zip(dst_data.chunks_exact_mut(4)) {
        let r = src_chunk[0] as u32;
        let g = src_chunk[1] as u32;
        let b = src_chunk[2] as u32;
        let a = ((src_chunk[3] as f32 * opacity_clamped).round() as u32).min(255);

        if a == 0 {
            dst_chunk[0] = 0;
            dst_chunk[1] = 0;
            dst_chunk[2] = 0;
            dst_chunk[3] = 0;
        } else if a == 255 {
            dst_chunk[0] = r as u8;
            dst_chunk[1] = g as u8;
            dst_chunk[2] = b as u8;
            dst_chunk[3] = 255;
        } else {
            // Premultiply
            dst_chunk[0] = ((r * a + 127) / 255) as u8;
            dst_chunk[1] = ((g * a + 127) / 255) as u8;
            dst_chunk[2] = ((b * a + 127) / 255) as u8;
            dst_chunk[3] = a as u8;
        }
    }

    // Compute transformation matrix:
    // 1. Image coords [0, crop_w] x [0, crop_h] -> dest_rect in shape EMU
    let s_img_x = dest_rect.w / crop_w as f32;
    let s_img_y = dest_rect.h / crop_h as f32;

    let m1 = AffineTransform {
        sx: s_img_x,
        ky: 0.0,
        kx: 0.0,
        sy: s_img_y,
        tx: dest_rect.x,
        ty: dest_rect.y,
    };

    // 2. Shape EMU -> Slide EMU
    let m2 = transform.post_concat(&m1);

    // 3. Slide EMU -> Screen pixels
    let total_sx = m2.sx * scale;
    let total_ky = m2.ky * scale;
    let total_kx = m2.kx * scale;
    let total_sy = m2.sy * scale;
    let total_tx = offset_x + m2.tx * scale;
    let total_ty = offset_y + m2.ty * scale;

    let ts_transform = tiny_skia::Transform::from_row(
        total_sx,
        total_ky,
        total_kx,
        total_sy,
        total_tx,
        total_ty,
    );

    let paint = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };

    pixmap.draw_pixmap(
        0,
        0,
        src_pixmap.as_ref(),
        &paint,
        ts_transform,
        None,
    );
}

/// Paints an inline PNG from `VectorCommand::DrawBitmap`.
///
/// `placement` maps the bitmap's `[0, 1]` square into element-local EMU; the
/// target scale/offset of the current raster pass is applied on top.
#[allow(clippy::too_many_arguments)]
pub fn render_bitmap(
    pixmap: &mut tiny_skia::Pixmap,
    png: &[u8],
    placement: &AffineTransform,
    opacity: f32,
    sampling: BitmapSampling,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let Ok(src) = tiny_skia::Pixmap::decode_png(png) else {
        return;
    };
    // `placement` maps the bitmap's [0, 1] square into element-local EMU, while
    // `draw_pixmap` transforms source *pixel* coordinates. Fold the inverse
    // source size into the matrix: pixel → unit → EMU → screen pixels.
    let src_w = src.width() as f32;
    let src_h = src.height() as f32;
    if src_w <= 0.0 || src_h <= 0.0 {
        return;
    }
    let transform = tiny_skia::Transform::from_row(
        placement.sx * scale / src_w,
        placement.ky * scale / src_w,
        placement.kx * scale / src_h,
        placement.sy * scale / src_h,
        offset_x + placement.tx * scale,
        offset_y + placement.ty * scale,
    );
    let paint = tiny_skia::PixmapPaint {
        opacity: opacity.clamp(0.0, 1.0),
        quality: match sampling {
            BitmapSampling::Nearest => tiny_skia::FilterQuality::Nearest,
            BitmapSampling::Linear => tiny_skia::FilterQuality::Bilinear,
        },
        ..Default::default()
    };
    pixmap.draw_pixmap(0, 0, src.as_ref(), &paint, transform, None);
}

/// Defensive fallback for an unexpanded `DrawMetafile` marker.
///
/// Expansion always replaces markers, so this only fires on a pipeline bug or
/// when a caller hands a raw compiled scene to a backend. It paints a light box
/// with a grey border instead of leaving an invisible hole.
pub fn render_missing_metafile(
    pixmap: &mut tiny_skia::Pixmap,
    dest_rect: &RectF32,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let local = AffineTransform {
        tx: dest_rect.x,
        ty: dest_rect.y,
        ..AffineTransform::default()
    };
    let m = transform.post_concat(&local);
    let target = tiny_skia::Transform::from_row(
        m.sx * scale,
        m.ky * scale,
        m.kx * scale,
        m.sy * scale,
        offset_x + m.tx * scale,
        offset_y + m.ty * scale,
    );

    let w = dest_rect.w.max(1.0);
    let h = dest_rect.h.max(1.0);
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(0.0, 0.0);
    builder.line_to(w, 0.0);
    builder.line_to(w, h);
    builder.line_to(0.0, h);
    builder.close();
    let Some(path) = builder.finish() else {
        return;
    };

    let mut fill = tiny_skia::Paint::default();
    fill.set_color(tiny_skia::Color::from_rgba8(242, 242, 242, 255));
    pixmap.fill_path(
        &path,
        &fill,
        tiny_skia::FillRule::Winding,
        target,
        None,
    );

    let mut stroke = tiny_skia::Paint::default();
    stroke.set_color(tiny_skia::Color::from_rgba8(150, 150, 150, 255));
    stroke.anti_alias = true;
    let stroke_style = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &stroke, &stroke_style, target, None);
}
