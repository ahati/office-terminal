//! Extended rectangle, frame, plaque and math-sign presets.
//!
//! Covers PRD 11 §2.1 (multi-corner rectangles) plus the enclosure frames,
//! plaques, corner tabs and math glyph presets from the extended catalog.

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;
use super::util::{mirror_points_x, mirror_points_y, norm_adj, push_polygon, push_round_rect};

/// Maximum corner cut/radius: half the smaller side (PRD 11 §2.1).
#[inline]
fn max_cut(w: f32, h: f32) -> f32 {
    0.5 * w.min(h).max(0.0)
}

/// Corner offset for handle `a`, `d = R_max · clamp(a, 0, 1)`.
#[inline]
fn cut(w: f32, h: f32, a: f32) -> f32 {
    max_cut(w, h) * a.clamp(0.0, 1.0)
}

pub fn build_rect_preset(geometry: &str, w: f32, h: f32, adj: &[Option<f64>]) -> Option<VectorPath> {
    let path = match geometry {
        "snip1Rect" => {
            let d1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            rect_path(vec![(0.0, 0.0), (w - d1, 0.0), (w, d1), (w, h), (0.0, h)])
        }

        "snip2SameRect" => {
            let d1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let d2 = cut(w, h, norm_adj(adj, 1, 0.16667, 0.0, 1.0));
            rect_path(vec![(d1, 0.0), (w - d2, 0.0), (w, d2), (w, h), (0.0, h), (0.0, d1)])
        }

        "snip2DiagRect" => {
            let d1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let d2 = cut(w, h, norm_adj(adj, 1, 0.16667, 0.0, 1.0));
            rect_path(vec![(d1, 0.0), (w, 0.0), (w, h - d2), (w - d2, h), (0.0, h), (0.0, d1)])
        }

        "snipRoundRect" => {
            let d1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let r2 = cut(w, h, norm_adj(adj, 1, 0.16667, 0.0, 1.0));
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: d1, y: 0.0 });
            segs.push(PathSegment::LineTo { x: w - r2, y: 0.0 });
            decompose_arc_to_cubic(w - r2, r2, r2, r2, 270.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: h });
            segs.push(PathSegment::LineTo { x: 0.0, y: h });
            segs.push(PathSegment::LineTo { x: 0.0, y: d1 });
            segs.push(PathSegment::Close);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "round1Rect" => {
            let r1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: 0.0, y: 0.0 });
            segs.push(PathSegment::LineTo { x: w - r1, y: 0.0 });
            decompose_arc_to_cubic(w - r1, r1, r1, r1, 270.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: h });
            segs.push(PathSegment::LineTo { x: 0.0, y: h });
            segs.push(PathSegment::Close);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "round2SameRect" => {
            let r1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let r2 = cut(w, h, norm_adj(adj, 1, 0.16667, 0.0, 1.0));
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: 0.0, y: r1 });
            decompose_arc_to_cubic(r1, r1, r1, r1, 180.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w - r2, y: 0.0 });
            decompose_arc_to_cubic(w - r2, r2, r2, r2, 270.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: h });
            segs.push(PathSegment::LineTo { x: 0.0, y: h });
            segs.push(PathSegment::Close);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "round2DiagRect" => {
            let r1 = cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0));
            let r2 = cut(w, h, norm_adj(adj, 1, 0.16667, 0.0, 1.0));
            let mut segs = Vec::new();
            segs.push(PathSegment::MoveTo { x: 0.0, y: r1 });
            decompose_arc_to_cubic(r1, r1, r1, r1, 180.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: w, y: 0.0 });
            segs.push(PathSegment::LineTo { x: w, y: h - r2 });
            decompose_arc_to_cubic(w - r2, h - r2, r2, r2, 0.0, 90.0, &mut segs);
            segs.push(PathSegment::LineTo { x: 0.0, y: h });
            segs.push(PathSegment::Close);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "halfFrame" | "corner" => {
            let a1 = norm_adj(adj, 0, 0.33333, 0.0, 0.5);
            let a2 = norm_adj(adj, 1, 0.33333, 0.0, 0.5);
            rect_path(vec![
                (0.0, 0.0),
                (w, 0.0),
                (w, h * a1),
                (w * a2, h * a1),
                (w * a2, h),
                (0.0, h),
            ])
        }

        "diagStripe" => {
            let t = (w.min(h) * norm_adj(adj, 0, 0.16667, 0.0, 0.9)).min(0.5 * w);
            rect_path(vec![(0.0, 0.0), (t, 0.0), (w, h - t), (w, h), (w - t, h), (0.0, t)])
        }

        "plaque" => plaque_path(w, h, cut(w, h, norm_adj(adj, 0, 0.16667, 0.0, 1.0))),

        "plaqueTabs" => {
            let tw = (w * norm_adj(adj, 0, 0.15, 0.02, 0.45)).min(0.4 * w);
            let th = (h * norm_adj(adj, 1, 0.12, 0.02, 0.45)).min(0.4 * h);
            let tab_h = 0.5 * h * norm_adj(adj, 2, 0.3, 0.0, 0.6);
            let r = 0.08 * w.min(h);
            let mut segs = Vec::new();
            push_round_rect(&mut segs, tw, th, w - tw, h - th, r, true);
            push_polygon(
                &mut segs,
                &[
                    (0.0, h / 2.0 - tab_h),
                    (tw, h / 2.0 - tab_h),
                    (tw, h / 2.0 + tab_h),
                    (0.0, h / 2.0 + tab_h),
                ],
            );
            push_polygon(
                &mut segs,
                &[
                    (w, h / 2.0 - tab_h),
                    (w - tw, h / 2.0 - tab_h),
                    (w - tw, h / 2.0 + tab_h),
                    (w, h / 2.0 + tab_h),
                ],
            );
            VectorPath::new(segs, FillRule::NonZero)
        }

        "squareTabs" => {
            let t = (w.min(h) * norm_adj(adj, 0, 0.2, 0.02, 0.45)).max(0.0);
            let mut segs = Vec::new();
            push_square(&mut segs, w / 2.0 - t / 2.0, 0.0, w / 2.0 + t / 2.0, t);
            push_square(&mut segs, w / 2.0 - t / 2.0, h - t, w / 2.0 + t / 2.0, h);
            push_square(&mut segs, 0.0, h / 2.0 - t / 2.0, t, h / 2.0 + t / 2.0);
            push_square(&mut segs, w - t, h / 2.0 - t / 2.0, w, h / 2.0 + t / 2.0);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "cornerTabs" => {
            let s = (w.min(h) * norm_adj(adj, 0, 0.2, 0.02, 0.45)).max(0.0);
            let t = s * 0.42;
            let tl = vec![(0.0, 0.0), (s, 0.0), (s, t), (t, t), (t, s), (0.0, s)];
            let tr = mirror_points_x(&tl, w);
            let bl = mirror_points_y(&tl, h);
            let br = mirror_points_x(&bl, w);
            let mut segs = Vec::new();
            for poly in [&tl, &tr, &bl, &br] {
                push_polygon(&mut segs, poly);
            }
            VectorPath::new(segs, FillRule::NonZero)
        }

        "frame" => {
            let inset = (w.min(h) * norm_adj(adj, 0, 0.16667, 0.0, 0.49)).max(0.0);
            hollow_rect_path(w, h, inset)
        }

        "bevel" => {
            let inset = (w.min(h) * norm_adj(adj, 0, 0.1, 0.0, 0.49)).max(0.0);
            let mut segs = Vec::new();
            push_round_rect(&mut segs, 0.0, 0.0, w, h, 0.16 * w.min(h), true);
            push_polygon(
                &mut segs,
                &[
                    (inset, inset),
                    (w - inset, inset),
                    (w - inset, h - inset),
                    (inset, h - inset),
                ],
            );
            VectorPath::new(segs, FillRule::EvenOdd)
        }

        "folderCorner" => {
            let fw = w * norm_adj(adj, 0, 0.2, 0.0, 0.4);
            let fh = h * norm_adj(adj, 1, 0.2, 0.0, 0.4);
            let mut segs = Vec::new();
            push_polygon(&mut segs, &[(0.0, 0.0), (w - fw, 0.0), (w, fh), (w, h), (0.0, h)]);
            push_polygon(&mut segs, &[(w - fw, 0.0), (w, fh), (w - fw, fh)]);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "plus" | "mathPlus" => {
            let a1 = norm_adj(adj, 0, 0.25, 0.05, 0.95);
            cross_points(w, h, a1)
        }

        "mathMultiply" => {
            let t = (w.min(h) * norm_adj(adj, 0, 0.25, 0.02, 0.6)).max(0.0);
            let mut segs = Vec::new();
            push_polygon(
                &mut segs,
                &[(0.0, 0.0), (t, 0.0), (w, h - t), (w, h), (w - t, h), (0.0, t)],
            );
            push_polygon(
                &mut segs,
                &[(w - t, 0.0), (w, 0.0), (w, t), (t, h), (0.0, h), (0.0, h - t)],
            );
            VectorPath::new(segs, FillRule::NonZero)
        }

        "mathDivide" => {
            let bar = h * norm_adj(adj, 0, 0.08, 0.0, 0.2).max(0.0);
            let r = (w.min(h) * norm_adj(adj, 1, 0.08, 0.02, 0.2))
                .min(0.45 * h)
                .min(0.45 * w);
            let mut segs = Vec::new();
            push_square(&mut segs, 0.0, h / 2.0 - bar / 2.0, w, h / 2.0 + bar / 2.0);
            super::util::push_ellipse(&mut segs, w / 2.0, (h * 0.15).max(r), r, r, true);
            super::util::push_ellipse(&mut segs, w / 2.0, (h * 0.85).min(h - r), r, r, true);
            VectorPath::new(segs, FillRule::NonZero)
        }

        "mathEqual" | "mathNotEqual" => {
            let bar = (h * norm_adj(adj, 0, 0.08, 0.01, 0.18)).max(0.0);
            let mut segs = Vec::new();
            push_square(&mut segs, 0.0, h * 0.3 - bar / 2.0, w, h * 0.3 + bar / 2.0);
            push_square(&mut segs, 0.0, h * 0.7 - bar / 2.0, w, h * 0.7 + bar / 2.0);
            if geometry == "mathNotEqual" {
                let t = (w.min(h) * 0.09).max(0.0);
                push_polygon(
                    &mut segs,
                    &[(0.12 * w, h), (0.12 * w + t, h), (0.88 * w, 0.0), (0.88 * w - t, 0.0)],
                );
            }
            VectorPath::new(segs, FillRule::NonZero)
        }

        "nonIsoscelesTriangle" => {
            let apex = w * norm_adj(adj, 0, 0.5, 0.0, 1.0);
            let inset = w * norm_adj(adj, 1, 0.0, 0.0, 1.0);
            let base = (w - inset).max(apex);
            rect_path(vec![(apex, 0.0), (base, h), (0.0, h)])
        }

        _ => return None,
    };
    Some(path)
}

