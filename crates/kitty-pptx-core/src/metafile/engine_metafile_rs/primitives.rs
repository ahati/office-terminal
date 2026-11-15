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
/// `Rect`/`Polygon`/`Path`/`Intersection` are exact. `Combine` is exact for
/// `Union`/`Xor` of simple closed rect/polygon operands (compound path with
/// `NonZero`/`EvenOdd`) and for `Exclude` of two rectangles; curved, nested or
/// `Complement` combinations keep the conservative bounding box + warning.
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
            if let Some(paths) = exact_combine(*operation, left, right) {
                return (paths, None);
            }
            // Conservative bounds only: `Exclude`/`Complement` can never
            // paint outside the left operand, and `Union`/`Xor` are bounded by
            // the union (PRD 13 §4.7).
            let bounds = match operation {
                mc::ClipOperation::Exclude | mc::ClipOperation::Complement => region_bounds(left),
                _ => union_bounds(region_bounds(left), region_bounds(right)),
            };
            let paths = bounds
                .map(|bounds| vec![rounded_rect(bounds, None)])
                .unwrap_or_default();
            (paths, Some("metafile_clip_combine"))
        }
    }
}

/// Simple closed polygons of a region, when it can be flattened without losing
/// geometry (rect, polygon, or a straight-line non-zero path).
fn region_rings(region: &mc::ClipRegion) -> Option<Vec<Vec<(f32, f32)>>> {
    match region {
        mc::ClipRegion::Rect(rect) => Some(vec![rect_ring(from_engine_rect(*rect))]),
        mc::ClipRegion::Polygon(points) => ring_from_points(points).map(|ring| vec![ring]),
        mc::ClipRegion::Path { path, fill_mode } if *fill_mode == 2 => {
            rings_from_path(&engine_path(path, FillRule::NonZero))
        }
        _ => None,
    }
}

fn rect_ring(rect: RectF32) -> Vec<(f32, f32)> {
    vec![
        (rect.x, rect.y),
        (rect.x + rect.w, rect.y),
        (rect.x + rect.w, rect.y + rect.h),
        (rect.x, rect.y + rect.h),
    ]
}

fn ring_from_points(points: &[mc::Point]) -> Option<Vec<(f32, f32)>> {
    if points.len() < 3 {
        return None;
    }
    let mut ring: Vec<(f32, f32)> = points.iter().map(|p| (p.x as f32, p.y as f32)).collect();
    if ring.first() == ring.last() {
        ring.pop();
    }
    (ring.len() >= 3).then_some(ring)
}

fn rings_from_path(path: &VectorPath) -> Option<Vec<Vec<(f32, f32)>>> {
    let mut rings = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo { x, y } => {
                if current.len() >= 3 {
                    push_ring(&mut rings, &current);
                }
                current.clear();
                current.push((x, y));
            }
            PathSegment::LineTo { x, y } => {
                if current.is_empty() {
                    return None;
                }
                current.push((x, y));
            }
            PathSegment::Close => {
                if current.len() < 3 {
                    return None;
                }
                push_ring(&mut rings, &current);
                current.clear();
            }
            PathSegment::QuadTo { .. } | PathSegment::CubicTo { .. } => return None,
        }
    }
    if current.len() >= 3 {
        push_ring(&mut rings, &current);
    }
    (!rings.is_empty()).then_some(rings)
}

fn push_ring(rings: &mut Vec<Vec<(f32, f32)>>, ring: &[(f32, f32)]) {
    let mut ring = ring.to_vec();
    if ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() >= 3 {
        rings.push(ring);
    }
}

fn signed_area(ring: &[(f32, f32)]) -> f32 {
    let mut area = 0.0f32;
    for index in 0..ring.len() {
        let (x0, y0) = ring[index];
        let (x1, y1) = ring[(index + 1) % ring.len()];
        area += x0 * y1 - x1 * y0;
    }
    area * 0.5
}

fn orient(ring: &mut [(f32, f32)], positive: bool) {
    if (signed_area(ring) > 0.0) != positive {
        ring.reverse();
    }
}

fn rings_to_path(rings: &[Vec<(f32, f32)>], rule: FillRule) -> Option<VectorPath> {
    let mut segments = Vec::new();
    for ring in rings {
        if ring.len() < 3 {
            continue;
        }
        segments.push(PathSegment::MoveTo {
            x: ring[0].0,
            y: ring[0].1,
        });
        for (x, y) in &ring[1..] {
            segments.push(PathSegment::LineTo { x: *x, y: *y });
        }
        segments.push(PathSegment::Close);
    }
    (!segments.is_empty()).then(|| VectorPath::new(segments, rule))
}

