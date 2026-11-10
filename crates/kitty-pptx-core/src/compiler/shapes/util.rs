//! Shared math and path helpers for the extended preset geometry builders.
//!
//! Everything here is resolution independent: builders receive local pixel
//! dimensions `(w, h)` and emit coordinates in the element's local space.
//! Adjustment handles are normalized exactly as specified by PRD 11 §4.1.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;

/// Cubic Bézier approximation factor for a quarter circular arc,
/// `k = 4/3 · tan(pi/8) = 4/3 · (sqrt(2) - 1)` (PRD 11 §4.2).
pub const KAPPA: f32 = 0.552_284_8;

/// Normalize an OOXML adjustment handle (`adj1`..`adj8`) into a dimensionless
/// ratio, applying the PRD 11 §4.1 clamp and NaN guard.
#[inline]
pub fn norm_adj(
    adj: &[Option<f64>],
    index: usize,
    default_val: f32,
    min_val: f32,
    max_val: f32,
) -> f32 {
    match adj.get(index).and_then(|v| *v) {
        Some(raw) => {
            let normalized = (raw / 100_000.0) as f32;
            if normalized.is_nan() {
                default_val
            } else {
                normalized.clamp(min_val, max_val)
            }
        }
        None => default_val,
    }
}

/// Normalize a callout pointer handle. Unlike body adjustment handles the
/// target point may legitimately live outside the element (PRD 11 §4.1 relaxes
/// the range to `[-5.0, 5.0]`).
#[inline]
pub fn free_adj(adj: &[Option<f64>], index: usize, default_val: f32) -> f32 {
    norm_adj(adj, index, default_val, -5.0, 5.0)
}

/// Keep a computed coordinate finite; non-finite values collapse to `fallback`.
#[inline]
pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// Append a closed polygon subpath (`MoveTo` … `LineTo` … `Close`).
pub fn push_polygon(segments: &mut Vec<PathSegment>, points: &[(f32, f32)]) {
    let Some(&(x0, y0)) = points.first() else {
        return;
    };
    segments.push(PathSegment::MoveTo { x: x0, y: y0 });
    for &(x, y) in &points[1..] {
        segments.push(PathSegment::LineTo { x, y });
    }
    segments.push(PathSegment::Close);
}

/// Build a closed polygon path.
pub fn polygon(points: &[(f32, f32)], fill_rule: FillRule) -> VectorPath {
    let mut segments = Vec::with_capacity(points.len() + 2);
    push_polygon(&mut segments, points);
    VectorPath::new(segments, fill_rule)
}

/// Append a full ellipse subpath starting at the right-most point.
pub fn push_ellipse(
    segments: &mut Vec<PathSegment>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    clockwise: bool,
) {
    segments.push(PathSegment::MoveTo { x: cx + rx, y: cy });
    let sweep = if clockwise { 360.0 } else { -360.0 };
    decompose_arc_to_cubic(cx, cy, rx, ry, 0.0, sweep, segments);
    segments.push(PathSegment::Close);
}

/// Append an elliptical arc subpath. `start_deg` follows OOXML's convention
/// (0° points right, angles increase clockwise on screen because y grows down).
pub fn push_arc(
    segments: &mut Vec<PathSegment>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    start_deg: f32,
    sweep_deg: f32,
) {
    let start = start_deg.to_radians();
    segments.push(PathSegment::MoveTo {
        x: cx + rx * start.cos(),
        y: cy + ry * start.sin(),
    });
    decompose_arc_to_cubic(cx, cy, rx, ry, start_deg, sweep_deg, segments);
}

/// Append a rounded rectangle subpath with absolute bounds. `r` is clamped to
/// half the smaller side so degenerate rectangles stay valid.
pub fn push_round_rect(
    segments: &mut Vec<PathSegment>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: f32,
    clockwise: bool,
) {
    let rw = (x1 - x0).abs();
    let rh = (y1 - y0).abs();
    let radius = if r.is_finite() {
        r.clamp(0.0, 0.5 * rw.min(rh))
    } else {
        0.0
    };
    if radius <= f32::EPSILON {
        push_polygon(segments, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        return;
    }

    if clockwise {
        segments.push(PathSegment::MoveTo {
            x: x0 + radius,
            y: y0,
        });
        segments.push(PathSegment::LineTo {
            x: x1 - radius,
            y: y0,
        });
        decompose_arc_to_cubic(
            x1 - radius,
            y0 + radius,
            radius,
            radius,
            270.0,
            90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x1,
            y: y1 - radius,
        });
        decompose_arc_to_cubic(
            x1 - radius,
            y1 - radius,
            radius,
            radius,
            0.0,
            90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x0 + radius,
            y: y1,
        });
        decompose_arc_to_cubic(
            x0 + radius,
            y1 - radius,
            radius,
            radius,
            90.0,
            90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x0,
            y: y0 + radius,
        });
        decompose_arc_to_cubic(
            x0 + radius,
            y0 + radius,
            radius,
            radius,
            180.0,
            90.0,
            segments,
        );
        segments.push(PathSegment::Close);
    } else {
        segments.push(PathSegment::MoveTo {
            x: x0 + radius,
            y: y0,
        });
        decompose_arc_to_cubic(
            x0 + radius,
            y0 + radius,
            radius,
            radius,
            270.0,
            -90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x0,
            y: y1 - radius,
        });
        decompose_arc_to_cubic(
            x0 + radius,
            y1 - radius,
            radius,
            radius,
            180.0,
            -90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x1 - radius,
            y: y1,
        });
        decompose_arc_to_cubic(
            x1 - radius,
            y1 - radius,
            radius,
            radius,
            90.0,
            -90.0,
            segments,
        );
        segments.push(PathSegment::LineTo {
            x: x1,
            y: y0 + radius,
        });
        decompose_arc_to_cubic(
            x1 - radius,
            y0 + radius,
            radius,
            radius,
            0.0,
            -90.0,
            segments,
        );
        segments.push(PathSegment::Close);
    }
}

