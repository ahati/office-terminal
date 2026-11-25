//! Bounds measurement: clip regions, point lists and engine paths.

use metafile_core as mc;

use crate::contracts::RectF32;

use super::super::from_engine_rect;

pub fn region_bounds(region: &mc::ClipRegion) -> Option<RectF32> {
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
        mc::ClipRegion::Combine { operation, left, right } => match operation {
            mc::ClipOperation::Exclude | mc::ClipOperation::Complement => region_bounds(left),
            _ => union_bounds(region_bounds(left), region_bounds(right)),
        },
    }
}

pub fn points_bounds(points: &[mc::Point]) -> Option<RectF32> {
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

pub fn path_bounds(path: &mc::Path) -> Option<RectF32> {
    let mut points = Vec::new();
    for figure in &path.figures {
        points.push(figure.start);
        for segment in &figure.segments {
            match segment {
                mc::PathSegment::Line(point) => points.push(*point),
                mc::PathSegment::Cubic { control1, control2, to } => {
                    points.push(*control1);
                    points.push(*control2);
                    points.push(*to);
                }
            }
        }
    }
    points_bounds(&points)
}

pub fn union_bounds(a: Option<RectF32>, b: Option<RectF32>) -> Option<RectF32> {
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

pub fn intersect_bounds(a: RectF32, b: RectF32) -> RectF32 {
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