/// Exact compound-path representation for the combine modes a single path can
/// express. `Complement` is unbounded, so it keeps the bbox fallback.
fn exact_combine(
    operation: mc::ClipOperation,
    left: &mc::ClipRegion,
    right: &mc::ClipRegion,
) -> Option<Vec<VectorPath>> {
    match operation {
        mc::ClipOperation::Union | mc::ClipOperation::Xor => {
            let mut rings = region_rings(left)?;
            rings.extend(region_rings(right)?);
            if matches!(operation, mc::ClipOperation::Union) {
                // Same winding so overlaps cannot cancel under NonZero.
                for ring in &mut rings {
                    orient(ring, true);
                }
            }
            let rule = if matches!(operation, mc::ClipOperation::Union) {
                FillRule::NonZero
            } else {
                FillRule::EvenOdd
            };
            Some(vec![rings_to_path(&rings, rule)?])
        }
        mc::ClipOperation::Exclude => {
            // `A - B` = intersect A with the compound path `A + reverse(B)`.
            // The first clip keeps a B part that sticks outside A from
            // painting; the reversed B subtracts the overlap.
            let mut left_rings = region_rings(left)?;
            let mut right_rings = region_rings(right)?;
            for ring in &mut left_rings {
                orient(ring, true);
            }
            for ring in &mut right_rings {
                orient(ring, false);
            }
            let outer = rings_to_path(&left_rings, FillRule::NonZero)?;
            let mut difference_rings = left_rings;
            difference_rings.extend(right_rings);
            let difference = rings_to_path(&difference_rings, FillRule::NonZero)?;
            Some(vec![outer, difference])
        }
        mc::ClipOperation::Complement => None,
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
            mc::ClipOperation::Exclude | mc::ClipOperation::Complement => region_bounds(left),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f64, top: f64, right: f64, bottom: f64) -> mc::ClipRegion {
        mc::ClipRegion::Rect(mc::Rect {
            left,
            top,
            right,
            bottom,
        })
    }

    fn combine(operation: mc::ClipOperation, left: mc::ClipRegion, right: mc::ClipRegion) -> mc::ClipRegion {
        mc::ClipRegion::Combine {
            operation,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn figures(path: &VectorPath) -> usize {
        path.segments
            .iter()
            .filter(|segment| matches!(segment, PathSegment::MoveTo { .. }))
            .count()
    }

    #[test]
    fn union_and_xor_are_exact_compound_paths() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let b = rect(50.0, 50.0, 150.0, 150.0);

        let (paths, tag) = map_clip(&combine(mc::ClipOperation::Union, a.clone(), b.clone()));
        assert_eq!(tag, None);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].fill_rule, FillRule::NonZero);
        assert_eq!(figures(&paths[0]), 2);

        let (paths, tag) = map_clip(&combine(mc::ClipOperation::Xor, a, b));
        assert_eq!(tag, None);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].fill_rule, FillRule::EvenOdd);
        assert_eq!(figures(&paths[0]), 2);
    }

    #[test]
    fn rect_difference_is_exact() {
        let outer = rect(0.0, 0.0, 100.0, 100.0);
        let inner = rect(25.0, 25.0, 75.0, 75.0);
        let (paths, tag) = map_clip(&combine(
            mc::ClipOperation::Exclude,
            outer.clone(),
            inner,
        ));
        assert_eq!(tag, None);
        assert_eq!(paths.len(), 2, "outer clip + difference compound");
        assert_eq!(figures(&paths[0]), 1);
        assert_eq!(figures(&paths[1]), 2);
        assert_eq!(paths[1].fill_rule, FillRule::NonZero);

        let outside = rect(200.0, 200.0, 300.0, 300.0);
        let (paths, tag) = map_clip(&combine(mc::ClipOperation::Exclude, outer, outside));
        assert_eq!(tag, None);
        assert_eq!(paths.len(), 2);
        assert_eq!(figures(&paths[0]), 1);
        assert_eq!(figures(&paths[1]), 2);
    }

    #[test]
    fn polygon_difference_is_exact() {
        let a = mc::ClipRegion::Polygon(vec![
            mc::Point::new(0.0, 0.0),
            mc::Point::new(100.0, 0.0),
            mc::Point::new(100.0, 100.0),
            mc::Point::new(0.0, 100.0),
        ]);
        let b = mc::ClipRegion::Polygon(vec![
            mc::Point::new(25.0, 25.0),
            mc::Point::new(75.0, 25.0),
            mc::Point::new(75.0, 75.0),
            mc::Point::new(25.0, 75.0),
        ]);
        let (paths, tag) = map_clip(&combine(mc::ClipOperation::Exclude, a, b));
        assert_eq!(tag, None, "polygon subtraction is representable");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn curved_or_complement_combines_keep_the_bbox_fallback() {
        let curved = mc::ClipRegion::Path {
            path: mc::Path {
                figures: vec![mc::PathFigure {
                    start: mc::Point::new(0.0, 0.0),
                    segments: vec![mc::PathSegment::Cubic {
                        control1: mc::Point::new(10.0, 0.0),
                        control2: mc::Point::new(10.0, 10.0),
                        to: mc::Point::new(0.0, 10.0),
                    }],
                    closed: true,
                }],
            },
            fill_mode: 2,
        };
        let (paths, tag) = map_clip(&combine(
            mc::ClipOperation::Union,
            rect(0.0, 0.0, 100.0, 100.0),
            curved,
        ));
        assert_eq!(tag, Some("metafile_clip_combine"));
        assert_eq!(paths.len(), 1);

        let (_, tag) = map_clip(&combine(
            mc::ClipOperation::Complement,
            rect(0.0, 0.0, 100.0, 100.0),
            rect(25.0, 25.0, 75.0, 75.0),
        ));
        assert_eq!(tag, Some("metafile_clip_combine"));
    }

    struct NullStore;

    impl crate::loader::AssetStore for NullStore {
        fn get(
            &self,
            rel_id: &str,
            _hint: crate::loader::SizeHint,
        ) -> Result<crate::loader::DecodedImage, crate::AssetError> {
            Err(crate::AssetError::NotFound(rel_id.to_string()))
        }

        fn stats(&self) -> crate::loader::AssetStoreStats {
            crate::loader::AssetStoreStats {
                cached_entries: 0,
                cached_bytes: 0,
            }
        }
    }

    fn render_clip(region: &mc::ClipRegion) -> Vec<u8> {
        use crate::contracts::VectorCommand;
        use crate::SlideRasterizer;

        let (paths, _) = map_clip(region);
        let mut scene = crate::contracts::VectorSlide::new(100, 100);
        let mut pushed = 0;
        for path in paths {
            scene.commands.push(VectorCommand::PushClip(path));
            pushed += 1;
        }
        scene.commands.push(VectorCommand::DrawPath {
            path: VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: 100.0, y: 0.0 },
                    PathSegment::LineTo { x: 100.0, y: 100.0 },
                    PathSegment::LineTo { x: 0.0, y: 100.0 },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            ),
            fill: Some(crate::contracts::VectorPaint::Solid([0, 0, 0, 255])),
            stroke: None,
            transform: crate::contracts::AffineTransform::identity(),
            effects: Box::default(),
        });
        for _ in 0..pushed {
            scene.commands.push(VectorCommand::PopClip);
        }
        let target = crate::contracts::RenderTargetSpec {
            width_px: 100,
            height_px: 100,
        };
        let bitmap = crate::TinySkiaRasterizer::new()
            .rasterize(&scene, &target, &NullStore)
            .expect("rasterize");
        bitmap.pixels.to_vec()
    }

    fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
        let index = (y * 100 + x) * 4;
        &pixels[index..index + 4]
    }

    #[test]
    fn combined_clips_mask_pixels_exactly() {
        let black = [0u8, 0, 0, 255];
        let white = [255u8, 255, 255, 255];

        let union = combine(
            mc::ClipOperation::Union,
            rect(0.0, 0.0, 60.0, 60.0),
            rect(40.0, 40.0, 100.0, 100.0),
        );
        let pixels = render_clip(&union);
        assert_eq!(pixel(&pixels, 50, 50), black, "union keeps the overlap");
        assert_eq!(pixel(&pixels, 90, 90), black);

        let xor = combine(
            mc::ClipOperation::Xor,
            rect(0.0, 0.0, 60.0, 60.0),
            rect(40.0, 40.0, 100.0, 100.0),
        );
        let pixels = render_clip(&xor);
        assert_eq!(pixel(&pixels, 50, 50), white, "xor cuts the overlap");
        assert_eq!(pixel(&pixels, 20, 20), black);
        assert_eq!(pixel(&pixels, 90, 90), black);

        let exclude = combine(
            mc::ClipOperation::Exclude,
            rect(0.0, 0.0, 100.0, 100.0),
            rect(40.0, 40.0, 60.0, 60.0),
        );
        let pixels = render_clip(&exclude);
        assert_eq!(pixel(&pixels, 50, 50), white, "difference cuts the hole");
        assert_eq!(pixel(&pixels, 10, 10), black);
        assert_eq!(pixel(&pixels, 90, 90), black);
    }
}
