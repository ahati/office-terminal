//! Extended symbol presets: polar stars, gears, teardrop, funnel, braces,
//! brackets, regular polygons and the radial arc primitives.
//!
//! Star geometry follows the universal polar formulation of PRD 11 §2.5.1;
//! brace hooks use the cubic construction of PRD 11 §2.4.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;
use super::util::{
    norm_adj, push_arc, push_ellipse, push_polygon, regular_polygon_vertices, star_vertices, KAPPA,
};

pub fn build_symbol_preset(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    match geometry {
        "star4" => Some(star_path(4, w, h, adj, 0.38197)),
        "star7" => Some(star_path(7, w, h, adj, 0.5)),
        "star10" => Some(star_path(10, w, h, adj, 0.6)),
        "star12" => Some(star_path(12, w, h, adj, 0.7)),
        "star16" => Some(star_path(16, w, h, adj, 0.75)),
        "star24" => Some(star_path(24, w, h, adj, 0.8)),
        "star32" => Some(star_path(32, w, h, adj, 0.85)),

        "gear6" => Some(gear_path(6, w, h, adj)),
        "gear9" => Some(gear_path(9, w, h, adj)),

        "teardrop" => Some(teardrop_path(w, h)),

        "funnel" => {
            let neck = w * norm_adj(adj, 0, 0.3, 0.02, 0.9);
            let shoulder = h * norm_adj(adj, 1, 0.5, 0.05, 0.95);
            let cx = w / 2.0;
            Some(super::util::polygon(
                &[
                    (0.0, 0.0),
                    (w, 0.0),
                    (cx + neck / 2.0, shoulder),
                    (cx + neck / 2.0, h),
                    (cx - neck / 2.0, h),
                    (cx - neck / 2.0, shoulder),
                ],
                FillRule::NonZero,
            ))
        }

        "leftBrace" => {
            let r = brace_radius(w, h, adj);
            let mut segs = Vec::new();
            push_brace(&mut segs, w, h, r, false, 0.0);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }
        "rightBrace" => {
            let r = brace_radius(w, h, adj);
            let mut segs = Vec::new();
            push_brace(&mut segs, w, h, r, true, 0.0);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }
        "bracePair" => {
            let half = w / 2.0;
            let r = brace_radius(half, h, adj);
            let mut segs = Vec::new();
            push_brace(&mut segs, half, h, r, false, 0.0);
            push_brace(&mut segs, half, h, r, true, half);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "leftBracket" => {
            let mut segs = Vec::new();
            push_bracket(&mut segs, w, h, bracket_arm(w, adj), false, 0.0);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }
        "rightBracket" => {
            let mut segs = Vec::new();
            push_bracket(&mut segs, w, h, bracket_arm(w, adj), true, 0.0);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }
        "bracketPair" => {
            let half = w / 2.0;
            let arm = bracket_arm(half, adj);
            let mut segs = Vec::new();
            push_bracket(&mut segs, half, h, arm, false, 0.0);
            push_bracket(&mut segs, half, h, arm, true, half);
            Some(VectorPath::new(segs, FillRule::NonZero))
        }

        "heptagon" => Some(poly_path(7, w, h)),
        "decagon" => Some(poly_path(10, w, h)),
        "dodecagon" => Some(poly_path(12, w, h)),

        "rtTriangle" => Some(super::util::polygon(
            &[(0.0, 0.0), (w, h), (0.0, h)],
            FillRule::NonZero,
        )),

        "blockArc" => Some(block_arc_path(w, h, adj)),
        "donut" => Some(donut_path(w, h, adj)),
        "noSmoking" => Some(no_smoking_path(w, h, adj)),
        "pie" => Some(pie_path(w, h, adj, 0.0, 0.75)),
        "pieWedge" => Some(pie_path(w, h, adj, 0.1, 0.6)),
        "chord" => Some(chord_path(w, h, adj)),
        "arc" => Some(open_arc_path(w, h, adj)),

        _ => None,
    }
}

fn star_path(points: usize, w: f32, h: f32, adj: &[Option<f64>], default: f32) -> VectorPath {
    let inner = norm_adj(adj, 0, default, 0.0, 0.95);
    super::util::polygon(&star_vertices(points, w, h, inner), FillRule::NonZero)
}

fn poly_path(n: usize, w: f32, h: f32) -> VectorPath {
    super::util::polygon(&regular_polygon_vertices(n, w, h, 0.0), FillRule::NonZero)
}

fn gear_path(teeth: usize, w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let bore_ratio = norm_adj(adj, 0, 0.3, 0.05, 0.6);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx_outer = w / 2.0;
    let ry_outer = h / 2.0;
    let rx_root = rx_outer * 0.75;
    let ry_root = ry_outer * 0.75;
    let step = std::f32::consts::TAU / teeth as f32;
    let half_tooth = step * 0.18;
    let root_half = step * 0.32;

    let mut points = Vec::with_capacity(teeth * 4);
    for i in 0..teeth {
        let base = i as f32 * step;
        for (angle, rx, ry) in [
            (base - half_tooth, rx_outer, ry_outer),
            (base + half_tooth, rx_outer, ry_outer),
            (base + root_half, rx_root, ry_root),
            (base + step - root_half, rx_root, ry_root),
        ] {
            points.push((cx + rx * angle.cos(), cy + ry * angle.sin()));
        }
    }

    let mut segs = Vec::new();
    push_polygon(&mut segs, &points);
    push_ellipse(
        &mut segs,
        cx,
        cy,
        rx_outer * bore_ratio,
        ry_outer * bore_ratio,
        false,
    );
    VectorPath::new(segs, FillRule::EvenOdd)
}

fn teardrop_path(w: f32, h: f32) -> VectorPath {
    let r = 0.5 * w.min(h);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let mut segs = Vec::new();
    push_arc(&mut segs, cx, cy, r, r, 200.0, 240.0);
    segs.push(PathSegment::LineTo { x: 0.0, y: h });
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn brace_radius(w: f32, h: f32, adj: &[Option<f64>]) -> f32 {
    let a1 = norm_adj(adj, 0, 0.1, 0.0, 1.0);
    (w.min(h / 4.0).max(0.0) * a1).max(0.0)
}

/// Push one smooth curly brace. `mirror` flips it horizontally and `ox`
/// translates it, which is how `bracePair` composes both halves.
fn push_brace(segments: &mut Vec<PathSegment>, w: f32, h: f32, r: f32, mirror: bool, ox: f32) {
    let x = |v: f32| if mirror { w - v } else { v } + ox;
    let r = r.clamp(0.0, w.min(h / 4.0).max(0.0));
    let x1 = w - r;
    let ym = h / 2.0;
    let k = r * KAPPA;

    segments.push(PathSegment::MoveTo { x: x(w), y: 0.0 });
    segments.push(PathSegment::CubicTo {
        x1: x(w - k),
        y1: 0.0,
        x2: x(x1),
        y2: r - k,
        x: x(x1),
        y: r,
    });
    segments.push(PathSegment::LineTo {
        x: x(x1),
        y: ym - r,
    });
    segments.push(PathSegment::CubicTo {
        x1: x(x1),
        y1: ym - r + k,
        x2: x(k),
        y2: ym,
        x: x(0.0),
        y: ym,
    });
    segments.push(PathSegment::CubicTo {
        x1: x(k),
        y1: ym,
        x2: x(x1),
        y2: ym + r - k,
        x: x(x1),
        y: ym + r,
    });
    segments.push(PathSegment::LineTo { x: x(x1), y: h - r });
    segments.push(PathSegment::CubicTo {
        x1: x(x1),
        y1: h - r + k,
        x2: x(w - k),
        y2: h,
        x: x(w),
        y: h,
    });
    segments.push(PathSegment::Close);
}

fn bracket_arm(w: f32, adj: &[Option<f64>]) -> f32 {
    w * norm_adj(adj, 0, 0.25, 0.02, 0.9)
}

fn push_bracket(segments: &mut Vec<PathSegment>, w: f32, h: f32, arm: f32, mirror: bool, ox: f32) {
    let x = |v: f32| if mirror { w - v } else { v } + ox;
    let arm = arm.clamp(0.0, w);
    let t = (arm * 0.5).min(h * 0.45).max(0.0);
    push_polygon(
        segments,
        &[
            (x(arm), 0.0),
            (x(0.0), 0.0),
            (x(0.0), h),
            (x(arm), h),
            (x(arm), h - t),
            (x(t), h - t),
            (x(t), t),
            (x(arm), t),
        ],
    );
}

fn block_arc_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let thickness = norm_adj(adj, 0, 0.4, 0.05, 0.9);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let rix = rx * (1.0 - thickness);
    let riy = ry * (1.0 - thickness);
    let mut segs = Vec::new();
    push_arc(&mut segs, cx, cy, rx, ry, 180.0, 180.0);
    segs.push(PathSegment::LineTo { x: cx + rix, y: cy });
    decompose_arc_to_cubic(cx, cy, rix, riy, 0.0, -180.0, &mut segs);
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn donut_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let inner = norm_adj(adj, 0, 0.25, 0.02, 0.95);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let mut segs = Vec::new();
    push_ellipse(&mut segs, cx, cy, rx, ry, true);
    push_ellipse(
        &mut segs,
        cx,
        cy,
        rx * (1.0 - inner),
        ry * (1.0 - inner),
        true,
    );
    VectorPath::new(segs, FillRule::EvenOdd)
}

fn no_smoking_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let inner = norm_adj(adj, 0, 0.25, 0.02, 0.9);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let t = (0.05 * w.min(h)).min(0.2 * rx).max(0.0);
    let mut segs = Vec::new();
    push_ellipse(&mut segs, cx, cy, rx, ry, true);
    push_ellipse(
        &mut segs,
        cx,
        cy,
        rx * (1.0 - inner),
        ry * (1.0 - inner),
        false,
    );
    push_polygon(
        &mut segs,
        &[
            (cx - 0.62 * rx, cy + 0.62 * ry),
            (cx - 0.62 * rx + t, cy + 0.62 * ry),
            (cx + 0.62 * rx + t, cy - 0.62 * ry),
            (cx + 0.62 * rx, cy - 0.62 * ry),
        ],
    );
    VectorPath::new(segs, FillRule::NonZero)
}

