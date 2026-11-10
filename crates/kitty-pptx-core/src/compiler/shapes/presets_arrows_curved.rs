//! Curved directional arrows (`curvedRightArrow` … `curvedDownArrow`).
//!
//! The canonical silhouette is a quarter-elliptical ribbon whose tail lies on
//! the top edge and whose head cap points right. The three sibling presets are
//! exact mirrors/rotations of that point list, so all four stay within bounds.

use crate::contracts::{FillRule, VectorPath};

use super::util::{
    mirror_points_x, norm_adj, polygon, rotate_points_ccw, rotate_points_cw, sample_arc,
};

pub fn build_curved_arrow_preset(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    let path = match geometry {
        "curvedRightArrow" => polygon(&curved_points(w, h, adj), FillRule::NonZero),
        "curvedLeftArrow" => {
            let points = curved_points(w, h, adj);
            let mirrored: Vec<(f32, f32)> = mirror_points_x(&points, w).into_iter().rev().collect();
            polygon(&mirrored, FillRule::NonZero)
        }
        "curvedDownArrow" => {
            let points = curved_points(h, w, adj);
            polygon(&rotate_points_cw(&points, h), FillRule::NonZero)
        }
        "curvedUpArrow" => {
            let points = curved_points(h, w, adj);
            polygon(&rotate_points_ccw(&points, w), FillRule::NonZero)
        }
        _ => return None,
    };
    Some(path)
}

/// Quarter-ribbon with an arrowhead cap, built in a `w × h` local frame.
fn curved_points(w: f32, h: f32, adj: &[Option<f64>]) -> Vec<(f32, f32)> {
    let u = w.min(h).max(0.0);
    let thickness = (u * norm_adj(adj, 0, 0.18, 0.06, 0.35)).max(0.0);
    let ro = 0.5 * u;
    let ri = (ro - thickness).max(0.0);
    let head_len = ro * 0.45;
    let spread = (thickness * 0.8).min(ri * 0.8).min(ro * 0.5);
    let cx = w / 2.0;
    let cy = 0.0;

    // Outer arc: from the top-left quadrant (180°) to the bottom of the circle
    // (90°), where the tangent is horizontal and points right.
    let mut points = sample_arc(cx, cy, ro, ro, 180.0, -90.0, 12);
    let (ax, ay) = (cx, cy + ro);
    let (bx, by) = (cx, cy + ri);
    let (fx, fy) = (1.0_f32, 0.0_f32);
    points.push((ax + fx * head_len, ay + fy * head_len + spread));
    points.push((
        (ax + bx) / 2.0 + fx * head_len * 1.3,
        (ay + by) / 2.0 + fy * head_len * 1.3,
    ));
    points.push((bx + fx * head_len, by + fy * head_len - spread));
    points.extend(sample_arc(cx, cy, ri, ri, 90.0, 90.0, 12));
    points
}
