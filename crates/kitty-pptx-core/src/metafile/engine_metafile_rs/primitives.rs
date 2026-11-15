//! Geometry and clip conversion helpers for the renderer adapter.

use metafile_core as mc;

use crate::contracts::{AffineTransform, FillRule, PathSegment, RectF32, VectorPath};

use super::{fill_rule, from_engine_rect};

/// Engine path → contract path (figure-aware).
pub(super) fn engine_path(path: &mc::Path, rule: FillRule) -> VectorPath {
    let mut segments = Vec::new();
    for figure in &path.figures {
        segments.push(PathSegment::MoveTo {
            x: figure.start.x as f32,
            y: figure.start.y as f32,
        });
        for segment in &figure.segments {
            match segment {
                mc::PathSegment::Line(point) => segments.push(PathSegment::LineTo {
                    x: point.x as f32,
                    y: point.y as f32,
                }),
                mc::PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => segments.push(PathSegment::CubicTo {
                    x1: control1.x as f32,
                    y1: control1.y as f32,
                    x2: control2.x as f32,
                    y2: control2.y as f32,
                    x: to.x as f32,
                    y: to.y as f32,
                }),
            }
        }
        if figure.closed {
            segments.push(PathSegment::Close);
        }
    }
    VectorPath::new(segments, rule)
}

pub(super) fn open_path(points: &[mc::Point]) -> VectorPath {
    let mut segments = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let segment = if index == 0 {
            PathSegment::MoveTo {
                x: point.x as f32,
                y: point.y as f32,
            }
        } else {
            PathSegment::LineTo {
                x: point.x as f32,
                y: point.y as f32,
            }
        };
        segments.push(segment);
    }
    VectorPath::new(segments, FillRule::NonZero)
}

pub(super) fn closed_polygon(points: &[mc::Point]) -> VectorPath {
    let mut path = open_path(points);
    path.segments.push(PathSegment::Close);
    path
}

pub(super) fn rounded_rect(rect: RectF32, radius: Option<(f32, f32)>) -> VectorPath {
    let (w, h) = (rect.w, rect.h);
    let path = |path: VectorPath| translate_path(path, rect.x, rect.y);
    let Some((rx, ry)) = radius.filter(|(rx, ry)| *rx > 0.0 && *ry > 0.0) else {
        return path(VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: w, y: 0.0 },
                PathSegment::LineTo { x: w, y: h },
                PathSegment::LineTo { x: 0.0, y: h },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ));
    };
    let rx = rx.min(w * 0.5);
    let ry = ry.min(h * 0.5);
    path(VectorPath::new(
        vec![
            PathSegment::MoveTo { x: rx, y: 0.0 },
            PathSegment::LineTo { x: w - rx, y: 0.0 },
            PathSegment::QuadTo {
                x1: w,
                y1: 0.0,
                x: w,
                y: ry,
            },
            PathSegment::LineTo { x: w, y: h - ry },
            PathSegment::QuadTo {
                x1: w,
                y1: h,
                x: w - rx,
                y: h,
            },
            PathSegment::LineTo { x: rx, y: h },
            PathSegment::QuadTo {
                x1: 0.0,
                y1: h,
                x: 0.0,
                y: h - ry,
            },
            PathSegment::LineTo { x: 0.0, y: ry },
            PathSegment::QuadTo {
                x1: 0.0,
                y1: 0.0,
                x: rx,
                y: 0.0,
            },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    ))
}

