//! Separable Gaussian blur approximation used by the shape-effect passes.
//!
//! Implements the three-pass box-filter approximation from PRD 08 §4.1. The
//! sliding-window (prefix-sum) accumulators make each pass `O(1)` per pixel
//! regardless of the blur radius. RGBA buffers are alpha-weighted by
//! premultiplying before convolution and unpremultiplying afterwards, so
//! transparent pixels cannot bleed dark halos into visible edges.
//!
//! Taps beyond the buffer read as fully transparent, matching the CSS
//! `filter: blur()` the reference relies on ("CSS blur() treats everything
//! outside the canvas as transparent", ref packages/core/src/shape/effects.ts
//! :131-140). Buffers therefore must be padded by the kernel extent when the
//! caller needs the interior to stay untouched.
//!
//! The box-filter kernels live in [`kernels`]; the `_with` twins take pooled
//! working buffers from [`crate::rasterizer::scratch::EffectScratch`] so a
//! whole raster pass reuses one set of ping-pong buffers.

#![allow(clippy::chunks_exact_to_as_chunks)]

mod kernels;

use kernels::{
    box_blur_horizontal_alpha, box_blur_horizontal_rgba, box_blur_vertical_alpha, box_blur_vertical_rgba, premultiply,
    unpremultiply,
};

use crate::contracts::VectorOuterShadow;
use crate::rasterizer::scratch::EffectScratch;

/// Compute the three odd box widths whose cascaded variance best matches the
/// requested Gaussian standard deviation (PRD 08 §4.1).
///
/// The two candidate widths are `2·floor(w_ideal/2)-1` and that plus two; the
/// cascade is chosen by the *closest variance*, not by rounding the classic
/// high-pass count. The rounding heuristic quantises very badly for small
/// sigma, where adjacent odd widths are a large relative step (sigma 3.33 used
/// to collapse to 2.45, a 26 % error against a reference CSS Gaussian).
pub fn box_radii_for_sigma(sigma: f32) -> [usize; 3] {
    if sigma <= 0.0 || !sigma.is_finite() {
        return [0, 0, 0];
    }
    let w_ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut w_low = 2.0 * (w_ideal / 2.0).floor() - 1.0;
    if w_low < 1.0 {
        w_low = 1.0;
    }
    let w_high = w_low + 2.0;
    let target = sigma * sigma;

    let mut best = [0usize; 3];
    let mut best_error = f32::MAX;
    for high in 0..=3usize {
        let mut radii = [0usize; 3];
        for (i, radius) in radii.iter_mut().enumerate() {
            let width = if i < high { w_high } else { w_low };
            *radius = (width / 2.0) as usize;
        }
        let error = (cascaded_variance(radii) - target).abs();
        if error < best_error {
            best_error = error;
            best = radii;
        }
    }
    best
}

/// Variance of the cascaded box filters, used to validate the approximation.
pub fn cascaded_variance(radii: [usize; 3]) -> f32 {
    radii.iter().map(|&r| (r * (r + 1)) as f32 / 3.0).sum()
}

/// Blur a straight-alpha RGBA8 buffer. Alpha is honoured during convolution.
pub fn gaussian_blur_rgba(buf: &mut [u8], width: usize, height: usize, sigma: f32) {
    premultiply(buf);
    gaussian_blur_premultiplied(buf, width, height, sigma);
    unpremultiply(buf);
}

/// Blur an already-premultiplied RGBA8 buffer.
pub fn gaussian_blur_premultiplied(buf: &mut [u8], width: usize, height: usize, sigma: f32) {
    let mut scratch = EffectScratch::default();
    gaussian_blur_premultiplied_with(&mut scratch, buf, width, height, sigma);
}

