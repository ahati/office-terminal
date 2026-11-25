//! Office bevel profile curves (S7a).
//!
//! Port of `packages/core/src/shape/bevel-profiles.ts`: the 12 documented
//! MS-OI29500 §2.1.1316 control-point sets plus the default `circle` fallback.
//! Each segment is a line/quadratic/cubic in `(inward, height)`; the curve is
//! sampled at a fixed resolution, normalised to its own peak height, and the
//! front-view surface selects the greatest height at each inward distance.

use std::collections::HashMap;
use std::sync::OnceLock;

const RESOLUTION: usize = 1024;
const STEPS: usize = 256;

/// Control points copied verbatim from the reference table.
const CURVES: &[(&str, &[&[f32]])] = &[
    ("angle", &[&[1.0, 1.0]]),
    (
        "artDeco",
        &[
            &[0.0, 0.184095, 0.149238, 0.333333, 0.333333, 0.333333],
            &[0.333333, 0.701523, 0.63181, 1.0, 1.0, 1.0],
        ],
    ),
    ("circle", &[&[0.0, 0.55627, 0.44373, 1.0, 1.0, 1.0]]),
    (
        "convex",
        &[
            &[0.0, 0.07082, 0.0297458, 0.1, 0.101416, 0.1],
            &[0.501416, 0.1, 0.9, 0.7, 0.901416, 0.899999],
            &[0.9, 0.97167, 0.93343, 1.0, 1.0, 1.0],
        ],
    ),
    (
        "coolSlant",
        &[
            &[0.0, 0.138122, 0.0, 0.2, 0.271356, 0.775535],
            &[0.377654, 1.0, 0.519455, 1.0, 0.583589, 1.0],
        ],
    ),
    (
        "cross",
        &[
            &[0.0, 0.05563, 0.04437, 0.1, 0.1, 0.1],
            &[0.4, 0.1],
            &[0.45563, 0.1, 0.5, 0.14437, 0.5, 0.2],
            &[0.5, 0.9],
            &[0.5, 0.95563, 0.54437, 1.0, 0.6, 1.0],
        ],
    ),
    (
        "divot",
        &[
            &[0.0, 0.236604, 0.119276, 0.607024, 0.263046, 0.760235],
            &[0.361098, 0.864726, 0.457934, 0.909567, 0.537806, 0.925082],
            &[0.559245, 0.929246, 0.567625, 0.89793, 0.542066, 0.845567],
            &[0.484073, 0.726757, 0.477103, 0.393693, 0.551651, 0.393693],
            &[0.899894, 0.393693],
            &[0.99148, 0.393693, 0.958466, 0.746659, 0.907348, 0.779629],
            &[0.879394, 0.797658],
            &[0.845545, 0.819489, 0.848775, 0.954172, 0.874334, 0.971627],
            &[0.915883, 1.0, 1.0, 1.0],
        ],
    ),
    (
        "hardEdge",
        &[
            &[0.0, 0.092437, 0.042353, 0.305322],
            &[0.170124, 0.947558],
            &[0.177203, 0.983142, 0.2, 1.0, 0.268235, 0.998599],
            &[0.614118, 0.998599],
            &[0.647059, 0.998599, 0.656471, 0.987395, 0.663529, 0.969188],
            &[0.687006, 0.908633, 0.802353, 0.822129, 1.0, 0.822129],
        ],
    ),
    (
        "relaxedInset",
        &[
            &[0.0, 0.367, 0.124605, 0.82, 0.507899, 1.0],
            &[0.724115, 0.737, 0.790455, 0.64, 1.0, 0.64],
        ],
    ),
    (
        "riblet",
        &[
            &[0.0, 0.238519, 0.132047, 0.500741, 0.357567, 0.731852],
            &[0.513167, 0.891311, 0.563798, 0.912593, 0.735905, 0.912593],
            &[0.873887, 0.912593],
            &[0.956973, 0.912593, 0.878338, 1.0, 1.0, 1.0],
        ],
    ),
    (
        "slope",
        &[
            &[0.0, 0.125, 0.025, 0.25],
            &[0.125, 0.75],
            &[0.175, 1.0, 0.25, 1.0, 0.375, 1.0],
            &[0.5, 1.0],
            &[0.625, 1.0, 0.7, 1.0, 0.75, 0.75],
            &[0.875, 0.125],
            &[0.9, 0.01, 0.98, 0.01, 1.0, 0.01],
        ],
    ),
    (
        "softRound",
        &[
            &[0.0, 0.4775, 0.0968736, 1.0, 0.156301, 1.0],
            &[0.264179, 1.0, 0.376919, 0.33333, 1.0, 0.33333],
        ],
    ),
];

