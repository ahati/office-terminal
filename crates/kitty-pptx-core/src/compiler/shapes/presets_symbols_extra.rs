//! Secondary symbol silhouettes: sun, moon, smiley, waves, hand-drawn seals,
//! ribbons and scroll containers.
//!
//! These are Tier-2 stand-ins in the PRD 11 sense: they preserve the bounding
//! box and the dominant contour of the ECMA-376 presets without reproducing
//! every guide equation.

use crate::contracts::{FillRule, VectorPath};

use super::util::{norm_adj, push_ellipse, push_polygon, push_round_rect, sample_arc};

pub fn build_symbol_extra_preset(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    match geometry {
        "sun" => Some(sun_path(w, h)),
        "moon" => Some(moon_path(w, h)),
        "smileyFace" => Some(smiley_path(w, h)),
        "wave" => Some(wave_path(w, h, 1.0)),
        "doubleWave" => Some(wave_path(w, h, 2.0)),
        "irregularSeal1" => Some(seal_path(w, h, SEAL1)),
        "irregularSeal2" => Some(seal_path(w, h, SEAL2)),
        "ribbon" => Some(ribbon_path(w, h)),
        "ribbon2" => Some(ribbon2_path(w, h)),
        "ellipseRibbon" => Some(ellipse_ribbon_path(w, h, adj, false)),
        "ellipseRibbon2" => Some(ellipse_ribbon_path(w, h, adj, true)),
        "leftRightRibbon" => Some(left_right_ribbon_path(w, h)),
        "verticalScroll" => Some(scroll_path(w, h, true)),
        "horizontalScroll" => Some(scroll_path(w, h, false)),
        _ => None,
    }
}

fn sun_path(w: f32, h: f32) -> VectorPath {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = 0.3 * w.min(h);
    let mut segs = Vec::new();
    push_ellipse(&mut segs, cx, cy, r, r, true);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let base = r * 1.05;
        let apex = r * 1.55;
        let spread = 0.32;
        push_polygon(
            &mut segs,
            &[
                (
                    cx + base * (a - spread).cos(),
                    cy + base * (a - spread).sin(),
                ),
                (cx + apex * a.cos(), cy + apex * a.sin()),
                (
                    cx + base * (a + spread).cos(),
                    cy + base * (a + spread).sin(),
                ),
            ],
        );
    }
    VectorPath::new(segs, FillRule::NonZero)
}

fn moon_path(w: f32, h: f32) -> VectorPath {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let mut points = sample_arc(cx, cy, rx, ry, -70.0, 140.0, 24);
    points.extend(sample_arc(
        w * 0.42,
        cy,
        rx * 0.8,
        ry * 0.85,
        70.0,
        -140.0,
        24,
    ));
    super::util::polygon(&points, FillRule::NonZero)
}

fn smiley_path(w: f32, h: f32) -> VectorPath {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let eye_r = 0.07 * w.min(h);
    let mouth_cy = h * 0.45;
    let mouth_outer = 0.32 * w.min(h);
    let mouth_inner = 0.24 * w.min(h);

    let mut segs = Vec::new();
    push_ellipse(&mut segs, cx, cy, rx, ry, true);
    push_ellipse(&mut segs, cx - 0.2 * w, h * 0.32, eye_r, eye_r, true);
    push_ellipse(&mut segs, cx + 0.2 * w, h * 0.32, eye_r, eye_r, true);
    let mut mouth = sample_arc(cx, mouth_cy, mouth_outer, mouth_outer, 30.0, 120.0, 16);
    mouth.extend(sample_arc(
        cx,
        mouth_cy,
        mouth_inner,
        mouth_inner,
        150.0,
        -120.0,
        16,
    ));
    push_polygon(&mut segs, &mouth);
    VectorPath::new(segs, FillRule::EvenOdd)
}

fn wave_path(w: f32, h: f32, cycles: f32) -> VectorPath {
    let steps = 32;
    let mut points = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let x = w * i as f32 / steps as f32;
        let phase = std::f32::consts::TAU * cycles * x / w.max(f32::EPSILON);
        points.push((x, h * (0.15 + 0.1 * phase.sin())));
    }
    for i in (0..=steps).rev() {
        let x = w * i as f32 / steps as f32;
        let phase = std::f32::consts::TAU * cycles * x / w.max(f32::EPSILON);
        points.push((x, h * (0.85 + 0.1 * phase.sin())));
    }
    super::util::polygon(&points, FillRule::NonZero)
}

/// Normalized outline of the hand-drawn `irregularSeal1` preset.
const SEAL1: &[(f32, f32)] = &[
    (0.50, 0.00),
    (0.61, 0.13),
    (0.78, 0.06),
    (0.78, 0.22),
    (0.95, 0.28),
    (0.86, 0.42),
    (1.00, 0.55),
    (0.85, 0.63),
    (0.90, 0.80),
    (0.72, 0.78),
    (0.65, 0.95),
    (0.50, 0.85),
    (0.36, 0.98),
    (0.27, 0.80),
    (0.10, 0.85),
    (0.14, 0.66),
    (0.00, 0.55),
    (0.13, 0.42),
    (0.04, 0.26),
    (0.22, 0.24),
    (0.24, 0.08),
    (0.39, 0.16),
];