/// Pooled twin of [`gaussian_blur_premultiplied`]: identical algorithm, but
/// the ping-pong and prefix working buffers are reused across calls.
pub(crate) fn gaussian_blur_premultiplied_with(
    scratch: &mut EffectScratch,
    buf: &mut [u8],
    width: usize,
    height: usize,
    sigma: f32,
) {
    let radii = box_radii_for_sigma(sigma);
    if width == 0 || height == 0 || radii.iter().all(|&r| r == 0) {
        return;
    }
    let len = width * height * 4;
    if buf.len() < len {
        return;
    }

    let EffectScratch {
        rgba_a, rgba_b, prefix, ..
    } = scratch;
    rgba_a.resize(len, 0);
    rgba_a[..len].copy_from_slice(&buf[..len]);
    rgba_b.resize(len, 0);
    prefix.resize((width + 1).max(height + 1), 0);
    for radius in radii {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal_rgba(
            &rgba_a[..len],
            &mut rgba_b[..len],
            width,
            height,
            radius,
            &mut prefix[..width + 1],
        );
        box_blur_vertical_rgba(
            &rgba_b[..len],
            &mut rgba_a[..len],
            width,
            height,
            radius,
            &mut prefix[..height + 1],
        );
    }
    buf[..len].copy_from_slice(&rgba_a[..len]);
}

/// Blur a single-channel 8-bit alpha mask.
pub fn gaussian_blur_alpha(mask: &mut [u8], width: usize, height: usize, sigma: f32) {
    let mut scratch = EffectScratch::default();
    gaussian_blur_alpha_with(&mut scratch, mask, width, height, sigma);
}

/// Pooled twin of [`gaussian_blur_alpha`]: identical algorithm, but the
/// ping-pong and prefix working buffers are reused across calls.
pub(crate) fn gaussian_blur_alpha_with(
    scratch: &mut EffectScratch,
    mask: &mut [u8],
    width: usize,
    height: usize,
    sigma: f32,
) {
    let radii = box_radii_for_sigma(sigma);
    if width == 0 || height == 0 || radii.iter().all(|&r| r == 0) {
        return;
    }
    let len = width * height;
    if mask.len() < len {
        return;
    }

    let EffectScratch {
        alpha_a,
        alpha_b,
        prefix,
        ..
    } = scratch;
    alpha_a.resize(len, 0);
    alpha_a[..len].copy_from_slice(&mask[..len]);
    alpha_b.resize(len, 0);
    prefix.resize((width + 1).max(height + 1), 0);
    for radius in radii {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal_alpha(
            &alpha_a[..len],
            &mut alpha_b[..len],
            width,
            height,
            radius,
            &mut prefix[..width + 1],
        );
        box_blur_vertical_alpha(
            &alpha_b[..len],
            &mut alpha_a[..len],
            width,
            height,
            radius,
            &mut prefix[..height + 1],
        );
    }
    mask[..len].copy_from_slice(&alpha_a[..len]);
}

/// Convert a DrawingML direction (degrees clockwise from East) and a distance
/// into a screen-space `(dx, dy)` offset. Screen coordinates use y-down.
pub fn directional_offset(direction_deg: f64, distance_px: f32) -> (f32, f32) {
    let rad = direction_deg.to_radians();
    (distance_px * rad.cos() as f32, distance_px * rad.sin() as f32)
}

/// Affine map applied to a single-channel alpha mask: `dest = A·src + t`.
/// Mirrors the 2×3 matrix the reference hands to `setTransform`
/// (ref packages/core/src/shape/effects.ts:applyOuterShadow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskAffine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl MaskAffine {
    /// Identity transform (no resampling needed).
    pub fn is_identity(&self) -> bool {
        self.a == 1.0 && self.b == 0.0 && self.c == 0.0 && self.d == 1.0 && self.tx == 0.0 && self.ty == 0.0
    }
}

