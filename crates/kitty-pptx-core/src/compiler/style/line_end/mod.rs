//! DrawingML line ends (`a:headEnd` / `a:tailEnd`) and compound lines
//! (`a:ln@cmpd`).
//!
//! Ports `packages/core/src/shape/arrow-geometry.ts` (decoration metrics, the
//! observed Office retraction table and `trimGeometryPathEnds`) plus the
//! connector re-stroke in `packages/pptx/src/renderer.ts:4380-4535, 7429-7530`.
//!
//! Every length here is authored EMU, so the projection is scale-free: the
//! reference computes in device px with `scale` = px per EMU and the retraction,
//! arm offsets and radii all carry that same factor. Working in EMU lets the
//! compiler emit trimmed shafts and decoration paths into the display list, so
//! the rasterizer and the SVG serializer paint byte-identical geometry.

pub mod trim;

pub use self::trim::trim_path_ends;

use crate::contracts::{FillRule, PathSegment, VectorPath};

/// One point in the path's local space.
type Point = (f32, f32);
/// One compound-line rail: start, end and painted width in EMU.
pub type Rail = (Point, Point, f32);

/// Full span across the line, in `e` units (`SPAN`).
const SPAN: [f32; 3] = [2.0, 3.0, 5.0];
/// `stealth` notch behind the tip, in `e` units (`STEALTH_NOTCH`).
const STEALTH_NOTCH: [f32; 3] = [1.0, 2.0, 3.0];
/// Open-arrow arm extent along the line, in `e` units (`OPEN_ARM_ALONG`).
const OPEN_ARM_ALONG: [f32; 3] = [2.0, 3.0, 4.5];
/// Open-arrow arm offset across the line, in `e` units (`OPEN_ARM_ACROSS`).
const OPEN_ARM_ACROSS: [f32; 3] = [1.25, 1.75, 2.5];
/// Line widths below 2 pt size their decorations as a 2 pt line.
const LINE_END_MIN_BASE_PT: f32 = 2.0;
/// EMU in one point (`PT_TO_EMU` in the reference).
pub const EMU_PER_POINT: f32 = 12_700.0;

/// `sm` / `med` / `lg` index; anything else is `med`.
fn step(value: &str) -> usize {
    match value {
        "sm" => 0,
        "lg" => 2,
        _ => 1,
    }
}

/// Device/EMU line-end metrics. `lw` is the painted line width; `e` the size
/// unit (`max(line width, 2 pt)`). ref arrow-geometry.ts:107-116
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEndMetrics {
    pub lw: f32,
    pub e: f32,
    pub width: f32,
    pub length: f32,
}

pub fn metrics(width_emu: i64, end: &pptx_model::ArrowEnd) -> LineEndMetrics {
    let width = width_emu as f32;
    let lw = width.max(0.5);
    let e = width.max(LINE_END_MIN_BASE_PT * EMU_PER_POINT);
    LineEndMetrics {
        lw,
        e,
        width: SPAN[step(&end.w)] * e,
        length: SPAN[step(&end.len)] * e,
    }
}

/// Open-arrow arm centreline: vertex x (≤ 0), arm extent `along` behind it and
/// `across` off axis. ref arrow-geometry.ts:118-123
fn open_arm(end: &pptx_model::ArrowEnd, m: LineEndMetrics) -> (f32, f32, f32) {
    let along = OPEN_ARM_ALONG[step(&end.len)] * m.e;
    let across = OPEN_ARM_ACROSS[step(&end.w)] * m.e;
    let sin = across / (along * along + across * across).sqrt();
    (-(m.lw / 2.0) / sin, along, across)
}

/// How far the line is pulled back from its end so it stops where Office stops
/// it. Zero for `diamond`, `oval` and `none`. ref arrow-geometry.ts:128-139
pub fn retract(width_emu: i64, end: &pptx_model::ArrowEnd) -> f32 {
    let m = metrics(width_emu, end);
    match end.kind.as_str() {
        "triangle" => m.length - m.e / 2.0,
        "stealth" => STEALTH_NOTCH[step(&end.len)] * m.e,
        "arrow" => -open_arm(end, m).0,
        _ => 0.0,
    }
}

/// One painted line-end decoration.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrowGeometry {
    /// Outline in the end's local frame: the tip sits at `(0, 0)`, `-x` points
    /// back into the line and `y` spans the line width.
    /// ref arrow-geometry.ts:161-188 (`resolveArrowGeometry`).
    pub path: VectorPath,
    /// Filled outlines (`triangle`, `stealth`, `diamond`, `oval`) paint with the
    /// line's paint; the open `arrow` is two separately stroked arms.
    pub filled: bool,
    /// Stroke width for the open arrow (always the line width).
    pub stroke_width: f32,
}

