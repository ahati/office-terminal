//! `trimGeometryPathEnds` (ref `packages/core/src/shape/arrow-geometry.ts:271-333`).
//!
//! Shortens a stroked path at its open ends so the line stops where Office stops
//! it. The distance is measured straight from the end point: the path is cut
//! where it last (tail) or first (head) crosses the circle of that radius, so
//! curves are split with de Casteljau and closed subpaths are left alone.
//!
//! `VectorPath` carries only line/quad/cubic segments (preset arcs are already
//! flattened), so the reference's arc/ellipse cases reduce to those three.

use super::Point;

/// A de Casteljau control polygon.
type Polygon = Vec<Point>;
use crate::contracts::{PathSegment, VectorPath};

// ── trimGeometryPathEnds ────────────────────────────────────────────────────

/// One drawn piece of a subpath (the reference's `TrimSegment`).
#[derive(Clone, Copy, Debug)]
enum TrimSeg {
    Line {
        from: (f32, f32),
        to: (f32, f32),
    },
    Quad {
        p0: (f32, f32),
        c: (f32, f32),
        p1: (f32, f32),
    },
    Cubic {
        p0: (f32, f32),
        c1: (f32, f32),
        c2: (f32, f32),
        p1: (f32, f32),
    },
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// De Casteljau split of a control polygon at `t`.
fn split_polygon(points: &[Point], t: f32) -> (Polygon, Polygon) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut q = points.to_vec();
    while !q.is_empty() {
        left.push(q[0]);
        right.insert(0, q[q.len() - 1]);
        q = q.windows(2).map(|w| lerp(w[0], w[1], t)).collect();
    }
    (left, right)
}

impl TrimSeg {
    fn at(&self, t: f32) -> (f32, f32) {
        match *self {
            TrimSeg::Line { from, to } => lerp(from, to, t),
            TrimSeg::Quad { p0, c, p1 } => {
                let (left, _) = split_polygon(&[p0, c, p1], t);
                *left.last().unwrap_or(&p0)
            }
            TrimSeg::Cubic { p0, c1, c2, p1 } => {
                let (left, _) = split_polygon(&[p0, c1, c2, p1], t);
                *left.last().unwrap_or(&p0)
            }
        }
    }

    fn left(&self, t: f32) -> PathSegment {
        match *self {
            TrimSeg::Line { from, to } => {
                let p = lerp(from, to, t);
                PathSegment::LineTo { x: p.0, y: p.1 }
            }
            TrimSeg::Quad { p0, c, p1 } => {
                let (left, _) = split_polygon(&[p0, c, p1], t);
                PathSegment::QuadTo {
                    x1: left[1].0,
                    y1: left[1].1,
                    x: left[2].0,
                    y: left[2].1,
                }
            }
            TrimSeg::Cubic { p0, c1, c2, p1 } => {
                let (left, _) = split_polygon(&[p0, c1, c2, p1], t);
                PathSegment::CubicTo {
                    x1: left[1].0,
                    y1: left[1].1,
                    x2: left[2].0,
                    y2: left[2].1,
                    x: left[3].0,
                    y: left[3].1,
                }
            }
        }
    }

    fn right(&self, t: f32) -> PathSegment {
        match *self {
            TrimSeg::Line { from, to } => {
                let _ = from;
                PathSegment::LineTo { x: to.0, y: to.1 }
            }
            TrimSeg::Quad { p0, c, p1 } => {
                let (_, right) = split_polygon(&[p0, c, p1], t);
                PathSegment::QuadTo {
                    x1: right[1].0,
                    y1: right[1].1,
                    x: right[2].0,
                    y: right[2].1,
                }
            }
            TrimSeg::Cubic { p0, c1, c2, p1 } => {
                let (_, right) = split_polygon(&[p0, c1, c2, p1], t);
                PathSegment::CubicTo {
                    x1: right[1].0,
                    y1: right[1].1,
                    x2: right[2].0,
                    y2: right[2].1,
                    x: right[3].0,
                    y: right[3].1,
                }
            }
        }
    }
}

/// A subpath as the reference's `trimSegments` sees it.
struct Subpath {
    start: (f32, f32),
    /// `(command index, segment)` pairs. Command indices point into the flat
    /// `PathSegment` list so a trim can splice the untouched prefix/suffix.
    segments: Vec<(usize, TrimSeg)>,
    closed: bool,
}

