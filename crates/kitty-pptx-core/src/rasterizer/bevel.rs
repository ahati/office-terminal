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
    let bevel_w_px = (shape_3d.bevel_top_width_emu.max(shape_3d.bevel_top_height_emu) as f32 * scale)
        .clamp(0.0, 48.0);
    if bevel_w_px < 1.0 || width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return;
    }
    let bevel_h_px = (shape_3d.bevel_top_height_emu as f32 * scale).max(1.0);
    let preset = if shape_3d.bevel_top_type.is_empty() {
        "circle"
    } else {
        shape_3d.bevel_top_type.as_str()
    };
    let light_deg = effects
        .scene_3d
        .as_ref()
        .map(|s| s.light_direction_deg)
        .unwrap_or(225.0);
    let material = scene3d::Material::from_preset("plastic");

    let inside: Vec<bool> = pixels.chunks_exact(4).map(|c| c[3] > 8).collect();
    let dist = distance_field(&inside, width, height);
    let light = scene3d::light_vector(light_deg);
    let flat = scene3d::blinn_phong([0.0, 0.0, 1.0], light, material).max(0.05);

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !inside[idx] || dist[idx] >= bevel_w_px {
                continue;
            }
            let u = (dist[idx] / bevel_w_px).clamp(0.0, 1.0);
            let (_, slope) = scene3d::bevel_curve(preset, u, bevel_h_px);
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