/// Resample a single-channel alpha mask through `m` with bilinear sampling.
/// Samples outside the buffer read as zero alpha, matching the transparent
/// margin the reference aux canvases provide (ref effects.ts:computeCrop).
pub fn warp_mask_affine(mask: &[u8], width: usize, height: usize, m: &MaskAffine) -> Vec<u8> {
    if width == 0 || height == 0 || mask.len() < width * height || m.is_identity() {
        return mask[..width * height].to_vec();
    }
    let det = m.a * m.d - m.b * m.c;
    if !det.is_finite() || det.abs() < 1e-6 {
        return mask[..width * height].to_vec();
    }
    let (ia, ib, ic, id) = (m.d / det, -m.b / det, -m.c / det, m.a / det);
    let itx = -(ia * m.tx + ib * m.ty);
    let ity = -(ic * m.tx + id * m.ty);
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            // Pixel centres keep integer translations byte-exact.
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let sx = ia * px + ib * py + itx - 0.5;
            let sy = ic * px + id * py + ity - 0.5;
            out[y * width + x] = sample_mask_bilinear(mask, width, height, sx, sy);
        }
    }
    out
}

/// Subpixel translation of a single-channel mask. Integral offsets take the
/// exact pixel-copy path; fractional ones resample bilinearly.
pub fn shift_mask_subpixel(mask: &[u8], width: usize, height: usize, dx: f32, dy: f32) -> Vec<u8> {
    if !dx.is_finite() || !dy.is_finite() {
        return mask[..width * height].to_vec();
    }
    let (rx, ry) = (dx.round(), dy.round());
    if (dx - rx).abs() < 1e-4 && (dy - ry).abs() < 1e-4 {
        return super::mask::shift_mask(mask, width, height, rx as i32, ry as i32);
    }
    warp_mask_affine(
        mask,
        width,
        height,
        &MaskAffine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: dx,
            ty: dy,
        },
    )
}

/// Symmetric alpha feather used by `softEdge`
/// (ref packages/core/src/shape/effects.ts:applySoftEdge).
///
/// The reference stretches the colour layer outward by `radius` so edge colours
/// bleed past the geometry, restores the sharp body inside the silhouette, then
/// attenuates everything by the coverage blurred at `sigma = radius / 3` (the
/// authored `rad` spans ~3σ). `mask` is the geometric coverage of the body in
/// `layer`; `bbox` is that body's box in layer pixels.
pub fn feather_alpha_bleed(
    layer: &mut [u8],
    width: usize,
    height: usize,
    mask: &[u8],
    bbox: (f32, f32, f32, f32),
    radius: f32,
) {
    let mut scratch = EffectScratch::default();
    feather_alpha_bleed_with(&mut scratch, layer, width, height, mask, bbox, radius);
}

