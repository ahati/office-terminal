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
    let camera_input = effects.scene_3d.as_ref().map(scene3d::CameraInput::from_scene);
    let (rotation, back_facing) = camera_input
        .as_ref()
        .map(scene3d::scene3d_surface_frame)
        .unwrap_or((
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            false,
        ));

    let inputs = scene3d::build_bevel_inputs(Some(shape_3d), effects.scene_3d.as_ref(), scale as f64, 1.0);
    // When the camera exposes the back face, the bottom bevel is shaded using
    // its surface frame rotation (MS-OI29500 §2.1.1268); otherwise the front
    // lip is shaded.
    let Some(bevel) = inputs.iter().find(|bevel| bevel.bottom == back_facing) else {
        return;
    };
    let band_px = bevel.width_px as f32;
    if band_px <= 0.0 {
        return;
    }
    let preset = bevel.preset.as_str();
    let profile_height = (bevel.height_px as f32).max(1.0);

    let rig = scene3d::lighting::rig_vector(
        effects.scene_3d.as_ref().map(|s| s.light_rig_type.as_str()).unwrap_or("threePt"),
        effects.scene_3d.as_ref().map(|s| s.light_dir.as_str()).unwrap_or("t"),
        effects.scene_3d.as_ref().and_then(|s| s.light_rot.as_ref()),
    );
    let lighting = scene3d::lighting::prepare_lighting(bevel.material.as_str(), &rig);

    let inside: Vec<bool> = pixels.chunks_exact(4).map(|c| c[3] > 8).collect();
    let dist = distance_field(&inside, width, height);

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
            let (nx, ny, nz) = if bevel.bottom {
                let rx = rotation[0] * normal[0] as f64 + rotation[1] * normal[1] as f64 - rotation[2] * normal[2] as f64;
                let ry = rotation[3] * normal[0] as f64 + rotation[4] * normal[1] as f64 - rotation[5] * normal[2] as f64;
                let rz = rotation[6] * normal[0] as f64 + rotation[7] * normal[1] as f64 - rotation[8] * normal[2] as f64;
                (rx as f32, ry as f32, rz as f32)
            } else {
                (normal[0], normal[1], normal[2])
            };

            let base = idx * 4;
            let pa = pixels[base + 3] as f32;
            if pa <= 0.0 {
                continue;
            }
            let inv_pa = 255.0 / pa;
            let r = (pixels[base] as f32 * inv_pa).min(255.0);
            let g = (pixels[base + 1] as f32 * inv_pa).min(255.0);
            let b = (pixels[base + 2] as f32 * inv_pa).min(255.0);

            let (fnx, fny, fnz) = if bevel.bottom {
                (-rotation[2] as f32, -rotation[5] as f32, -rotation[8] as f32)
            } else {
                (0.0, 0.0, 1.0)
            };

            let flat_col = scene3d::lighting::shade_surface(r, g, b, pa, fnx, fny, fnz, &lighting);
            let bevel_col = scene3d::lighting::shade_surface(r, g, b, pa, nx, ny, nz, &lighting);

            for c in 0..3 {
                let factor = if flat_col[c] > 1.0 {
                    (bevel_col[c] / flat_col[c]).clamp(0.1, 8.0)
                } else if bevel_col[c] > 0.0 {
                    1.0 + (bevel_col[c] / 255.0)
                } else {
                    1.0
                };
                let new_ch = (pixels[base + c] as f32 * factor).round().clamp(0.0, pa) as u8;
                pixels[base + c] = new_ch;
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