fn rect_path(points: Vec<(f32, f32)>) -> VectorPath {
    super::util::polygon(&points, FillRule::NonZero)
}

fn push_square(segments: &mut Vec<PathSegment>, x0: f32, y0: f32, x1: f32, y1: f32) {
    push_polygon(segments, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
}

fn cross_points(w: f32, h: f32, a1: f32) -> VectorPath {
    let tx = w * a1;
    let ty = h * a1;
    let x1 = (w - tx) / 2.0;
    let x2 = (w + tx) / 2.0;
    let y1 = (h - ty) / 2.0;
    let y2 = (h + ty) / 2.0;
    rect_path(vec![
        (x1, 0.0),
        (x2, 0.0),
        (x2, y1),
        (w, y1),
        (w, y2),
        (x2, y2),
        (x2, h),
        (x1, h),
        (x1, y2),
        (0.0, y2),
        (0.0, y1),
        (x1, y1),
    ])
}

/// Rectangle with four concave quarter-circle corner scoops (PRD 11 §2.5).
fn plaque_path(w: f32, h: f32, r: f32) -> VectorPath {
    let mut segs = Vec::new();
    segs.push(PathSegment::MoveTo { x: 0.0, y: r });
    decompose_arc_to_cubic(0.0, 0.0, r, r, 90.0, -90.0, &mut segs);
    segs.push(PathSegment::LineTo { x: w - r, y: 0.0 });
    decompose_arc_to_cubic(w, 0.0, r, r, 180.0, -90.0, &mut segs);
    segs.push(PathSegment::LineTo { x: w, y: h - r });
    decompose_arc_to_cubic(w, h, r, r, 270.0, -90.0, &mut segs);
    segs.push(PathSegment::LineTo { x: r, y: h });
    decompose_arc_to_cubic(0.0, h, r, r, 0.0, -90.0, &mut segs);
    segs.push(PathSegment::Close);
    VectorPath::new(segs, FillRule::NonZero)
}

/// Outer rectangle with a rectangular hole, rendered with the even-odd rule.
fn hollow_rect_path(w: f32, h: f32, inset: f32) -> VectorPath {
    let mut segs = Vec::new();
    push_polygon(&mut segs, &[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
    if inset > 0.0 && w > 2.0 * inset && h > 2.0 * inset {
        push_polygon(
            &mut segs,
            &[
                (inset, inset),
                (w - inset, inset),
                (w - inset, h - inset),
                (inset, h - inset),
            ],
        );
    }
    VectorPath::new(segs, FillRule::EvenOdd)
}
