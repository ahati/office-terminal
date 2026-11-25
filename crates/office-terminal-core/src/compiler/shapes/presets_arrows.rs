//! Extended directional arrow presets: notched, striped, bent, U-turn,
//! multi-way and circular arrows.
//!
//! Straight and orthogonal arrows are exact parametric polygons; the circular
//! family is a sampled annular-ribbon stand-in (Tier 2) that keeps the arrow
//! head cap and the bounding box semantics.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::util::{mirror_points_x, norm_adj, polygon, push_polygon, sample_arc};

/// Shared metrics for orthogonal multi-way arrows.
struct ArrowMetrics {
    spread: f32,
    shaft: f32,
    head_x: f32,
    head_y: f32,
}

fn arrow_metrics(w: f32, h: f32, adj: &[Option<f64>]) -> ArrowMetrics {
    let u = w.min(h).max(0.0);
    let spread = (u * norm_adj(adj, 0, 0.22, 0.02, 0.45)).max(0.0);
    let shaft = (u * norm_adj(adj, 1, 0.08, 0.005, 0.25)).max(0.0);
    let head = (u * norm_adj(adj, 2, 0.22, 0.02, 0.45)).max(0.0);
    ArrowMetrics {
        spread,
        shaft,
        head_x: head.min((0.5 * w - shaft).max(0.0)),
        head_y: head.min((0.5 * h - shaft).max(0.0)),
    }
}

