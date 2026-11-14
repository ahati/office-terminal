//! Bevel normal-field shading (PRD 08 §3.6.3).
//!
//! Approximates the bevel surface as a distance-field profile around the
//! shape boundary, evaluates the preset profile curve, and applies Blinn-Phong
//! material lighting to the rendered layer.

use crate::contracts::{ShapeEffects, VectorShape3d};
use crate::scene3d;

#[allow(clippy::chunks_exact_to_as_chunks)]
pub(crate) fn apply_bevel_shading(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    shape_3d: &VectorShape3d,
    effects: &ShapeEffects,
    scale: f32,
) {
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return;
    }
    // The layer is already at device resolution, so `scale` (EMU → device px)
    // plays the role of the reference's `scale × devScale`; `devScale` is 1.
    // ref renderer.ts:6302-6330 — no band clamp, and `w > 0 && h > 0` gates the
    // whole lip, so a zero-sized bevel shades nothing.
    let inputs = scene3d::build_bevel_inputs(Some(shape_3d), effects.scene_3d.as_ref(), scale as f64, 1.0);
    // The rasterizer paints a single planar layer (no rear plane), so only the
    // front lip is shaded; the bottom lip additionally needs the back-face
    // surface geometry (MS-OI29500 §2.1.1268) that this layer does not build.
    let Some(bevel) = inputs.iter().find(|bevel| !bevel.bottom) else {
        return;
    };
    let band_px = bevel.width_px as f32;
    if band_px <= 0.0 {
        return;
    }
    let preset = bevel.preset.as_str();
    let profile_height = (bevel.height_px as f32).max(1.0);
    let material = scene3d::Material::from_preset(bevel.material.as_str());
    let light = [bevel.light[0] as f32, bevel.light[1] as f32, bevel.light[2] as f32];

    let inside: Vec<bool> = pixels.chunks_exact(4).map(|c| c[3] > 8).collect();
    let dist = distance_field(&inside, width, height);
    let flat = scene3d::blinn_phong([0.0, 0.0, 1.0], light, material).max(0.05);

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !inside[idx] || dist[idx] >= band_px {
                continue;
            }
            let u = (dist[idx] / band_px).clamp(0.0, 1.0);
            let (_, slope) = scene3d::bevel_curve(preset, u, profile_height);
            let gx = (sample(&dist, width, height, x + 1, y) - sample(&dist, width, height, x - 1, y)) * 0.5;
            let gy = (sample(&dist, width, height, x, y + 1) - sample(&dist, width, height, x, y - 1)) * 0.5;
            let normal = scene3d::bevel_normal(slope.min(8.0), (gx, gy));
            let intensity = scene3d::blinn_phong(normal, light, material);
            let factor = (intensity / flat).clamp(0.25, 2.0);

            let base = idx * 4;
            let alpha = pixels[base + 3] as f32;
            for channel in 0..3 {
                // Keep the premultiplied RGBA invariant: RGB may never exceed
                // the alpha channel.
                pixels[base + channel] = (pixels[base + channel] as f32 * factor)
                    .round()
                    .clamp(0.0, alpha) as u8;
            }
        }
    }
}

fn sample(dist: &[f32], width: usize, height: usize, x: usize, y: usize) -> f32 {
    if x >= width || y >= height {
        0.0
    } else {
        dist[y * width + x]
    }
}

/// Two-pass chamfer distance transform (distance from the shape boundary).
fn distance_field(inside: &[bool], width: usize, height: usize) -> Vec<f32> {
    const FAR: f32 = 1_000_000.0;
    let mut dist = vec![FAR; width * height];
    for (i, &is_inside) in inside.iter().enumerate() {
        if !is_inside {
            dist[i] = 0.0;
        }
    }
    let diag = std::f32::consts::SQRT_2;
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let mut best = dist[i];
            if x > 0 {
                best = best.min(dist[i - 1] + 1.0);
            }
            if y > 0 {
                best = best.min(dist[i - width] + 1.0);
                if x > 0 {
                    best = best.min(dist[i - width - 1] + diag);
                }
                if x + 1 < width {
                    best = best.min(dist[i - width + 1] + diag);
                }
            }
            dist[i] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let i = y * width + x;
            let mut best = dist[i];
            if x + 1 < width {
                best = best.min(dist[i + 1] + 1.0);
            }
            if y + 1 < height {
                best = best.min(dist[i + width] + 1.0);
                if x + 1 < width {
                    best = best.min(dist[i + width + 1] + diag);
                }
                if x > 0 {
                    best = best.min(dist[i + width - 1] + diag);
                }
            }
            dist[i] = best;
        }
    }
    dist
}
