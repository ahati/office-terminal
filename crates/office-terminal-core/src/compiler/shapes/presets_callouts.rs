//! Speech balloons, wedge callouts, cloud callouts and leader-line callouts.
//!
//! Wedge hulls follow the edge-binding construction of PRD 11 §2.2: the pointer
//! base is placed on the element edge facing the target and the wedge apex is
//! the target point itself. Targets may legitimately live outside the element
//! bounds, which is why pointer handles use the relaxed `[-5, 5]` range.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;
use super::util::{free_adj, norm_adj, push_ellipse, push_polygon, push_round_rect};

/// Edge of the callout hull that the pointer attaches to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    Inside,
}

pub fn build_callout_preset(
    geometry: &str,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) -> Option<VectorPath> {
    let path = match geometry {
        "rectCallout" | "wedgeRectCallout" => wedge_rect_path(w, h, adj, false),
        "roundRectCallout" | "wedgeRoundRectCallout" => wedge_rect_path(w, h, adj, true),
        "ellipseCallout" | "wedgeEllipseCallout" => ellipse_callout_path(w, h, adj),
        "cloudCallout" => cloud_callout_path(w, h, adj),

        "lineCallout1" => leader_path(w, h, adj, false, false, 1),
        "lineCallout2" => leader_path(w, h, adj, false, false, 2),
        "lineCallout3" => leader_path(w, h, adj, false, true, 3),
        "lineCallout4" => leader_path(w, h, adj, true, true, 3),

        "borderCallout1" | "callout1" => leader_path(w, h, adj, true, false, 1),
        "borderCallout2" | "callout2" => leader_path(w, h, adj, true, false, 2),
        "borderCallout3" | "callout3" => leader_path(w, h, adj, true, false, 3),

        "accentCallout1" => leader_path(w, h, adj, false, true, 1),
        "accentCallout2" => leader_path(w, h, adj, false, true, 2),
        "accentCallout3" => leader_path(w, h, adj, false, true, 3),
        "accentBorderCallout1" => leader_path(w, h, adj, true, true, 1),
        "accentBorderCallout2" => leader_path(w, h, adj, true, true, 2),
        "accentBorderCallout3" => leader_path(w, h, adj, true, true, 3),

        _ => return None,
    };
    Some(path)
}

/// Target point and pointer geometry shared by every callout.
struct Pointer {
    xt: f32,
    yt: f32,
    base_center: f32,
    half_width: f32,
    edge: Edge,
}

fn pointer(w: f32, h: f32, adj: &[Option<f64>]) -> Pointer {
    let xt = w * free_adj(adj, 0, -0.2);
    let yt = h * free_adj(adj, 1, 1.3);
    let base_center = norm_adj(adj, 2, 0.25, 0.0, 1.0);
    let half_width = w.min(h).max(0.0) * norm_adj(adj, 3, 0.15, 0.05, 0.3);
    Pointer {
        xt,
        yt,
        base_center,
        half_width,
        edge: edge_for(w, h, xt, yt),
    }
}

fn edge_for(w: f32, h: f32, xt: f32, yt: f32) -> Edge {
    // Ordered quadrant binding, mirroring the reference engines: an external
    // target first binds the top/bottom edge, then the left/right edge.
    if yt >= h {
        Edge::Bottom
    } else if yt <= 0.0 {
        Edge::Top
    } else if xt <= 0.0 {
        Edge::Left
    } else if xt >= w {
        Edge::Right
    } else {
        Edge::Inside
    }
}

/// Base segment along the attachment edge, clamped so it never leaves the hull.
fn base_span(span: f32, center: f32, half: f32) -> (f32, f32) {
    let lo = (span * center - half).clamp(0.0, (span - 2.0 * half).max(0.0));
    (lo, lo + 2.0 * half)
}