pub fn build_arrow_preset(geometry: &str, w: f32, h: f32, adj: &[Option<f64>]) -> Option<VectorPath> {
    let path = match geometry {
        "notchedRightArrow" => {
            let head = w * norm_adj(adj, 0, 0.5, 0.0, 0.9);
            let shaft = 0.5 * h * norm_adj(adj, 1, 0.5, 0.0, 1.0);
            let notch = (w * norm_adj(adj, 2, 0.25, 0.0, 0.5)).min((w - head).max(0.0));
            let y1 = h / 2.0 - shaft;
            let y2 = h / 2.0 + shaft;
            let xh = w - head;
            polygon(
                &[
                    (notch, h / 2.0),
                    (0.0, y1),
                    (xh, y1),
                    (xh, 0.0),
                    (w, h / 2.0),
                    (xh, h),
                    (xh, y2),
                    (0.0, y2),
                ],
                FillRule::NonZero,
            )
        }

        "stripedRightArrow" => striped_path(w, h, adj),

        "quadArrow" => {
            let m = arrow_metrics(w, h, adj);
            let cx = w / 2.0;
            let cy = h / 2.0;
            polygon(
                &[
                    (cx, 0.0),
                    (cx + m.spread, m.head_y),
                    (cx + m.shaft, m.head_y),
                    (cx + m.shaft, cy - m.shaft),
                    (w - m.head_x, cy - m.shaft),
                    (w - m.head_x, cy - m.spread),
                    (w, cy),
                    (w - m.head_x, cy + m.spread),
                    (w - m.head_x, cy + m.shaft),
                    (cx + m.shaft, cy + m.shaft),
                    (cx + m.shaft, h - m.head_y),
                    (cx + m.spread, h - m.head_y),
                    (cx, h),
                    (cx - m.spread, h - m.head_y),
                    (cx - m.shaft, h - m.head_y),
                    (cx - m.shaft, cy + m.shaft),
                    (m.head_x, cy + m.shaft),
                    (m.head_x, cy + m.spread),
                    (0.0, cy),
                    (m.head_x, cy - m.spread),
                    (m.head_x, cy - m.shaft),
                    (cx - m.shaft, cy - m.shaft),
                    (cx - m.shaft, m.head_y),
                    (cx - m.spread, m.head_y),
                ],
                FillRule::NonZero,
            )
        }

        "leftRightUpArrow" => {
            let m = arrow_metrics(w, h, adj);
            let cx = w / 2.0;
            let cy = h / 2.0;
            polygon(
                &[
                    (cx, 0.0),
                    (cx + m.spread, m.head_y),
                    (cx + m.shaft, m.head_y),
                    (cx + m.shaft, cy - m.shaft),
                    (w - m.head_x, cy - m.shaft),
                    (w - m.head_x, cy - m.spread),
                    (w, cy),
                    (w - m.head_x, cy + m.spread),
                    (w - m.head_x, cy + m.shaft),
                    (cx + m.shaft, cy + m.shaft),
                    (cx + m.shaft, h),
                    (cx - m.shaft, h),
                    (cx - m.shaft, cy + m.shaft),
                    (m.head_x, cy + m.shaft),
                    (m.head_x, cy + m.spread),
                    (0.0, cy),
                    (m.head_x, cy - m.spread),
                    (m.head_x, cy - m.shaft),
                    (cx - m.shaft, cy - m.shaft),
                    (cx - m.shaft, m.head_y),
                    (cx - m.spread, m.head_y),
                ],
                FillRule::NonZero,
            )
        }

        "leftUpArrow" => {
            let u = w.min(h).max(0.0);
            let spread = (u * norm_adj(adj, 0, 0.22, 0.02, 0.45)).min(0.28 * w).min(0.28 * h);
            let shaft = (u * norm_adj(adj, 1, 0.08, 0.005, 0.25)).min(0.4 * spread);
            let head = (u * norm_adj(adj, 2, 0.22, 0.02, 0.45)).min(0.28 * w.min(h));
            let cx = 0.65 * w;
            let cy = 0.65 * h;
            polygon(
                &[
                    (cx, 0.0),
                    (cx + spread, head),
                    (cx + shaft, head),
                    (cx + shaft, h),
                    (cx - shaft, h),
                    (cx - shaft, cy + shaft),
                    (head, cy + shaft),
                    (head, cy + spread),
                    (0.0, cy),
                    (head, cy - spread),
                    (head, cy - shaft),
                    (cx - shaft, cy - shaft),
                    (cx - shaft, head),
                    (cx - spread, head),
                ],
                FillRule::NonZero,
            )
        }

        "upDownArrow" => {
            let m = arrow_metrics(w, h, adj);
            let cx = w / 2.0;
            let hd = m.head_y;
            polygon(
                &[
                    (cx, 0.0),
                    (cx + m.spread, hd),
                    (cx + m.shaft, hd),
                    (cx + m.shaft, h - hd),
                    (cx + m.spread, h - hd),
                    (cx, h),
                    (cx - m.spread, h - hd),
                    (cx - m.shaft, h - hd),
                    (cx - m.shaft, hd),
                    (cx - m.spread, hd),
                ],
                FillRule::NonZero,
            )
        }

        "bentArrow" => {
            let points = bent_arrow_points(w, h, adj);
            polygon(&points, FillRule::NonZero)
        }

        "bentUpArrow" => {
            let points = bent_arrow_points(w, h, adj);
            let mirrored: Vec<(f32, f32)> = mirror_points_x(&points, w).into_iter().rev().collect();
            polygon(&mirrored, FillRule::NonZero)
        }

        "uturnArrow" => uturn_path(w, h, adj),

        "circularArrow" => {
            let points = circular_arrow_points(w, h, adj, 200.0, 270.0);
            polygon(&points, FillRule::NonZero)
        }

        "leftCircularArrow" => {
            let points = circular_arrow_points(w, h, adj, 200.0, 270.0);
            let mirrored: Vec<(f32, f32)> = mirror_points_x(&points, w).into_iter().rev().collect();
            polygon(&mirrored, FillRule::NonZero)
        }

        "leftRightCircularArrow" => {
            let right = circular_arrow_points(w, h, adj, 180.0, 180.0);
            let left: Vec<(f32, f32)> = mirror_points_x(&right, w).into_iter().rev().collect();
            let mut segs = Vec::new();
            push_polygon(&mut segs, &right);
            push_polygon(&mut segs, &left);
            VectorPath::new(segs, FillRule::NonZero)
        }

        _ => return None,
    };
    Some(path)
}

fn striped_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let head = norm_adj(adj, 0, 0.5, 0.0, 0.9);
    let shaft = norm_adj(adj, 1, 0.5, 0.0, 1.0);
    let x_head = w * (1.0 - head);
    let y1 = h * (1.0 - shaft) / 2.0;
    let y2 = h * (1.0 + shaft) / 2.0;
    let mut segs = Vec::new();
    push_polygon(
        &mut segs,
        &[
            (0.0, y1),
            (x_head, y1),
            (x_head, 0.0),
            (w, h / 2.0),
            (x_head, h),
            (x_head, y2),
            (0.0, y2),
        ],
    );
    let slit = (x_head * 0.05).max(0.0);
    for frac in [0.25, 0.45, 0.65] {
        let cx = x_head * frac;
        push_polygon(
            &mut segs,
            &[(cx - slit, y1), (cx + slit, y1), (cx + slit, y2), (cx - slit, y2)],
        );
    }
    VectorPath::new(segs, FillRule::EvenOdd)
}