/// Full-box rounded rectangle path.
pub fn round_rect_path(w: f32, h: f32, r: f32) -> VectorPath {
    let mut segments = Vec::new();
    push_round_rect(&mut segments, 0.0, 0.0, w, h, r, true);
    VectorPath::new(segments, FillRule::NonZero)
}

/// Vertices of a regular star inscribed in the element bounds, first tip at
/// the top centre (PRD 11 §2.5.1).
pub fn star_vertices(n: usize, w: f32, h: f32, inner_ratio: f32) -> Vec<(f32, f32)> {
    let points = n.max(3);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let ratio = if inner_ratio.is_finite() {
        inner_ratio.clamp(0.0, 0.95)
    } else {
        0.5
    };
    let step = std::f32::consts::PI / points as f32;
    let mut out = Vec::with_capacity(points * 2);
    for i in 0..points * 2 {
        let phi = -std::f32::consts::FRAC_PI_2 + i as f32 * step;
        let rho = if i % 2 == 0 { 1.0 } else { ratio };
        out.push((cx + rx * rho * phi.cos(), cy + ry * rho * phi.sin()));
    }
    out
}

/// Vertices of a regular polygon inscribed in the element bounds, first vertex
/// at the top centre.
pub fn regular_polygon_vertices(n: usize, w: f32, h: f32, rotation_deg: f32) -> Vec<(f32, f32)> {
    let points = n.max(3);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let start = -std::f32::consts::FRAC_PI_2 + rotation_deg.to_radians();
    let step = std::f32::consts::TAU / points as f32;
    let mut out = Vec::with_capacity(points);
    for i in 0..points {
        let phi = start + i as f32 * step;
        out.push((cx + rx * phi.cos(), cy + ry * phi.sin()));
    }
    out
}

/// Mirror a point list across the vertical centre line.
pub fn mirror_points_x(points: &[(f32, f32)], w: f32) -> Vec<(f32, f32)> {
    points.iter().map(|&(x, y)| (w - x, y)).collect()
}

/// Mirror a point list across the horizontal centre line.
pub fn mirror_points_y(points: &[(f32, f32)], h: f32) -> Vec<(f32, f32)> {
    points.iter().map(|&(x, y)| (x, h - y)).collect()
}

/// Rotate a point list 90° clockwise into a box of swapped dimensions.
pub fn rotate_points_cw(points: &[(f32, f32)], src_w: f32) -> Vec<(f32, f32)> {
    points.iter().map(|&(x, y)| (y, src_w - x)).collect()
}

/// Rotate a point list 90° counter-clockwise into a box of swapped dimensions.
pub fn rotate_points_ccw(points: &[(f32, f32)], src_h: f32) -> Vec<(f32, f32)> {
    points.iter().map(|&(x, y)| (src_h - y, x)).collect()
}

/// Build a closed path from a list of point lists (one subpath each).
pub fn multi_polygon(polys: &[Vec<(f32, f32)>], fill_rule: FillRule) -> VectorPath {
    let mut segments = Vec::new();
    for poly in polys {
        push_polygon(&mut segments, poly);
    }
    VectorPath::new(segments, fill_rule)
}

/// Sample an elliptical arc into line segments, used by stand-in silhouettes
/// that do not need exact Bézier decomposition (clouds, scrolls, seals).
pub fn sample_arc(
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    start_deg: f32,
    sweep_deg: f32,
    steps: usize,
) -> Vec<(f32, f32)> {
    let n = steps.max(1);
    let start = start_deg.to_radians();
    let step = sweep_deg.to_radians() / n as f32;
    (0..=n)
        .map(|i| {
            let a = start + step * i as f32;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

/// Clamp helper that also replaces non-finite input.
#[inline]
pub fn clamp_finite(value: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    let v = finite_or(value, fallback);
    v.clamp(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_adj_defaults_and_clamps() {
        assert!((norm_adj(&[], 0, 0.25, 0.0, 1.0) - 0.25).abs() < 1e-6);
        assert!((norm_adj(&[Some(50_000.0)], 0, 0.0, 0.0, 1.0) - 0.5).abs() < 1e-6);
        assert!((norm_adj(&[Some(500_000.0)], 0, 0.0, 0.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((norm_adj(&[Some(f64::NAN)], 0, 0.3, 0.0, 1.0) - 0.3).abs() < 1e-6);
        assert!((free_adj(&[Some(-100_000.0)], 0, 0.0) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn star_vertices_stay_in_bounds() {
        for n in [4usize, 5, 7, 10, 12, 16, 24, 32] {
            for &(w, h) in &[(100.0f32, 60.0f32), (60.0, 100.0), (80.0, 80.0)] {
                for (x, y) in star_vertices(n, w, h, 0.55) {
                    assert!(x >= -1e-3 && x <= w + 1e-3);
                    assert!(y >= -1e-3 && y <= h + 1e-3);
                }
            }
        }
    }

    #[test]
    fn round_rect_arcs_are_closed() {
        let path = round_rect_path(100.0, 40.0, 500.0);
        assert_eq!(path.segments.last(), Some(&PathSegment::Close));
        assert!(matches!(path.segments[0], PathSegment::MoveTo { .. }));
    }
}
