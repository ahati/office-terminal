//! Engine geometry → contract path builders: figure-aware engine paths,
//! open/closed polygons, rounded rectangles, ellipses and sampled arcs.

use metafile_core as mc;

use crate::contracts::{AffineTransform, FillRule, PathSegment, RectF32, VectorPath};

/// Engine path → contract path (figure-aware).
pub fn engine_path(path: &mc::Path, rule: FillRule) -> VectorPath {
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
                mc::PathSegment::Cubic { control1, control2, to } => segments.push(PathSegment::CubicTo {
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

pub fn open_path(points: &[mc::Point]) -> VectorPath {
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

pub fn closed_polygon(points: &[mc::Point]) -> VectorPath {
    let mut path = open_path(points);
    path.segments.push(PathSegment::Close);
    path
}

pub fn rounded_rect(rect: RectF32, radius: Option<(f32, f32)>) -> VectorPath {
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
pub fn ellipse_path(rect: RectF32) -> VectorPath {
    let (w, h) = (rect.w, rect.h);
    let (rx, ry) = (w * 0.5, h * 0.5);
    let (cx, cy) = (rx, ry);
    let kx = rx * 0.552_284_8;
    let ky = ry * 0.552_284_8;
    translate_path(
        VectorPath::new(
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
        ),
        rect.x,
        rect.y,
    )
}

/// Polyline-sampled arc; pie/chord are closed to the centre/chord.
pub fn arc_path(rect: RectF32, start: mc::Point, end: mc::Point, kind: mc::ArcKind, clockwise: bool) -> VectorPath {
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
    let first = mc::Point::new(cx + rx * start_angle.cos(), cy + ry * start_angle.sin());
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
pub fn translate_path(path: VectorPath, x: f32, y: f32) -> VectorPath {
    if x == 0.0 && y == 0.0 {
        return path;
    }
    let transform = AffineTransform::translate(x, y);
    crate::metafile::mapping::transform_path(&path, &transform)
}
