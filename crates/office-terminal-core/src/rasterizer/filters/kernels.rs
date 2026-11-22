//! Box-filter kernels and premultiplication helpers for the Gaussian blur
//! approximation in [`super`]. The sliding-window prefix sums are supplied by
//! the caller ([`crate::rasterizer::scratch::EffectScratch::prefix`]) so a
//! whole raster pass reuses one buffer; the sums are fully rewritten before
//! every read, so pooled contents never leak between shapes.

pub(super) fn box_blur_horizontal_rgba(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    prefix: &mut [u32],
) {
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
                let count = (radius * 2 + 1) as u32;
                let sum = prefix[hi + 1] - prefix[lo];
                dst[(row + x) * 4 + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
}

pub(super) fn box_blur_vertical_rgba(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    prefix: &mut [u32],
) {
    for x in 0..width {
        for channel in 0..4 {
            prefix[0] = 0;
            for y in 0..height {
                prefix[y + 1] = prefix[y] + src[(y * width + x) * 4 + channel] as u32;
            }
            for y in 0..height {
                let lo = y.saturating_sub(radius);
                let hi = (y + radius).min(height - 1);
                let count = (radius * 2 + 1) as u32;
                let sum = prefix[hi + 1] - prefix[lo];
                dst[(y * width + x) * 4 + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
}

pub(super) fn box_blur_horizontal_alpha(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    prefix: &mut [u32],
) {
    for y in 0..height {
        let row = y * width;
        prefix[0] = 0;
        for x in 0..width {
            prefix[x + 1] = prefix[x] + src[row + x] as u32;
        }
        for x in 0..width {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(width - 1);
            let count = (radius * 2 + 1) as u32;
            let sum = prefix[hi + 1] - prefix[lo];
            dst[row + x] = ((sum + count / 2) / count) as u8;
        }
    }
}

pub(super) fn box_blur_vertical_alpha(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    prefix: &mut [u32],
) {
    for x in 0..width {
        prefix[0] = 0;
        for y in 0..height {
            prefix[y + 1] = prefix[y] + src[y * width + x] as u32;
        }
        for y in 0..height {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius).min(height - 1);
            let count = (radius * 2 + 1) as u32;
            let sum = prefix[hi + 1] - prefix[lo];
            dst[y * width + x] = ((sum + count / 2) / count) as u8;
        }
    }
}

#[allow(clippy::chunks_exact_to_as_chunks)]
pub(super) fn premultiply(buf: &mut [u8]) {
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

pub(super) fn unpremultiply(buf: &mut [u8]) {
    for chunk in buf.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a > 0 && a < 255 {
            chunk[0] = ((chunk[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            chunk[1] = ((chunk[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            chunk[2] = ((chunk[2] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
}