struct Profile {
    height: Vec<f32>,
    slope: Vec<f32>,
}

fn compile(segments: &[&[f32]]) -> Profile {
    let mut height = vec![f32::NEG_INFINITY; RESOLUTION + 1];
    let mut slope = vec![0.0f32; RESOLUTION + 1];
    let mut xs: Vec<f32> = vec![0.0];
    let mut ys: Vec<f32> = vec![0.0];
    let (mut x0, mut y0) = (0.0f32, 0.0f32);
    for segment in segments {
        let degree = segment.len() / 2;
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (x, y) = match degree {
                1 => (u * x0 + t * segment[0], u * y0 + t * segment[1]),
                2 => (
                    u * u * x0 + 2.0 * u * t * segment[0] + t * t * segment[2],
                    u * u * y0 + 2.0 * u * t * segment[1] + t * t * segment[3],
                ),
                _ => (
                    u * u * u * x0
                        + 3.0 * u * u * t * segment[0]
                        + 3.0 * u * t * t * segment[2]
                        + t * t * t * segment[4],
                    u * u * u * y0
                        + 3.0 * u * u * t * segment[1]
                        + 3.0 * u * t * t * segment[3]
                        + t * t * t * segment[5],
                ),
            };
            xs.push(x);
            ys.push(y);
        }
        x0 = segment[segment.len() - 2];
        y0 = segment[segment.len() - 1];
    }
    let max_y = ys.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    for k in 1..xs.len() {
        let (px, x) = (xs[k - 1], xs[k]);
        let py = ys[k - 1] / max_y;
        let (dx, dy) = (x - px, ys[k] / max_y - py);
        if dx == 0.0 {
            continue;
        }
        let lo = (px.min(x) * RESOLUTION as f32).ceil().clamp(0.0, RESOLUTION as f32) as usize;
        let hi = (px.max(x) * RESOLUTION as f32).floor().clamp(0.0, RESOLUTION as f32) as usize;
        for i in lo..=hi {
            let z = py + ((i as f32 / RESOLUTION as f32 - px) * dy) / dx;
            if z > height[i] {
                height[i] = z;
                slope[i] = dy / dx;
            }
        }
    }
    for i in 0..=RESOLUTION {
        if i as f32 / RESOLUTION as f32 >= x0 {
            height[i] = y0 / max_y;
            slope[i] = 0.0;
        }
    }
    Profile { height, slope }
}

fn profiles() -> &'static HashMap<&'static str, Profile> {
    static PROFILES: OnceLock<HashMap<&'static str, Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        CURVES
            .iter()
            .map(|(name, segments)| (*name, compile(segments)))
            .collect()
    })
}

fn known(name: &str) -> &'static str {
    CURVES
        .iter()
        .map(|(name, _)| *name)
        .find(|candidate| *candidate == name)
        .unwrap_or("circle")
}

fn sample(values: &[f32], t: f32) -> f32 {
    let u = t.clamp(0.0, 1.0) * RESOLUTION as f32;
    let i = (u as usize).min(RESOLUTION - 1);
    values[i] + (values[i + 1] - values[i]) * (u - i as f32)
}

/// Height of the normalized profile at `t` in `[0, 1]`.
pub fn profile_height(name: &str, t: f32) -> f32 {
    sample(&profiles()[known(name)].height, t)
}

/// Slope `d(height)/d(t)` of the normalized profile at `t`.
pub fn profile_slope(name: &str, t: f32) -> f32 {
    sample(&profiles()[known(name)].slope, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_presets_fall_back_to_circle() {
        for name in ["unknown", "__proto__", "toString", "constructor"] {
            for t in [0.0, 0.25, 0.5, 1.0] {
                assert_eq!(profile_height(name, t), profile_height("circle", t));
                assert_eq!(profile_slope(name, t), profile_slope("circle", t));
            }
        }
        assert_ne!(profile_height("angle", 0.5), profile_height("circle", 0.5));
    }
}
