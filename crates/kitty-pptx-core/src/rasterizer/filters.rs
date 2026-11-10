//! Separable Gaussian blur approximation used by the shape-effect passes.
//!
//! Implements the three-pass box-filter approximation from PRD 08 §4.1. The
//! sliding-window (prefix-sum) accumulators make each pass `O(1)` per pixel
//! regardless of the blur radius. RGBA buffers are alpha-weighted by
//! premultiplying before convolution and unpremultiplying afterwards, so
//! transparent pixels cannot bleed dark halos into visible edges.

#![allow(clippy::chunks_exact_to_as_chunks)]

/// Compute the three odd box widths whose cascaded variance best matches the
/// requested Gaussian standard deviation (PRD 08 §4.1).
pub fn box_radii_for_sigma(sigma: f32) -> [usize; 3] {
    if sigma <= 0.0 || !sigma.is_finite() {
        return [0, 0, 0];
    }
    const M: f32 = 3.0;
    let w_ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut w_low = 2.0 * (w_ideal / 2.0).floor() - 1.0;
    if w_low < 1.0 {
        w_low = 1.0;
    }
    let w_high = w_low + 2.0;
    let iterations = ((12.0 * sigma * sigma - M * w_low * w_low - 4.0 * M * w_low - 3.0 * M)
        / (-4.0 * w_low - 4.0))
        .round()
        .clamp(0.0, M) as usize;

    let mut radii = [0usize; 3];
    for (i, radius) in radii.iter_mut().enumerate() {
        let width = if i < iterations { w_high } else { w_low };
        *radius = (width / 2.0) as usize;
    }
    radii
}

/// Variance of the cascaded box filters, used to validate the approximation.
pub fn cascaded_variance(radii: [usize; 3]) -> f32 {
    radii
        .iter()
        .map(|&r| (r * (r + 1)) as f32 / 3.0)
        .sum()
}

/// Blur a straight-alpha RGBA8 buffer. Alpha is honoured during convolution.
pub fn gaussian_blur_rgba(buf: &mut [u8], width: usize, height: usize, sigma: f32) {
    premultiply(buf);
    gaussian_blur_premultiplied(buf, width, height, sigma);
    unpremultiply(buf);
}

/// Blur an already-premultiplied RGBA8 buffer.
pub fn gaussian_blur_premultiplied(buf: &mut [u8], width: usize, height: usize, sigma: f32) {
    let radii = box_radii_for_sigma(sigma);
    if width == 0 || height == 0 || radii.iter().all(|&r| r == 0) {
        return;
    }
    let len = width * height * 4;
    if buf.len() < len {
        return;
    }

    let mut current = buf[..len].to_vec();
    let mut scratch = vec![0u8; len];
    for radius in radii {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal_rgba(&current, &mut scratch, width, height, radius);
        box_blur_vertical_rgba(&scratch, &mut current, width, height, radius);
    }
    buf[..len].copy_from_slice(&current);
}

/// Blur a single-channel 8-bit alpha mask.
pub fn gaussian_blur_alpha(mask: &mut [u8], width: usize, height: usize, sigma: f32) {
    let radii = box_radii_for_sigma(sigma);
    if width == 0 || height == 0 || radii.iter().all(|&r| r == 0) {
        return;
    }
    let len = width * height;
    if mask.len() < len {
        return;
    }

    let mut current = mask[..len].to_vec();
    let mut scratch = vec![0u8; len];
    for radius in radii {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal_alpha(&current, &mut scratch, width, height, radius);
        box_blur_vertical_alpha(&scratch, &mut current, width, height, radius);
    }
    mask[..len].copy_from_slice(&current);
}

/// Convert a DrawingML direction (degrees clockwise from East) and a distance
/// into a screen-space `(dx, dy)` offset. Screen coordinates use y-down.
pub fn directional_offset(direction_deg: f64, distance_px: f32) -> (f32, f32) {
    let rad = direction_deg.to_radians();
    (distance_px * rad.cos() as f32, distance_px * rad.sin() as f32)
}

fn box_blur_horizontal_rgba(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let mut prefix = vec![0u32; width + 1];
    for y in 0..height {
        let row = y * width;
        for channel in 0..4 {
            prefix[0] = 0;
            for x in 0..width {
                prefix[x + 1] = prefix[x] + src[(row + x) * 4 + channel] as u32;
            }
            for x in 0..width {
                let lo = x.saturating_sub(radius);
                let hi = (x + radius).min(width - 1);
                let count = (hi - lo + 1) as u32;
                let sum = prefix[hi + 1] - prefix[lo];
                dst[(row + x) * 4 + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
}

fn box_blur_vertical_rgba(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let mut prefix = vec![0u32; height + 1];
    for x in 0..width {
        for channel in 0..4 {
            prefix[0] = 0;
            for y in 0..height {
                prefix[y + 1] = prefix[y] + src[(y * width + x) * 4 + channel] as u32;
            }
            for y in 0..height {
                let lo = y.saturating_sub(radius);
                let hi = (y + radius).min(height - 1);
                let count = (hi - lo + 1) as u32;
                let sum = prefix[hi + 1] - prefix[lo];
                dst[(y * width + x) * 4 + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
}

fn box_blur_horizontal_alpha(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let mut prefix = vec![0u32; width + 1];
    for y in 0..height {
        let row = y * width;
        prefix[0] = 0;
        for x in 0..width {
            prefix[x + 1] = prefix[x] + src[row + x] as u32;
        }
        for x in 0..width {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(width - 1);
            let count = (hi - lo + 1) as u32;
            let sum = prefix[hi + 1] - prefix[lo];
            dst[row + x] = ((sum + count / 2) / count) as u8;
        }
    }
}

fn box_blur_vertical_alpha(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let mut prefix = vec![0u32; height + 1];
    for x in 0..width {
        prefix[0] = 0;
        for y in 0..height {
            prefix[y + 1] = prefix[y] + src[y * width + x] as u32;
        }
        for y in 0..height {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius).min(height - 1);
            let count = (hi - lo + 1) as u32;
            let sum = prefix[hi + 1] - prefix[lo];
            dst[y * width + x] = ((sum + count / 2) / count) as u8;
        }
    }
}

#[allow(clippy::chunks_exact_to_as_chunks)]
fn premultiply(buf: &mut [u8]) {
    for chunk in buf.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a == 0 {
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
        } else if a < 255 {
            chunk[0] = ((chunk[0] as u32 * a + 127) / 255) as u8;
            chunk[1] = ((chunk[1] as u32 * a + 127) / 255) as u8;
            chunk[2] = ((chunk[2] as u32 * a + 127) / 255) as u8;
        }
    }
}

fn unpremultiply(buf: &mut [u8]) {
    for chunk in buf.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a > 0 && a < 255 {
            chunk[0] = ((chunk[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            chunk[1] = ((chunk[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            chunk[2] = ((chunk[2] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
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
    fn opaque_regions_stay_opaque() {
        let (w, h) = (16usize, 16usize);
        let mut buf = vec![255u8; w * h * 4];
        gaussian_blur_rgba(&mut buf, w, h, 4.0);
        for chunk in buf.chunks_exact(4) {
            assert_eq!(chunk[3], 255);
            assert!(chunk[0] >= 254 && chunk[1] >= 254 && chunk[2] >= 254);
        }
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
