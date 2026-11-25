//! `a:reflection` compositing — ECMA-376 §20.1.8.50.
//!
//! Port of ref packages/core/src/shape/effects.ts:562-640 `applyReflection`:
//!
//! 1. paint the silhouette into an aux layer (blurred with sigma = `blurRad`),
//! 2. fade it with the authored band gradient `stA`/`stPos` → `endA`/`endPos`,
//! 3. blit it back mirrored about the `algn` pivot, rotated so the mirror faces
//!    `dir`, and pushed `dist` along `dir`.
//!
//! The reference always mirrors vertically because its parser drops `dir`
//! (`effects.ts:553-559`); here the mirror axis follows `dir` so that 0° puts
//! the copy to the right of the shape. With `dir = 90°` the matrix reduces to
//! the reference's `translate(bbox.x, bottom) · scale(sx, sy) · translate(-…)`.

use super::filters;
use crate::contracts::VectorReflection;
use crate::rasterizer::mask::pixmap_from_premultiplied;
use crate::rasterizer::scratch::EffectScratch;

/// `(x, y, w, h)` box in the coordinate space of the target canvas.
pub(crate) type Bbox = (f32, f32, f32, f32);

/// Alpha at band offset `t` — `0` at the shape's bottom edge, `1` at its top.
///
/// Rebuilds the reference's gradient stops verbatim (effects.ts:612-622):
/// `stA` holds up to `stPos`, then a linear ramp reaches `endA` at `endPos` and
/// holds beyond. The reference skips the `endPos` stop when `endPos >= 1` or
/// `endPos <= stPos` (canvas stops must be non-decreasing), which leaves the
/// ramp spanning the rest of the band — mirrored here.
pub(crate) fn band_alpha(t: f32, r: &VectorReflection) -> f32 {
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

/// `algn` pivot (`ST_RectAlignment`), defaulting to the bottom centre.
/// ref effects.ts:98-110 `shadowAlignmentPoint`.
pub(crate) fn align_point(bbox: Bbox, align: &str) -> (f32, f32) {
    let (x, y, w, h) = bbox;
    let ax = match align {
        "tl" | "l" | "bl" => x,
        "tr" | "r" | "br" => x + w,
        _ => x + w * 0.5,
    };
    let ay = match align {
        "tl" | "t" | "tr" => y,
        "l" | "ctr" | "r" => y + h * 0.5,
        _ => y + h,
    };
    (ax, ay)
}

/// Authored `sx`/`sy` as a positive magnitude (the reference passes the sign
/// through `scale()`, and ECMA defaults `sy` to −100 %). Degenerate and
/// non-finite values must not poison the blit matrix.
pub(crate) fn scale_magnitude(v: f32) -> f32 {
    if v.is_finite() {
        v.abs().clamp(1e-3, 64.0)
    } else {
        1.0
    }
}

/// `[a b c d e f]` with `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
///
/// The mirrored copy is `R(θ)·diag(sx, -sy)` about the `algn` pivot, where
/// `θ = dir - 90°` turns the default downward mirror onto `dir`, plus the
/// `dist` offset along `dir`. ref effects.ts:626-639.
pub(crate) fn mirror_matrix(r: &VectorReflection, bbox: Bbox, scale: f32) -> [f32; 6] {
    let (ax, ay) = align_point(bbox, &r.align);
    let mx = scale_magnitude(r.scale_x);
    let my = scale_magnitude(r.scale_y);
    let dir = if r.direction_deg.is_finite() {
        r.direction_deg
    } else {
        90.0
    };
    let dist = if scale.is_finite() {
        r.distance_emu as f32 * scale
    } else {
        0.0
    };
    let (off_x, off_y) = filters::directional_offset(dir, dist);
    let (sin, cos) = (dir as f32 - 90.0).to_radians().sin_cos();
    let (a, c) = (cos * mx, sin * my);
    let (b, d) = (sin * mx, -cos * my);
    [
        a,
        b,
        c,
        d,
        ax + off_x - (a * ax + c * ay),
        ay + off_y - (b * ax + d * ay),
    ]
}

/// Pixels the mirrored copy reaches outside `bbox`, plus the blur margin.
/// Used to size the scratch layer (ref effects.ts:4385-4400 `margin`).
pub(crate) fn padding(r: &VectorReflection, bbox: Bbox, scale: f32, sigma: f32) -> Bbox {
    let m = mirror_matrix(r, bbox, scale);
    let (x, y, w, h) = bbox;
    let mut lo = (f32::MAX, f32::MAX);
    let mut hi = (f32::MIN, f32::MIN);
    for (px, py) in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
        let (u, v) = (m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5]);
        lo = (lo.0.min(u), lo.1.min(v));
        hi = (hi.0.max(u), hi.1.max(v));
    }
    // 3σ covers the Gaussian tail; +2 px survives rounding to whole pixels.
    let pad = 3.0 * sigma + 2.0;
    (
        (x - lo.0).max(0.0) + pad,
        (y - lo.1).max(0.0) + pad,
        (hi.0 - (x + w)).max(0.0) + pad,
        (hi.1 - (y + h)).max(0.0) + pad,
    )
}

/// Composite the reflection under the body.
///
/// `layer` is the scratch layer holding the painted shape, `bbox` the body box
/// in layer-local pixels and `origin` the layer's canvas offset.
#[allow(clippy::too_many_arguments)]
pub(super) fn composite(
    pixmap: &mut tiny_skia::Pixmap,
    layer: &tiny_skia::Pixmap,
    reflection: &VectorReflection,
    scale: f32,
    reach_cap: f32,
    origin: (f32, f32),
    bbox: Bbox,
    scratch: &mut EffectScratch,
) {
    let (lw, lh) = (layer.width() as usize, layer.height() as usize);
    let (_, _, bw, bh) = bbox;
    if lw == 0 || lh == 0 || bw <= 0.0 || bh <= 0.0 {
        return;
    }
    if reflection.start_opacity.clamp(0.0, 1.0) <= 0.0 && reflection.end_opacity.clamp(0.0, 1.0) <= 0.0 {
        return;
    }

    // 1. The full painted layer is the aux canvas: blur first, then fade, as in
    //    ref effects.ts:598-624 (`filter='blur(r)'` takes r as sigma).
    let mut src = layer.data().to_vec();
    let sigma = filters::clamp_reach_px(reflection.blur_radius_emu, scale, reach_cap);
    if sigma > 0.0 {
        filters::gaussian_blur_premultiplied_with(scratch, &mut src, lw, lh, sigma);
    }

    // 2. `destination-in` against the authored band gradient.
    let bottom = bbox.1 + bh;
    for (row, line) in src.chunks_mut(lw * 4).enumerate() {
        let f = band_alpha((bottom - row as f32) / bh, reflection);
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

    let Some(aux) = pixmap_from_premultiplied(&src, lw, lh) else {
        return;
    };

    // 3. Mirrored blit under the shape (ref effects.ts:626-639). `draw_pixmap`
    //    maps source pixels through `transform`, so the layer origin is folded
    //    into the translation.
    let m = mirror_matrix(reflection, bbox, scale);
    let transform = tiny_skia::Transform::from_row(m[0], m[1], m[2], m[3], m[4] + origin.0, m[5] + origin.1);
    let paint = tiny_skia::PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..tiny_skia::PixmapPaint::default()
    };
    pixmap.draw_pixmap(0, 0, aux.as_ref(), &paint, transform, None);
}