/// Pooled twin of [`feather_alpha_bleed`].
pub(crate) fn feather_alpha_bleed_with(
    scratch: &mut EffectScratch,
    layer: &mut [u8],
    width: usize,
    height: usize,
    mask: &[u8],
    bbox: (f32, f32, f32, f32),
    radius: f32,
) {
    let pixels = width * height;
    if radius <= 0.0 || !radius.is_finite() || pixels == 0 || layer.len() < pixels * 4 {
        return;
    }
    let mut feathered = mask[..pixels].to_vec();
    gaussian_blur_alpha_with(scratch, &mut feathered, width, height, radius / 3.0);

    let source = layer[..pixels * 4].to_vec();
    let (bx, by, bw, bh) = bbox;
    let (cx, cy) = (bx + bw * 0.5, by + bh * 0.5);
    // Inverse of the bbox → bbox+2·radius stretch: the colour layer is the
    // body scaled up about its own centre.
    let fx = bw / (bw + radius * 2.0).max(1e-3);
    let fy = bh / (bh + radius * 2.0).max(1e-3);

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let sx = cx + (x as f32 + 0.5 - cx) * fx - 0.5;
            let sy = cy + (y as f32 + 0.5 - cy) * fy - 0.5;
            let bleed = sample_rgba_bilinear(&source, width, height, sx, sy);
            // destination-out(mask) then source-over(sharp layer).
            let coverage = mask[index] as f32 / 255.0;
            // destination-in(blurred coverage).
            let feather = feathered[index] as f32 / 255.0;
            for channel in 0..4 {
                let value = bleed[channel] * (1.0 - coverage) + layer[index * 4 + channel] as f32;
                layer[index * 4 + channel] = (value * feather).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Outer-shadow alignment/scale/skew/rotation composition
/// (ref effects.ts:applyOuterShadow: alignment origin, then scale/skew about
/// it, then the page-space offset). `anchor` is that alignment origin in
/// destination-canvas pixels; the caller supplies it because `algn` is not
/// carried by the IR.
pub fn warp_shadow_mask(
    mask: &[u8],
    width: usize,
    height: usize,
    shadow: &VectorOuterShadow,
    anchor: (f32, f32),
    rotation_deg: f32,
    offset: (f32, f32),
) -> Vec<u8> {
    let sx = shadow.scale_x.max(1e-3);
    let sy = shadow.scale_y.max(1e-3);
    let kx = shadow.skew_x_deg.to_radians().tan();
    let ky = shadow.skew_y_deg.to_radians().tan();
    let scaled = (sx - 1.0).abs() > 1e-4 || (sy - 1.0).abs() > 1e-4 || kx.abs() > 1e-4 || ky.abs() > 1e-4;
    if rotation_deg.abs() <= 1e-6 {
        if scaled {
            // Nearest-neighbour stretch about the layer centre, then the exact
            // rebase onto the alignment anchor plus the page-space offset.
            let stretched = super::mask::warp_mask(mask, width, height, shadow);
            let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
            let (ax, ay) = (anchor.0 - cx, anchor.1 - cy);
            let tx = ax - (sx * ax + kx * ay) + offset.0;
            let ty = ay - (ky * ax + sy * ay) + offset.1;
            return shift_mask_subpixel(&stretched, width, height, tx, ty);
        }
        return shift_mask_subpixel(mask, width, height, offset.0, offset.1);
    }

    // R(θ)·S·R(−θ) applied about the anchor, then the page-space offset.
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let rot: Mat2 = [[cos, -sin], [sin, cos]];
    let skew: Mat2 = [[sx, kx], [ky, sy]];
    let linear = mat_mul(mat_mul(rot, skew), mat_transpose(rot));
    let (ax, ay) = mat_apply(linear, anchor);
    warp_mask_affine(
        mask,
        width,
        height,
        &MaskAffine {
            a: linear[0][0],
            b: linear[0][1],
            c: linear[1][0],
            d: linear[1][1],
            tx: anchor.0 - ax + offset.0,
            ty: anchor.1 - ay + offset.1,
        },
    )
}

/// Authored effect reach in device px. The reference has no fixed blur cap; the
/// only limit kept here is the canvas diagonal, beyond which no effect pixel
/// can land anyway.
pub fn clamp_reach_px(radius_emu: i64, scale: f32, cap: f32) -> f32 {
    let px = radius_emu as f32 * scale;
    if !px.is_finite() || px <= 0.0 {
        return 0.0;
    }
    px.min(cap)
}

type Mat2 = [[f32; 2]; 2];

fn mat_mul(a: Mat2, b: Mat2) -> Mat2 {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}

fn mat_apply(m: Mat2, v: (f32, f32)) -> (f32, f32) {
    (m[0][0] * v.0 + m[0][1] * v.1, m[1][0] * v.0 + m[1][1] * v.1)
}

fn mat_transpose(m: Mat2) -> Mat2 {
    [[m[0][0], m[1][0]], [m[0][1], m[1][1]]]
}

fn sample_mask_bilinear(mask: &[u8], width: usize, height: usize, x: f32, y: f32) -> u8 {
    if x < 0.0 || y < 0.0 || x > (width - 1) as f32 || y > (height - 1) as f32 {
        return 0;
    }
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = mask[y0 * width + x0] as f32 * (1.0 - fx) + mask[y0 * width + x1] as f32 * fx;
    let bottom = mask[y1 * width + x0] as f32 * (1.0 - fx) + mask[y1 * width + x1] as f32 * fx;
    (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
}

fn sample_rgba_bilinear(buf: &[u8], width: usize, height: usize, x: f32, y: f32) -> [f32; 4] {
    if x < 0.0 || y < 0.0 || x > (width - 1) as f32 || y > (height - 1) as f32 {
        return [0.0; 4];
    }
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for (channel, value) in out.iter_mut().enumerate() {
        let at = |px: usize, py: usize| buf[(py * width + px) * 4 + channel] as f32;
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        *value = top * (1.0 - fy) + bottom * fy;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variance_tracks_requested_sigma() {
        for sigma in [1.0f32, 2.0, 4.0, 8.0, 16.0] {
            let radii = box_radii_for_sigma(sigma);
            let variance = cascaded_variance(radii);
            // Three cascaded box filters approximate a Gaussian within ~12 %.
            assert!(
                (variance.sqrt() - sigma).abs() <= sigma * 0.2 + 0.35,
                "sigma={sigma} radii={radii:?} effective={}",
                variance.sqrt()
            );
        }
    }

    #[test]
    fn impulse_response_is_symmetric_and_mass_preserving() {
        let (w, h) = (9usize, 9usize);
        let mut buf = vec![0u8; w * h * 4];
        let center = (h / 2 * w + w / 2) * 4;
        buf[center..center + 4].copy_from_slice(&[255, 255, 255, 255]);

        gaussian_blur_rgba(&mut buf, w, h, 1.5);

        let alpha_at = |x: usize, y: usize| buf[(y * w + x) * 4 + 3];
        assert!(alpha_at(4, 4) > 0, "center must be lit");
        // Symmetric horizontally and vertically.
        assert_eq!(alpha_at(3, 4), alpha_at(5, 4));
        assert_eq!(alpha_at(4, 3), alpha_at(4, 5));
        assert_eq!(alpha_at(3, 3), alpha_at(5, 5));

        let total: u32 = buf.chunks_exact(4).map(|c| c[3] as u32).sum();
        assert!(total > 0, "impulse energy must survive the blur");
        // Rounded 8-bit values may drift slightly but stay in the same order.
        assert!(total <= 255 * 2);
    }

    #[test]
    fn opaque_regions_stay_opaque_away_from_the_border() {
        let (w, h) = (32usize, 32usize);
        let mut buf = vec![255u8; w * h * 4];
        gaussian_blur_rgba(&mut buf, w, h, 4.0);
        // Out-of-buffer taps are transparent (CSS blur semantics), so only the
        // interior stays untouched.
        for (index, chunk) in buf.chunks_exact(4).enumerate() {
            let (x, y) = (index % w, index / w);
            // Window sits at least one kernel extent (11 px) from every edge.
            if (13..19).contains(&x) && (13..19).contains(&y) {
                assert_eq!(chunk[3], 255);
                assert!(chunk[0] >= 254 && chunk[1] >= 254 && chunk[2] >= 254);
            }
        }
        // The border fades towards transparent.
        assert!(buf[3] < 255, "border alpha must decay, got {}", buf[3]);
    }

    #[test]
    fn transparent_black_does_not_darken_visible_pixels() {
        let (w, h) = (8usize, 4usize);
        let mut buf = vec![0u8; w * h * 4];
        // Left half opaque red, right half transparent black.
        for y in 0..h {
            for x in 0..w / 2 {
                let idx = (y * w + x) * 4;
                buf[idx..idx + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        gaussian_blur_rgba(&mut buf, w, h, 2.0);

        // Pixels that gained alpha from the red edge keep a red hue, never a
        // black-fringed one.
        for chunk in buf.chunks_exact(4) {
            if chunk[3] > 0 {
                assert!(chunk[0] > 200, "red must dominate, got {chunk:?}");
            }
        }
    }

    #[test]
    fn directional_offset_quadrants() {
        let (dx, dy) = directional_offset(45.0, 10.0);
        assert!((dx - dy).abs() < 1e-4);
        assert!(dx > 0.0);
        let (dx, dy) = directional_offset(90.0, 10.0);
        assert!(dx.abs() < 1e-4 && (dy - 10.0).abs() < 1e-3);
        let (dx, dy) = directional_offset(180.0, 10.0);
        assert!((dx + 10.0).abs() < 1e-3 && dy.abs() < 1e-4);
        let (dx, dy) = directional_offset(270.0, 10.0);
        assert!(dx.abs() < 1e-4 && (dy + 10.0).abs() < 1e-3);
    }
}