/// Geometry of one `a:headEnd` / `a:tailEnd`. `None` for `none`/unknown types.
/// ref arrow-geometry.ts:161-188
pub fn arrow_geometry(width_emu: i64, end: &pptx_model::ArrowEnd) -> Option<ArrowGeometry> {
    let m = metrics(width_emu, end);
    let half_w = m.width / 2.0;
    let len = m.length;
    let mut segments = Vec::new();
    let filled = match end.kind.as_str() {
        "triangle" => {
            segments.push(PathSegment::MoveTo { x: 0.0, y: 0.0 });
            segments.push(PathSegment::LineTo { x: -len, y: -half_w });
            segments.push(PathSegment::LineTo { x: -len, y: half_w });
            segments.push(PathSegment::Close);
            true
        }
        "stealth" => {
            let notch = -STEALTH_NOTCH[step(&end.len)] * m.e;
            segments.push(PathSegment::MoveTo { x: 0.0, y: 0.0 });
            segments.push(PathSegment::LineTo { x: -len, y: -half_w });
            segments.push(PathSegment::LineTo { x: notch, y: 0.0 });
            segments.push(PathSegment::LineTo { x: -len, y: half_w });
            segments.push(PathSegment::Close);
            true
        }
        "arrow" => {
            let (vertex, along, across) = open_arm(end, m);
            segments.push(PathSegment::MoveTo {
                x: vertex - along,
                y: -across,
            });
            segments.push(PathSegment::LineTo { x: vertex, y: 0.0 });
            segments.push(PathSegment::LineTo {
                x: vertex - along,
                y: across,
            });
            false
        }
        "diamond" => {
            segments.push(PathSegment::MoveTo { x: len / 2.0, y: 0.0 });
            segments.push(PathSegment::LineTo { x: 0.0, y: -half_w });
            segments.push(PathSegment::LineTo { x: -len / 2.0, y: 0.0 });
            segments.push(PathSegment::LineTo { x: 0.0, y: half_w });
            segments.push(PathSegment::Close);
            true
        }
        "oval" => {
            // Canvas `ellipse(cx, cy, rx, ry, …)`; the IR has no arc segment, so
            // the ellipse is flattened with four cubic quarters (the same
            // kappa the reference's canvas backend uses implicitly).
            for segment in flatten_ellipse(0.0, 0.0, len / 2.0, half_w) {
                segments.push(segment);
            }
            true
        }
        _ => return None,
    };
    Some(ArrowGeometry {
        path: VectorPath::new(segments, FillRule::NonZero),
        filled,
        stroke_width: m.lw,
    })
}

/// Four-quarter cubic approximation of an ellipse (kappa 0.5523).
fn flatten_ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<PathSegment> {
    const K: f32 = 0.552_284_8;
    let (kx, ky) = (rx * K, ry * K);
    vec![
        PathSegment::MoveTo {
            x: cx + rx,
            y: cy,
        },
        PathSegment::CubicTo {
            x1: cx + rx,
            y1: cy + ky,
            x2: cx + kx,
            y2: cy + ry,
            x: cx,
            y: cy + ry,
        },
        PathSegment::CubicTo {
            x1: cx - kx,
            y1: cy + ry,
            x2: cx - rx,
            y2: cy + ky,
            x: cx - rx,
            y: cy,
        },
        PathSegment::CubicTo {
            x1: cx - rx,
            y1: cy - ky,
            x2: cx - kx,
            y2: cy - ry,
            x: cx,
            y: cy - ry,
        },
        PathSegment::CubicTo {
            x1: cx + kx,
            y1: cy - ry,
            x2: cx + rx,
            y2: cy - ky,
            x: cx + rx,
            y: cy,
        },
        PathSegment::Close,
    ]
}

/// One open end of a path: its point and the outward direction (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathEnd {
    pub point: (f32, f32),
    pub angle: f32,
}

/// The outward frame of a path's head (start) and tail (end) ends.
///
/// The reference derives both from the leader's vertices (`getConnectorAnchors`)
/// and, for custom geometry, from `getCustGeomEndpoints`; here they come from
/// the emitted path itself, which is the same polyline. Each `angle` is the
/// outward direction, i.e. the direction the decoration points in.
/// ref renderer.ts:4465-4535
pub fn path_ends(path: &VectorPath) -> Option<(PathEnd, PathEnd)> {
    let start = path.segments.iter().find_map(|segment| match segment {
        PathSegment::MoveTo { x, y } => Some((*x, *y)),
        _ => None,
    })?;
    // Walk every drawn vertex in order: the first neighbour frames the head,
    // the last two frame the tail.
    let mut points: Vec<(f32, f32)> = Vec::new();
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => points.push((*x, *y)),
            PathSegment::QuadTo { x, y, .. } | PathSegment::CubicTo { x, y, .. } => {
                points.push((*x, *y))
            }
            PathSegment::Close => {}
        }
    }
    let tip = points.last().copied()?;
    let before_tip = if points.len() >= 2 {
        points[points.len() - 2]
    } else {
        start
    };
    // The first vertex after the leading `MoveTo` sets the head direction.
    let first_end = points.get(1).copied().unwrap_or(start);
    let head = PathEnd {
        point: start,
        angle: (start.1 - first_end.1).atan2(start.0 - first_end.0),
    };
    let tail = PathEnd {
        point: tip,
        angle: (tip.1 - before_tip.1).atan2(tip.0 - before_tip.0),
    };
    Some((head, tail))
}

