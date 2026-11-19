//! Run-level distance-aware text reflection compositing (S8).
//!
//! PowerPoint keeps a floor reflection sharp where it meets the source text
//! run and increases the authored blur toward the far edge. The contact edge
//! is sharp and the far edge reaches `blurRad` through [`reflection_blur_bands`].

use cosmic_text::{FontSystem, SwashCache};

use crate::contracts::{AffineTransform, PositionedGlyphRun, VectorReflection};
use crate::rasterizer::filters;
use crate::rasterizer::mask::pixmap_from_premultiplied;
use crate::rasterizer::reflection_blur::reflection_blur_bands;
use crate::rasterizer::shape_effect::reflection::{
    band_alpha, mirror_matrix, padding, Bbox,
};
use crate::rasterizer::shape_effect::MAX_LAYER_PIXELS;
use crate::rasterizer::scratch::EffectScratch;

/// Render the distance-aware reflection under a single text run.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_reflected_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    reflection: &VectorReflection,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    scratch: &mut EffectScratch,
) {
    if run.text.is_empty() || run.glyphs.is_empty() {
        return;
    }
    if reflection.start_opacity.clamp(0.0, 1.0) <= 0.0
        && reflection.end_opacity.clamp(0.0, 1.0) <= 0.0
    {
        return;
    }

    // 1. Determine run bounding box in slide EMU space.
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for g in &run.glyphs {
        min_x = min_x.min(g.x);
        max_x = max_x.max(g.x + g.advance);
    }
    if min_x >= max_x || !min_x.is_finite() || !max_x.is_finite() {
        return;
    }

    let font_size_emu = (run.font_size_pt * 12700.0).max(1.0);
    let ascent_emu = font_size_emu * 0.85;
    let descent_emu = font_size_emu * 0.35;
    let baseline_emu = run.glyphs.first().map(|g| g.y).unwrap_or(0.0);
    let emu_top = baseline_emu - ascent_emu;
    let emu_h = (ascent_emu + descent_emu).max(1.0);

    // Transform EMU bbox corners to destination pixel coordinates.
    let (p0_x, p0_y) = transform.transform_point(min_x, emu_top);
    let (p1_x, p1_y) = transform.transform_point(max_x, emu_top);
    let (p2_x, p2_y) = transform.transform_point(max_x, emu_top + emu_h);
    let (p3_x, p3_y) = transform.transform_point(min_x, emu_top + emu_h);

    let xs = [p0_x, p1_x, p2_x, p3_x];
    let ys = [p0_y, p1_y, p2_y, p3_y];
    let px_min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min) * scale + offset_x;
    let px_max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max) * scale + offset_x;
    let px_min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min) * scale + offset_y;
    let px_max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max) * scale + offset_y;

    let body_w = (px_max_x - px_min_x).max(1.0);
    let body_h = (px_max_y - px_min_y).max(1.0);
    let body_bbox: Bbox = (px_min_x, px_min_y, body_w, body_h);

    let reach_cap = ((pixmap.width() * pixmap.width() + pixmap.height() * pixmap.height()) as f32).sqrt();
    let sigma = filters::clamp_reach_px(reflection.blur_radius_emu, scale, reach_cap);
    let (pad_left, pad_top, pad_right, pad_bottom) = padding(reflection, body_bbox, scale, sigma);

    let min_layer_x = (body_bbox.0 - pad_left).floor();
    let min_layer_y = (body_bbox.1 - pad_top).floor();
    let max_layer_x = (body_bbox.0 + body_bbox.2 + pad_right).ceil();
    let max_layer_y = (body_bbox.1 + body_bbox.3 + pad_bottom).ceil();

    let layer_w = (max_layer_x - min_layer_x).max(1.0) as usize;
    let layer_h = (max_layer_y - min_layer_y).max(1.0) as usize;
    if layer_w == 0 || layer_h == 0 || layer_w * layer_h > MAX_LAYER_PIXELS {
        return;
    }

    let Some(mut layer) = tiny_skia::Pixmap::new(layer_w as u32, layer_h as u32) else {
        return;
    };

    // Render un-mirrored run into layer
    let layer_offset_x = offset_x - min_layer_x;
    let layer_offset_y = offset_y - min_layer_y;
    super::render_single_run(
        &mut layer,
        run,
        transform,
        scale,
        layer_offset_x,
        layer_offset_y,
        font_system,
        swash_cache,
    );

    let layer_bbox: Bbox = (
        body_bbox.0 - min_layer_x,
        body_bbox.1 - min_layer_y,
        body_w,
        body_h,
    );

    // 2. Distance-aware blur bands (S8)
    let bands = reflection_blur_bands(layer_bbox, sigma);
    let mut src = vec![0u8; layer_w * layer_h * 4];

    for band in bands {
        let band_top = (band.y.floor() as usize).clamp(0, layer_h);
        let band_bottom = ((band.y + band.h).ceil() as usize).clamp(0, layer_h);
        if band_top >= band_bottom {
            continue;
        }

        if band.radius <= 1e-4 {
            for row in band_top..band_bottom {
                let start = row * layer_w * 4;
                let end = start + layer_w * 4;
                src[start..end].copy_from_slice(&layer.data()[start..end]);
            }
        } else {
            let mut blurred = layer.data().to_vec();
            filters::gaussian_blur_premultiplied_with(scratch, &mut blurred, layer_w, layer_h, band.radius);
            for row in band_top..band_bottom {
                let start = row * layer_w * 4;
                let end = start + layer_w * 4;
                src[start..end].copy_from_slice(&blurred[start..end]);
            }
        }
    }

    // 3. Alpha fade gradient
    let bottom = layer_bbox.1 + layer_bbox.3;
    for (row, line) in src.chunks_mut(layer_w * 4).enumerate() {
        let f = band_alpha((bottom - row as f32) / layer_bbox.3, reflection);
        if f >= 1.0 - 1e-6 {
            continue;
        }
        if f <= 1e-6 {
            line.fill(0);
            continue;
        }
        for byte in line.iter_mut() {
            *byte = (*byte as f32 * f).round() as u8;
        }
    }

    let Some(aux) = pixmap_from_premultiplied(&src, layer_w, layer_h) else {
        return;
    };

    // 4. Mirrored blit under the run
    let m = mirror_matrix(reflection, layer_bbox, scale);
    let blit_transform = tiny_skia::Transform::from_row(
        m[0],
        m[1],
        m[2],
        m[3],
        m[4] + min_layer_x,
        m[5] + min_layer_y,
    );
    let paint = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..tiny_skia::PixmapPaint::default()
    };
    pixmap.draw_pixmap(0, 0, aux.as_ref(), &paint, blit_transform, None);
}