fn bent_arrow_points(w: f32, h: f32, adj: &[Option<f64>]) -> Vec<(f32, f32)> {
    let u = w.min(h).max(0.0);
    let shaft = (u * norm_adj(adj, 0, 0.2, 0.02, 0.4)).max(0.0);
    let barb = (u * norm_adj(adj, 1, 0.18, 0.02, 0.36)).max(0.0);
    let head = (w * norm_adj(adj, 2, 0.28, 0.02, 0.7)).clamp(0.0, (w - shaft).max(0.0));
    let band_top = (barb + 0.04 * h).min((h - shaft - barb).max(0.0));
    let barb = barb.min(band_top);
    let xh = w - head;
    vec![
        (0.0, h),
        (shaft, h),
        (shaft, band_top + shaft),
        (xh, band_top + shaft),
        (xh, band_top + shaft + barb),
        (w, band_top + shaft / 2.0),
        (xh, band_top - barb),
        (xh, band_top),
        (0.0, band_top),
    ]
}

fn uturn_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let u = w.min(h).max(0.0);
    let thickness = (u * norm_adj(adj, 0, 0.2, 0.02, 0.4)).max(0.0);
    let barb = (u * norm_adj(adj, 1, 0.12, 0.01, 0.2)).max(0.0);
    let cx = w / 2.0;
    // Elliptical arch: touches both vertical edges and never leaves the box.
    let rx = cx;
    let ry = (0.5 * w).min(0.4 * h).max(0.0);
    let cy = ry;
    let rix = (rx - thickness).max(0.0);
    let riy = (ry - thickness).max(0.0);
    let barb = barb.min((ry - riy).max(0.0)).min(thickness);
    let head = (h * norm_adj(adj, 2, 0.2, 0.05, 0.35)).min((h - cy).max(0.0)).max(0.0);
    let ybase = h - head;

    let mut segs = Vec::new();
    segs.push(PathSegment::MoveTo { x: 0.0, y: h });
    segs.push(PathSegment::LineTo { x: 0.0, y: cy });
    super::arc::decompose_arc_to_cubic(cx, cy, rx, ry, 180.0, 180.0, &mut segs);
    segs.push(PathSegment::LineTo { x: w, y: ybase });
    segs.push(PathSegment::LineTo {
        x: w - thickness / 2.0,
        y: h,
    });
    segs.push(PathSegment::LineTo {
        x: w - thickness - barb,
        y: ybase,
    });
    segs.push(PathSegment::LineTo { x: cx + rix, y: ybase });
    segs.push(PathSegment::LineTo { x: cx + rix, y: cy });
    super::arc::decompose_arc_to_cubic(cx, cy, rix, riy, 0.0, -180.0, &mut segs);
    segs.push(PathSegment::LineTo { x: cx - rix, y: h });
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn circular_arrow_points(w: f32, h: f32, adj: &[Option<f64>], start_deg: f32, sweep_deg: f32) -> Vec<(f32, f32)> {
    let u = w.min(h).max(0.0);
    let thickness = norm_adj(adj, 0, 0.5, 0.2, 0.75);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let ro = 0.28 * u;
    let ri = ro * (1.0 - thickness);
    let steps = 24;
    let mut points = sample_arc(cx, cy, ro, ro, start_deg, sweep_deg, steps);
    let end = (start_deg + sweep_deg).to_radians();
    let dir = if sweep_deg >= 0.0 { 1.0 } else { -1.0 };
    let fx = -end.sin() * dir;
    let fy = end.cos() * dir;
    let head = ro * 0.7;
    let spread = (ro - ri) * 0.9;
    let (rx_hat, ry_hat) = (end.cos(), end.sin());
    let ox = cx + ro * rx_hat;
    let oy = cy + ro * ry_hat;
    let ix = cx + ri * rx_hat;
    let iy = cy + ri * ry_hat;
    // Barbs flare radially outward/inward around the ribbon cross-section.
    points.push((ox + fx * head + rx_hat * spread, oy + fy * head + ry_hat * spread));
    points.push(((ox + ix) / 2.0 + fx * head * 1.3, (oy + iy) / 2.0 + fy * head * 1.3));
    points.push((ix + fx * head - rx_hat * spread, iy + fy * head - ry_hat * spread));
    points.extend(sample_arc(cx, cy, ri, ri, start_deg + sweep_deg, -sweep_deg, steps));
    points
}