fn subpaths(segments: &[PathSegment]) -> Vec<Subpath> {
    let mut out: Vec<Subpath> = Vec::new();
    let mut pen: Option<(f32, f32)> = None;
    for (index, segment) in segments.iter().enumerate() {
        match *segment {
            PathSegment::MoveTo { x, y } => {
                pen = Some((x, y));
                out.push(Subpath {
                    start: (x, y),
                    segments: Vec::new(),
                    closed: false,
                });
            }
            PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => {
                let from = pen.unwrap_or((x, y));
                if out.is_empty() {
                    out.push(Subpath {
                        start: from,
                        segments: Vec::new(),
                        closed: false,
                    });
                }
                let seg = match *segment {
                    PathSegment::LineTo { x, y } => TrimSeg::Line {
                        from,
                        to: (x, y),
                    },
                    PathSegment::QuadTo { x1, y1, x, y } => TrimSeg::Quad {
                        p0: from,
                        c: (x1, y1),
                        p1: (x, y),
                    },
                    PathSegment::CubicTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    } => TrimSeg::Cubic {
                        p0: from,
                        c1: (x1, y1),
                        c2: (x2, y2),
                        p1: (x, y),
                    },
                    _ => unreachable!(),
                };
                if let Some(current) = out.last_mut() {
                    current.segments.push((index, seg));
                }
                pen = Some((x, y));
            }
            PathSegment::Close => {
                if let Some(current) = out.last_mut() {
                    current.closed = true;
                }
            }
        }
    }
    out
}

/// Parameter where `segment`, walked from its `from` end, first reaches
/// distance `d` from `anchor`; `None` when the whole segment stays inside.
/// ref arrow-geometry.ts:271-289
fn reach_parameter(seg: &TrimSeg, from_end: bool, anchor: (f32, f32), d: f32) -> Option<f32> {
    const STEPS: usize = 64;
    let distance = |t: f32| {
        let q = seg.at(t);
        ((q.0 - anchor.0).powi(2) + (q.1 - anchor.1).powi(2)).sqrt()
    };
    let at = |k: f32| if from_end { 1.0 - k } else { k };
    let mut inside = 0.0f32;
    for i in 0..=STEPS {
        let k = i as f32 / STEPS as f32;
        if distance(at(k)) >= d {
            if i == 0 {
                return Some(at(0.0));
            }
            let (mut lo, mut hi) = (inside, k);
            for _ in 0..50 {
                let mid = (lo + hi) / 2.0;
                if distance(at(mid)) >= d {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return Some(at(hi));
        }
        inside = k;
    }
    None
}

fn trim_tail(commands: &[PathSegment], d: f32) -> Vec<PathSegment> {
    let subs = subpaths(commands);
    let Some(last) = subs.last() else {
        return commands.to_vec();
    };
    if last.closed || last.segments.is_empty() {
        return commands.to_vec();
    }
    let Some((_, last_seg)) = last.segments.last() else {
        return commands.to_vec();
    };
    let anchor = last_seg.at(1.0);
    for (index, seg) in last.segments.iter().rev() {
        if let Some(t) = reach_parameter(seg, true, anchor, d) {
            let mut out = commands[..*index].to_vec();
            out.push(seg.left(t));
            return out;
        }
    }
    // The whole subpath lies within reach: nothing of it is stroked.
    commands[..last.segments[0].0].to_vec()
}

fn trim_head(commands: &[PathSegment], d: f32) -> Vec<PathSegment> {
    let subs = subpaths(commands);
    let Some(first) = subs.first() else {
        return commands.to_vec();
    };
    if first.closed
        || first.segments.is_empty()
        || !matches!(commands.first(), Some(PathSegment::MoveTo { .. }))
    {
        return commands.to_vec();
    }
    let anchor = first.start;
    for (index, seg) in &first.segments {
        if let Some(t) = reach_parameter(seg, false, anchor, d) {
            let point = seg.at(t);
            let mut out = vec![PathSegment::MoveTo {
                x: point.0,
                y: point.1,
            }];
            out.push(seg.right(t));
            out.extend_from_slice(&commands[index + 1..]);
            return out;
        }
    }
    commands[first.segments[first.segments.len() - 1].0 + 1..].to_vec()
}

/// Shorten a stroked path at its open ends so the line stops `head` / `tail`
/// EMU from its first / last point, cutting where the path last (tail) or first
/// (head) crosses that radius. Closed subpaths are left alone.
/// ref arrow-geometry.ts:326-333 (`trimGeometryPathEnds`)
pub fn trim_path_ends(path: &VectorPath, head: f32, tail: f32) -> VectorPath {
    if head <= 0.0 && tail <= 0.0 {
        return path.clone();
    }
    let mut commands = path.segments.clone();
    if tail > 0.0 {
        commands = trim_tail(&commands, tail);
    }
    if head > 0.0 {
        commands = trim_head(&commands, head);
    }
    VectorPath::new(commands, path.fill_rule)
}