/// Four cubic arcs (kappa = 0.5522847498).
pub(super) fn ellipse_path(rect: RectF32) -> VectorPath {
    let (w, h) = (rect.w, rect.h);
    let (rx, ry) = (w * 0.5, h * 0.5);
    let (cx, cy) = (rx, ry);
    let kx = rx * 0.552_284_8;
    let ky = ry * 0.552_284_8;
    translate_path(VectorPath::new(
        vec![
            PathSegment::MoveTo { x: cx, y: 0.0 },
            PathSegment::CubicTo {
                x1: cx + kx,
                y1: 0.0,
                x2: w,
                y2: cy - ky,
                x: w,
                y: cy,
            },
            PathSegment::CubicTo {
                x1: w,
                y1: cy + ky,
                x2: cx + kx,
                y2: h,
                x: cx,
                y: h,
            },
            PathSegment::CubicTo {
                x1: cx - kx,
                y1: h,
                x2: 0.0,
                y2: cy + ky,
                x: 0.0,
                y: cy,
            },
            PathSegment::CubicTo {
                x1: 0.0,
                y1: cy - ky,
                x2: cx - kx,
                y2: 0.0,
                x: cx,
                y: 0.0,
            },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    ), rect.x, rect.y)
}

/// Polyline-sampled arc; pie/chord are closed to the centre/chord.
pub(super) fn arc_path(
    rect: RectF32,
    start: mc::Point,
    end: mc::Point,
    kind: mc::ArcKind,
    clockwise: bool,
) -> VectorPath {
    let cx = f64::from(rect.w) * 0.5;
    let cy = f64::from(rect.h) * 0.5;
    let rx = cx.max(1.0e-6);
    let ry = cy.max(1.0e-6);
    let angle_of = |point: mc::Point| ((point.y - cy) / ry).atan2((point.x - cx) / rx);
    let start_angle = angle_of(start);
    let end_angle = angle_of(end);
    let two_pi = std::f64::consts::TAU;
    let mut sweep = end_angle - start_angle;
    if clockwise {
        if sweep >= 0.0 {
            sweep -= two_pi;
        }
    } else if sweep <= 0.0 {
        sweep += two_pi;
    }

    let segments_count = 64usize;
    let mut segments = Vec::with_capacity(segments_count + 4);
    let first = mc::Point::new(
        cx + rx * start_angle.cos(),
        cy + ry * start_angle.sin(),
    );
    let center = mc::Point::new(cx, cy);
    if matches!(kind, mc::ArcKind::Pie) {
        segments.push(PathSegment::MoveTo {
            x: cx as f32,
            y: cy as f32,
        });
        segments.push(PathSegment::LineTo {
            x: first.x as f32,
            y: first.y as f32,
        });
    } else {
        segments.push(PathSegment::MoveTo {
            x: first.x as f32,
            y: first.y as f32,
        });
    }
    for step in 1..=segments_count {
        let t = step as f64 / segments_count as f64;
        let angle = start_angle + sweep * t;
        segments.push(PathSegment::LineTo {
            x: (cx + rx * angle.cos()) as f32,
            y: (cy + ry * angle.sin()) as f32,
        });
    }
    match kind {
        mc::ArcKind::Arc => {}
        mc::ArcKind::Pie => {
            segments.push(PathSegment::LineTo {
                x: center.x as f32,
                y: center.y as f32,
            });
            segments.push(PathSegment::Close);
        }
        mc::ArcKind::Chord => segments.push(PathSegment::Close),
    }
    translate_path(VectorPath::new(segments, FillRule::NonZero), rect.x, rect.y)
}

/// Translates a path into place (primitive helpers are authored at the origin).
pub(super) fn translate_path(path: VectorPath, x: f32, y: f32) -> VectorPath {
    if x == 0.0 && y == 0.0 {
        return path;
    }
    let transform = AffineTransform::translate(x, y);
    crate::metafile::mapping::transform_path(&path, &transform)
}

/// Converts a clip region into nested clip paths.
///
/// `Rect`/`Polygon`/`Path`/`Intersection` are exact; `Combine` operations are
/// approximated by their bounding box and flagged.
pub(super) fn map_clip(region: &mc::ClipRegion) -> (Vec<VectorPath>, Option<&'static str>) {
    match region {
        mc::ClipRegion::Rect(rect) => {
            let rect = from_engine_rect(*rect);
            (
                vec![rounded_rect(rect, None)],
                None,
            )
        }
        mc::ClipRegion::Polygon(points) => {
            if points.len() < 3 {
                (Vec::new(), None)
            } else {
                (vec![closed_polygon(points)], None)
            }
        }
        mc::ClipRegion::Path { path, fill_mode } => {
            let path = engine_path(path, fill_rule(*fill_mode));
            if path.segments.is_empty() {
                (Vec::new(), None)
            } else {
                (vec![path], None)
            }
        }
        mc::ClipRegion::Intersection(regions) => {
            let mut paths = Vec::new();
            let mut approximation = None;
            for region in regions {
                let (mut nested, tag) = map_clip(region);
                if tag.is_some() {
                    approximation = tag;
                }
                paths.append(&mut nested);
            }
            (paths, approximation)
        }
        mc::ClipRegion::Combine {
            operation,
            left,
            right,
        } => {
            let bounds = match operation {
                mc::ClipOperation::Exclude => region_bounds(left),
                _ => union_bounds(region_bounds(left), region_bounds(right)),
            };
            let paths = bounds
                .map(|bounds| vec![rounded_rect(bounds, None)])
                .unwrap_or_default();
            (paths, Some("metafile_clip_combine"))
        }
    }
}

pub(super) fn region_bounds(region: &mc::ClipRegion) -> Option<RectF32> {
    match region {
        mc::ClipRegion::Rect(rect) => Some(from_engine_rect(*rect)),
        mc::ClipRegion::Polygon(points) => points_bounds(points),
        mc::ClipRegion::Path { path, .. } => path_bounds(path),
        mc::ClipRegion::Intersection(regions) => {
            let mut bounds: Option<RectF32> = None;
            for region in regions {
                bounds = match (bounds, region_bounds(region)) {
                    (Some(a), Some(b)) => Some(intersect_bounds(a, b)),
                    (None, other) => other,
                    (some, None) => some,
                };
            }
            bounds
        }
        mc::ClipRegion::Combine {
            operation,
            left,
            right,
        } => match operation {
            mc::ClipOperation::Exclude => region_bounds(left),
            _ => union_bounds(region_bounds(left), region_bounds(right)),
        },
    }
}

pub(super) fn points_bounds(points: &[mc::Point]) -> Option<RectF32> {
    let mut iter = points.iter();
    let first = iter.next()?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for point in iter {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some(RectF32 {
        x: min_x as f32,
        y: min_y as f32,
        w: (max_x - min_x) as f32,
        h: (max_y - min_y) as f32,
    })
}

pub(super) fn path_bounds(path: &mc::Path) -> Option<RectF32> {
    let mut points = Vec::new();
    for figure in &path.figures {
        points.push(figure.start);
        for segment in &figure.segments {
            match segment {
                mc::PathSegment::Line(point) => points.push(*point),
                mc::PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => {
                    points.push(*control1);
                    points.push(*control2);
                    points.push(*to);
                }
            }
        }
    }
    points_bounds(&points)
}

pub(super) fn union_bounds(a: Option<RectF32>, b: Option<RectF32>) -> Option<RectF32> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let min_x = a.x.min(b.x);
            let min_y = a.y.min(b.y);
            let max_x = (a.x + a.w).max(b.x + b.w);
            let max_y = (a.y + a.h).max(b.y + b.h);
            Some(RectF32 {
                x: min_x,
                y: min_y,
                w: max_x - min_x,
                h: max_y - min_y,
            })
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub(super) fn intersect_bounds(a: RectF32, b: RectF32) -> RectF32 {
    let min_x = a.x.max(b.x);
    let min_y = a.y.max(b.y);
    let max_x = (a.x + a.w).min(b.x + b.w);
    let max_y = (a.y + a.h).min(b.y + b.h);
    RectF32 {
        x: min_x,
        y: min_y,
        w: (max_x - min_x).max(0.0),
        h: (max_y - min_y).max(0.0),
    }
}
