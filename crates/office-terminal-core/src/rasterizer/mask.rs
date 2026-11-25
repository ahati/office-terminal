//! Alpha-mask helpers shared by the shape-effect passes: tinting, offsetting,
//! scale/skew warping, and scratch-pixmap construction.

use crate::contracts::VectorOuterShadow;

#[allow(clippy::chunks_exact_to_as_chunks)]
pub(crate) fn alpha_mask(data: &[u8]) -> Vec<u8> {
    data.chunks_exact(4).map(|c| c[3]).collect()
}

pub(crate) fn tint_mask(
    mask: &[u8],
    width: usize,
    height: usize,
    color: [u8; 4],
    alpha_map: impl Fn(f32) -> f32,
) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32)?;
    let data = pixmap.data_mut();
    let color_alpha = color[3] as f32 / 255.0;
    for (i, &value) in mask.iter().enumerate() {
        let alpha = (value as f32 / 255.0) * color_alpha;
        let alpha = alpha_map(alpha).clamp(0.0, 1.0);
        let base = i * 4;
        data[base] = (color[0] as f32 * alpha).round() as u8;
        data[base + 1] = (color[1] as f32 * alpha).round() as u8;
        data[base + 2] = (color[2] as f32 * alpha).round() as u8;
        data[base + 3] = (alpha * 255.0).round() as u8;
    }
    Some(pixmap)
}

pub(crate) fn pixmap_from_premultiplied(data: &[u8], width: usize, height: usize) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32)?;
    pixmap.data_mut().copy_from_slice(data);
    Some(pixmap)
}

pub(crate) fn shift_mask(mask: &[u8], width: usize, height: usize, dx: i32, dy: i32) -> Vec<u8> {
    let mut out = vec![0u8; width * height];
    if dx.unsigned_abs() as usize >= width || dy.unsigned_abs() as usize >= height {
        return out;
    }
    for y in 0..height {
        let sy = y as i32 - dy;
        if sy < 0 || sy >= height as i32 {
            continue;
        }
        for x in 0..width {
            let sx = x as i32 - dx;
            if sx < 0 || sx >= width as i32 {
                continue;
            }
            out[y * width + x] = mask[sy as usize * width + sx as usize];
        }
    }
    out
}

/// Apply the outer-shadow scale/skew matrix (PRD 08 §3.1) around the mask
/// centre using nearest-neighbour sampling.
pub(crate) fn warp_mask(mask: &[u8], width: usize, height: usize, shadow: &VectorOuterShadow) -> Vec<u8> {
    let sx = shadow.scale_x.max(1e-3);
    let sy = shadow.scale_y.max(1e-3);
    let kx = shadow.skew_x_deg.to_radians().tan();
    let ky = shadow.skew_y_deg.to_radians().tan();
    let det = sx * sy - kx * ky;
    if det.abs() < 1e-4 {
        return mask.to_vec();
    }
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 - cx;
            let py = y as f32 - cy;
            // Inverse of [[sx, kx], [ky, sy]].
            let ux = (sy * px - kx * py) / det + cx;
            let uy = (-ky * px + sx * py) / det + cy;
            if ux < 0.0 || uy < 0.0 || ux >= width as f32 || uy >= height as f32 {
                continue;
            }
            out[y * width + x] = mask[uy as usize * width + ux as usize];
        }
    }
    out
}
