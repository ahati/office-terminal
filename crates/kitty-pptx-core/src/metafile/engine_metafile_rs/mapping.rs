//! Engine pens/brushes/paints → display-list values.

use metafile_core as mc;

use super::rgba;
use crate::metafile::{MetafilePaint, MetafilePen};

pub(super) fn map_pen(pen: &mc::Pen) -> (Option<MetafilePen>, Option<&'static str>) {
    if matches!(pen.style, mc::PenStyle::Null) {
        return (None, None);
    }
    let (dash, tag) = match pen.style {
        mc::PenStyle::Solid => (Vec::new(), None),
        mc::PenStyle::Dash => (vec![4.0, 3.0], None),
        mc::PenStyle::Dot => (vec![1.0, 2.0], None),
        mc::PenStyle::DashDot => (vec![4.0, 3.0, 1.0, 3.0], None),
        mc::PenStyle::DashDotDot => (vec![4.0, 3.0, 1.0, 3.0, 1.0, 3.0], None),
        mc::PenStyle::InsideFrame => (Vec::new(), Some("metafile_pen_style")),
        mc::PenStyle::Null => (Vec::new(), None),
        mc::PenStyle::Unknown(_) => (Vec::new(), Some("metafile_pen_style")),
    };
    (
        Some(MetafilePen {
            color: rgba(pen.color),
            width: pen.width as f32,
            cosmetic: pen.cosmetic,
            dash,
            line_cap: map_cap(pen.line_cap),
            line_join: map_join(pen.line_join),
            miter_limit: 10.0,
        }),
        tag,
    )
}

pub(super) fn map_stroke(stroke: &mc::Stroke) -> (MetafilePen, Option<&'static str>) {
    let (color, tag) = match &stroke.paint {
        mc::Paint::Solid(paint_color) => (rgba(*paint_color), None),
        mc::Paint::LinearGradient { stops, .. } => (
            stops
                .first()
                .map(|stop| rgba(stop.color))
                .unwrap_or([0, 0, 0, 255]),
            Some("metafile_pen_style"),
        ),
        mc::Paint::Hatch { foreground, .. } => (rgba(*foreground), Some("metafile_hatch")),
        mc::Paint::Texture { .. } => ([128, 128, 128, 255], Some("metafile_pattern_brush")),
    };
    (
        MetafilePen {
            color,
            width: stroke.width as f32,
            cosmetic: false,
            dash: stroke.dash_pattern.iter().map(|v| *v as f32).collect(),
            line_cap: map_cap(stroke.line_cap),
            line_join: map_join(stroke.line_join),
            miter_limit: stroke.miter_limit as f32,
        },
        tag,
    )
}

pub(super) fn map_paint(paint: &mc::Paint) -> (Option<MetafilePaint>, Option<&'static str>) {
    match paint {
        mc::Paint::Solid(paint_color) => (Some(MetafilePaint::Solid(rgba(*paint_color))), None),
        mc::Paint::LinearGradient {
            start,
            end,
            stops,
            transform,
            ..
        } => {
            let (from, to) = if is_identity(*transform) {
                ((start.x as f32, start.y as f32), (end.x as f32, end.y as f32))
            } else {
                (
                    (transform_point(*transform, *start).0, transform_point(*transform, *start).1),
                    (transform_point(*transform, *end).0, transform_point(*transform, *end).1),
                )
            };
            let stops = stops
                .iter()
                .map(|stop| (stop.offset as f32, rgba(stop.color)))
                .collect();
            (
                Some(MetafilePaint::LinearGradient { from, to, stops }),
                (!is_identity(*transform)).then_some("metafile_texture_transform"),
            )
        }
        mc::Paint::Hatch { foreground, .. } => {
            (Some(MetafilePaint::Solid(rgba(*foreground))), Some("metafile_hatch"))
        }
        mc::Paint::Texture { .. } => (None, Some("metafile_pattern_brush")),
    }
}

pub(super) fn map_brush(brush: &mc::Brush) -> (Option<MetafilePaint>, Option<&'static str>) {
    match brush.style {
        mc::BrushStyle::Solid => (Some(MetafilePaint::Solid(rgba(brush.color))), None),
        mc::BrushStyle::Null => (None, None),
        mc::BrushStyle::Hatched(_) => (
            Some(MetafilePaint::Solid(rgba(brush.color))),
            Some("metafile_hatch"),
        ),
        mc::BrushStyle::Pattern | mc::BrushStyle::Unknown(_) => (
            Some(MetafilePaint::Solid(rgba(brush.color))),
            Some("metafile_pattern_brush"),
        ),
    }
}

pub(super) fn map_cap(cap: mc::LineCap) -> crate::contracts::LineCap {
    match cap {
        mc::LineCap::Butt => crate::contracts::LineCap::Butt,
        mc::LineCap::Round => crate::contracts::LineCap::Round,
        mc::LineCap::Square => crate::contracts::LineCap::Square,
    }
}

pub(super) fn map_join(join: mc::LineJoin) -> crate::contracts::LineJoin {
    match join {
        mc::LineJoin::Miter => crate::contracts::LineJoin::Miter,
        mc::LineJoin::Round => crate::contracts::LineJoin::Round,
        mc::LineJoin::Bevel => crate::contracts::LineJoin::Bevel,
    }
}

pub(super) fn is_identity(transform: mc::Transform) -> bool {
    (transform.m11 - 1.0).abs() < 1e-9
        && transform.m12.abs() < 1e-9
        && transform.m21.abs() < 1e-9
        && (transform.m22 - 1.0).abs() < 1e-9
        && transform.dx.abs() < 1e-9
        && transform.dy.abs() < 1e-9
}

pub(super) fn transform_point(transform: mc::Transform, point: mc::Point) -> (f32, f32) {
    (
        (transform.m11 * point.x + transform.m21 * point.y + transform.dx) as f32,
        (transform.m12 * point.x + transform.m22 * point.y + transform.dy) as f32,
    )
}