/// Place a decoration authored in its end frame (`tip` at the origin, `-x` back
/// into the line) at `end`.
pub fn place(geometry: &ArrowGeometry, end: PathEnd) -> VectorPath {
    let (sin, cos) = end.angle.sin_cos();
    let map = |x: f32, y: f32| {
        (
            end.point.0 + x * cos - y * sin,
            end.point.1 + x * sin + y * cos,
        )
    };
    let segments = geometry
        .path
        .segments
        .iter()
        .map(|segment| match *segment {
            PathSegment::MoveTo { x, y } => {
                let (px, py) = map(x, y);
                PathSegment::MoveTo { x: px, y: py }
            }
            PathSegment::LineTo { x, y } => {
                let (px, py) = map(x, y);
                PathSegment::LineTo { x: px, y: py }
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (ax, ay) = map(x1, y1);
                let (bx, by) = map(x, y);
                PathSegment::QuadTo {
                    x1: ax,
                    y1: ay,
                    x: bx,
                    y: by,
                }
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (ax, ay) = map(x1, y1);
                let (bx, by) = map(x2, y2);
                let (cx2, cy2) = map(x, y);
                PathSegment::CubicTo {
                    x1: ax,
                    y1: ay,
                    x2: bx,
                    y2: by,
                    x: cx2,
                    y: cy2,
                }
            }
            PathSegment::Close => PathSegment::Close,
        })
        .collect();
    VectorPath::new(segments, geometry.path.fill_rule)
}

/// Straight run (`MoveTo` + one `LineTo`) of an open path, if it is one.
pub fn straight_run(path: &VectorPath) -> Option<((f32, f32), (f32, f32))> {
    let mut start = None;
    let mut end = None;
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y } if start.is_none() => start = Some((*x, *y)),
            PathSegment::LineTo { x, y } if start.is_some() && end.is_none() => {
                end = Some((*x, *y))
            }
            PathSegment::Close => return None,
            _ => return None,
        }
    }
    Some((start?, end?))
}

/// Offsets (perpendicular, as a fraction of the line width) and widths (as a
/// fraction of the line width) of one `a:ln@cmpd` style.
/// ref renderer.ts:7478-7510 (`drawCompoundLine`)
pub fn compound_subs(cmpd: &str) -> Option<&'static [(f32, f32)]> {
    const DBL: &[(f32, f32)] = &[(-1.0 / 3.0, 1.0 / 3.0), (1.0 / 3.0, 1.0 / 3.0)];
    const THIN_THICK: &[(f32, f32)] = &[(-3.0 / 8.0, 1.0 / 4.0), (1.0 / 4.0, 1.0 / 2.0)];
    const THICK_THIN: &[(f32, f32)] = &[(-1.0 / 4.0, 1.0 / 2.0), (3.0 / 8.0, 1.0 / 4.0)];
    const TRI: &[(f32, f32)] = &[
        (-2.0 / 5.0, 1.0 / 5.0),
        (0.0, 3.0 / 5.0),
        (2.0 / 5.0, 1.0 / 5.0),
    ];
    match cmpd {
        "dbl" => Some(DBL),
        "thinThick" => Some(THIN_THICK),
        "thickThin" => Some(THICK_THIN),
        "tri" => Some(TRI),
        _ => None,
    }
}

/// Parallel rails of a compound straight run: `(start, end, width_emu)`.
/// The centre line is not painted (`destination-out` in the reference).
/// ref renderer.ts:7513-7530
pub fn compound_rails(
    start: Point,
    end: Point,
    width_emu: f32,
    cmpd: &str,
) -> Vec<Rail> {
    let Some(subs) = compound_subs(cmpd) else {
        return Vec::new();
    };
    let total_w = width_emu.max(0.5);
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return Vec::new();
    }
    let (px, py) = (-dy / len, dx / len);
    subs.iter()
        .map(|(offset, width_frac)| {
            let ox = px * (total_w * offset);
            let oy = py * (total_w * offset);
            (
                (start.0 + ox, start.1 + oy),
                (end.0 + ox, end.1 + oy),
                (total_w * width_frac).max(0.5),
            )
        })
        .collect()
}