/// Normalized outline of the hand-drawn `irregularSeal2` preset.
const SEAL2: &[(f32, f32)] = &[
    (0.52, 0.00),
    (0.66, 0.10),
    (0.83, 0.04),
    (0.86, 0.21),
    (1.00, 0.38),
    (0.88, 0.50),
    (0.96, 0.66),
    (0.80, 0.72),
    (0.82, 0.90),
    (0.64, 0.88),
    (0.55, 1.00),
    (0.41, 0.87),
    (0.25, 0.93),
    (0.20, 0.75),
    (0.04, 0.70),
    (0.12, 0.54),
    (0.00, 0.40),
    (0.15, 0.30),
    (0.10, 0.13),
    (0.28, 0.14),
    (0.36, 0.02),
];

fn seal_path(w: f32, h: f32, outline: &[(f32, f32)]) -> VectorPath {
    let points: Vec<(f32, f32)> = outline.iter().map(|&(x, y)| (x * w, y * h)).collect();
    super::util::polygon(&points, FillRule::NonZero)
}

fn ribbon_path(w: f32, h: f32) -> VectorPath {
    let points = vec![
        (0.0, 0.35 * h),
        (0.15 * w, 0.35 * h),
        (0.15 * w, 0.25 * h),
        (0.85 * w, 0.25 * h),
        (0.85 * w, 0.35 * h),
        (w, 0.35 * h),
        (0.92 * w, 0.5 * h),
        (w, 0.65 * h),
        (0.85 * w, 0.65 * h),
        (0.85 * w, 0.75 * h),
        (0.15 * w, 0.75 * h),
        (0.15 * w, 0.65 * h),
        (0.0, 0.65 * h),
        (0.08 * w, 0.5 * h),
    ];
    super::util::polygon(&points, FillRule::NonZero)
}

fn ribbon2_path(w: f32, h: f32) -> VectorPath {
    let points = vec![
        (0.0, 0.2 * h),
        (0.18 * w, 0.3 * h),
        (0.82 * w, 0.3 * h),
        (w, 0.2 * h),
        (w, 0.8 * h),
        (0.82 * w, 0.7 * h),
        (0.18 * w, 0.7 * h),
        (0.0, 0.8 * h),
    ];
    super::util::polygon(&points, FillRule::NonZero)
}

fn ellipse_ribbon_path(w: f32, h: f32, adj: &[Option<f64>], invert: bool) -> VectorPath {
    let sag = 0.15 * h * norm_adj(adj, 0, 1.0, 0.2, 1.5);
    let thickness = 0.3 * h;
    let sign = if invert { 1.0 } else { -1.0 };
    let steps = 16;
    let mut points = Vec::with_capacity(steps * 2 + 4);
    for i in 0..=steps {
        let x = w * i as f32 / steps as f32;
        let bulge = sign * sag * (std::f32::consts::PI * x / w.max(f32::EPSILON)).sin();
        points.push((x, h * 0.35 + bulge));
    }
    for i in (0..=steps).rev() {
        let x = w * i as f32 / steps as f32;
        let bulge = sign * sag * (std::f32::consts::PI * x / w.max(f32::EPSILON)).sin();
        points.push((x, h * 0.35 + thickness + bulge));
    }
    let mut segs = Vec::new();
    push_polygon(&mut segs, &points);
    let end_y = h * 0.35 + 0.5 * thickness;
    push_polygon(
        &mut segs,
        &[
            (0.0, h * 0.35),
            (0.08 * w, end_y),
            (0.0, h * 0.35 + thickness),
        ],
    );
    push_polygon(
        &mut segs,
        &[(w, h * 0.35 + thickness), (0.92 * w, end_y), (w, h * 0.35)],
    );
    VectorPath::new(segs, FillRule::NonZero)
}

fn left_right_ribbon_path(w: f32, h: f32) -> VectorPath {
    let points = vec![
        (0.1 * w, 0.35 * h),
        (0.9 * w, 0.35 * h),
        (0.9 * w, 0.2 * h),
        (w, 0.5 * h),
        (0.9 * w, 0.8 * h),
        (0.9 * w, 0.65 * h),
        (0.1 * w, 0.65 * h),
        (0.1 * w, 0.8 * h),
        (0.0, 0.5 * h),
        (0.1 * w, 0.2 * h),
    ];
    super::util::polygon(&points, FillRule::NonZero)
}

fn scroll_path(w: f32, h: f32, vertical: bool) -> VectorPath {
    let r = 0.08 * w.min(h);
    let mut segs = Vec::new();
    push_round_rect(&mut segs, 0.1 * w, 0.1 * h, 0.9 * w, 0.9 * h, r, true);
    if vertical {
        push_round_rect(&mut segs, 0.82 * w, 0.0, w, 0.22 * h, r, true);
        push_round_rect(&mut segs, 0.0, 0.78 * h, 0.18 * w, h, r, true);
    } else {
        push_round_rect(&mut segs, 0.0, 0.82 * h, 0.22 * w, h, r, true);
        push_round_rect(&mut segs, 0.78 * w, 0.0, w, 0.18 * h, r, true);
    }
    VectorPath::new(segs, FillRule::NonZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::PathSegment;

    #[test]
    fn waves_stay_in_bounds() {
        for name in ["wave", "doubleWave", "moon", "smileyFace"] {
            let path = build_symbol_extra_preset(name, 200.0, 120.0, &[]).expect("preset");
            for seg in &path.segments {
                if let PathSegment::LineTo { x, y } | PathSegment::MoveTo { x, y } = seg {
                    assert!(x.is_finite() && y.is_finite(), "{name}");
                    assert!(*x >= -1e-3 && *x <= 200.0 + 1e-3, "{name} x={x}");
                    assert!(*y >= -1e-3 && *y <= 120.0 + 1e-3, "{name} y={y}");
                }
            }
        }
    }
}