fn wedge_rect_path(w: f32, h: f32, adj: &[Option<f64>], rounded: bool) -> VectorPath {
    let p = pointer(w, h, adj);
    let r = (w.min(h) * norm_adj(adj, 4, 0.16667, 0.0, 0.25)).max(0.0);
    let mut segs = Vec::new();

    if rounded {
        push_round_rect(&mut segs, 0.0, 0.0, w, h, r, true);
    } else {
        push_polygon(&mut segs, &[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
    }

    match p.edge {
        Edge::Inside => {}
        Edge::Bottom => {
            let (x1, x2) = base_span(w, p.base_center, p.half_width);
            push_polygon(&mut segs, &[(x2, h), (p.xt, p.yt), (x1, h)]);
        }
        Edge::Top => {
            let (x1, x2) = base_span(w, p.base_center, p.half_width);
            push_polygon(&mut segs, &[(x1, 0.0), (p.xt, p.yt), (x2, 0.0)]);
        }
        Edge::Left => {
            let (y1, y2) = base_span(h, p.base_center, p.half_width);
            push_polygon(&mut segs, &[(0.0, y1), (0.0, y2), (p.xt, p.yt)]);
        }
        Edge::Right => {
            let (y1, y2) = base_span(h, p.base_center, p.half_width);
            push_polygon(&mut segs, &[(w, y1), (p.xt, p.yt), (w, y2)]);
        }
    }

    VectorPath::new(segs, FillRule::NonZero)
}

fn ellipse_callout_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let p = pointer(w, h, adj);
    let cx = w / 2.0;
    let cy = h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;

    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        let mut segs = Vec::new();
        push_ellipse(&mut segs, cx, cy, rx, ry, true);
        return VectorPath::new(segs, FillRule::NonZero);
    }

    let ut = (p.xt - cx) / rx;
    let vt = (p.yt - cy) / ry;
    let d2 = ut * ut + vt * vt;
    let mut segs = Vec::new();
    if d2 <= 1.0 {
        push_ellipse(&mut segs, cx, cy, rx, ry, true);
        return VectorPath::new(segs, FillRule::NonZero);
    }

    let s = (d2 - 1.0).max(0.0).sqrt();
    let (u1, v1) = ((ut - s * vt) / d2, (vt + s * ut) / d2);
    let (u2, v2) = ((ut + s * vt) / d2, (vt - s * ut) / d2);
    let (px1, py1) = (cx + rx * u1, cy + ry * v1);

    let a1 = v1.atan2(u1);
    let a2 = v2.atan2(u2);
    let at = vt.atan2(ut);
    let ccw = normalize_degrees((a2 - a1).to_degrees());
    let sweep = if angle_within(at, a1, ccw) {
        ccw - 360.0
    } else {
        ccw
    };

    segs.push(PathSegment::MoveTo { x: px1, y: py1 });
    decompose_arc_to_cubic(cx, cy, rx, ry, a1.to_degrees(), sweep, &mut segs);
    segs.push(PathSegment::LineTo { x: p.xt, y: p.yt });
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

fn normalize_degrees(deg: f32) -> f32 {
    let mut v = deg % 360.0;
    if v < 0.0 {
        v += 360.0;
    }
    v
}

fn angle_within(angle: f32, start: f32, sweep_deg: f32) -> bool {
    let rel = normalize_degrees((angle - start).to_degrees());
    let sweep = normalize_degrees(sweep_deg);
    rel > 0.0 && rel < sweep
}

fn cloud_callout_path(w: f32, h: f32, adj: &[Option<f64>]) -> VectorPath {
    let p = pointer(w, h, adj);
    let cx = w / 2.0;
    let cy = h * 0.42;
    let rx = 0.4 * w;
    let ry = 0.32 * h;
    let mut segs = Vec::new();
    let mut body = Vec::with_capacity(49);
    for i in 0..48 {
        let a = std::f32::consts::TAU * i as f32 / 48.0;
        let rr = 1.0 + 0.18 * (8.0 * a).cos();
        body.push((cx + rx * rr * a.cos(), cy + ry * rr * a.sin()));
    }
    push_polygon(&mut segs, &body);

    let bubble_r = 0.09 * w.min(h);
    for (frac, scale) in [(0.55_f32, 1.0_f32), (0.8, 0.62)] {
        let bx = cx + (p.xt - cx) * frac;
        let by = cy + (p.yt - cy) * frac;
        let r = bubble_r * scale;
        let bx = bx.clamp(r, (w - r).max(r));
        let by = by.clamp(r, (h - r).max(r));
        push_ellipse(&mut segs, bx, by, r, r, true);
    }
    VectorPath::new(segs, FillRule::NonZero)
}

/// Leader-line callout family (`lineCallout*`, `borderCallout*`,
/// `accentCallout*`). The frame and accent bar are filled subpaths; the leader
/// itself is an open polyline so it only appears when the shape is stroked.
fn leader_path(
    w: f32,
    h: f32,
    adj: &[Option<f64>],
    border: bool,
    accent: bool,
    style: u8,
) -> VectorPath {
    let p = pointer(w, h, adj);
    let (ax, ay) = match p.edge {
        Edge::Top => (w * p.base_center, 0.0),
        Edge::Left => (0.0, h * p.base_center),
        Edge::Right => (w, h * p.base_center),
        _ => (w * p.base_center, h),
    };

    let mut segs = Vec::new();
    if border {
        push_polygon(&mut segs, &[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
    }
    if accent {
        push_polygon(
            &mut segs,
            &[
                (0.0, 0.12 * h),
                (0.06 * w, 0.12 * h),
                (0.06 * w, 0.88 * h),
                (0.0, 0.88 * h),
            ],
        );
    }

    let mut leader = vec![(ax, ay)];
    match style {
        1 => leader.push((p.xt, p.yt)),
        2 => {
            if p.edge == Edge::Top || p.edge == Edge::Bottom || p.edge == Edge::Inside {
                leader.push((p.xt, ay));
            } else {
                leader.push((ax, p.yt));
            }
            leader.push((p.xt, p.yt));
        }
        _ => {
            if p.edge == Edge::Top || p.edge == Edge::Bottom || p.edge == Edge::Inside {
                let mid_x = ax + 0.35 * (p.xt - ax);
                leader.push((mid_x, ay));
                leader.push((mid_x, p.yt));
            } else {
                let mid_y = ay + 0.35 * (p.yt - ay);
                leader.push((ax, mid_y));
                leader.push((p.xt, mid_y));
            }
            leader.push((p.xt, p.yt));
        }
    }

    let Some(&(x0, y0)) = leader.first() else {
        return VectorPath::new(segs, FillRule::NonZero);
    };
    segs.push(PathSegment::MoveTo { x: x0, y: y0 });
    for &(x, y) in &leader[1..] {
        segs.push(PathSegment::LineTo { x, y });
    }
    VectorPath::new(segs, FillRule::NonZero)
}

/// Pointer used by the arrow callouts: a wedge rooted inside the arrow body so
/// the composite silhouette stays connected for every arrow direction.
pub fn push_arrow_callout_pointer(
    segments: &mut Vec<PathSegment>,
    w: f32,
    h: f32,
    adj: &[Option<f64>],
) {
    let p = pointer(w, h, adj);
    match p.edge {
        Edge::Inside => {}
        Edge::Bottom => push_polygon(
            segments,
            &[(0.6 * w, 0.55 * h), (p.xt, p.yt), (0.4 * w, 0.55 * h)],
        ),
        Edge::Top => push_polygon(
            segments,
            &[(0.4 * w, 0.45 * h), (p.xt, p.yt), (0.6 * w, 0.45 * h)],
        ),
        Edge::Left => push_polygon(
            segments,
            &[(0.45 * w, 0.6 * h), (p.xt, p.yt), (0.45 * w, 0.4 * h)],
        ),
        Edge::Right => push_polygon(
            segments,
            &[(0.55 * w, 0.4 * h), (p.xt, p.yt), (0.55 * w, 0.6 * h)],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wedge_target_selects_bottom_edge() {
        let p = pointer(200.0, 100.0, &[]);
        assert_eq!(p.edge, Edge::Bottom);
        assert!((p.xt - (-40.0)).abs() < 1e-3);
        assert!((p.yt - 130.0).abs() < 1e-3);
    }

    #[test]
    fn wedge_target_inside_is_deadband() {
        let p = pointer(200.0, 100.0, &[Some(50_000.0), Some(50_000.0)]);
        assert_eq!(p.edge, Edge::Inside);
    }
}