fn pie_angles(adj: &[Option<f64>], default_start: f32, default_end: f32) -> (f32, f32) {
    let start = norm_adj(adj, 0, default_start, 0.0, 1.0) * 360.0;
    let mut end = norm_adj(adj, 1, default_end, 0.0, 1.0) * 360.0;
    if end <= start {
        end = start + 360.0;
    }
    (start, end)
}

fn pie_path(
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    default_start: f32,
    default_end: f32,
) -> VectorPath {
    let (start, end) = pie_angles(adj, default_start, default_end);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let a = start.to_radians();
    let mut segs = Vec::new();
    segs.push(PathSegment::MoveTo { x: cx, y: cy });
    segs.push(PathSegment::LineTo {
        x: cx + rx * a.cos(),
        y: cy + ry * a.sin(),
    });
    decompose_arc_to_cubic(cx, cy, rx, ry, start, end - start, &mut segs);
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn chord_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let (start, end) = pie_angles(adj, 0.0, 0.5);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let mut segs = Vec::new();
    push_arc(&mut segs, cx, cy, rx, ry, start, end - start);
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn open_arc_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let (start, end) = pie_angles(adj, 0.0, 0.75);
    let mut segs = Vec::new();
    push_arc(
        &mut segs,
        w / 2.0,
        h / 2.0,
        w / 2.0,
        h / 2.0,
        start,
        end - start,
    );
    VectorPath::new(segs, FillRule::NonZero)
}
