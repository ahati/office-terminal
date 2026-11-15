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
        mc::Paint::Texture { bitmap, .. } => {
            (bitmap_average(bitmap), Some("metafile_pattern_brush"))
        }
    };
    // The engine converts dash patterns from pen-width multiples into output
    // units; `VectorStroke::dash_segments` is expressed in width multiples. A
    // zero-width (cosmetic) stroke normalises against one output unit.
    let width = if stroke.width > 0.0 { stroke.width } else { 1.0 };
    (
        MetafilePen {
            color,
            width: stroke.width as f32,
            cosmetic: false,
            dash: stroke
                .dash_pattern
                .iter()
                .map(|value| (*value / width) as f32)
                .collect(),
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
            let from = transform_point(*transform, *start);
            // GDI+ encodes the gradient angle in the brush transform, while
            // `start`/`end` are the gradient rectangle corners; the gradient
            // axis is the transformed x-axis, not the rectangle diagonal.
            let to = if transform.m11.abs() < 1.0e-9 && transform.m12.abs() < 1.0e-9 {
                transform_point(*transform, *end)
            } else {
                (from.0 + transform.m11 as f32, from.1 + transform.m12 as f32)
            };
            let stops = stops
                .iter()
                .map(|stop| (stop.offset as f32, rgba(stop.color)))
                .collect();
            (
                Some(MetafilePaint::LinearGradient { from, to, stops }),
                None,
            )
        }
        mc::Paint::Hatch {
            style,
            foreground,
            background,
        } => (
            Some(MetafilePaint::Pattern {
                preset: hatch_preset(*style),
                fg: rgba(*foreground),
                bg: rgba(*background),
            }),
            Some("metafile_hatch"),
        ),
        mc::Paint::Texture {
            bitmap, opacity, ..
        } => {
            let mut color = bitmap_average(bitmap);
            color[3] = (f64::from(color[3]) * opacity.clamp(0.0, 1.0)).round() as u8;
            (
                Some(MetafilePaint::Solid(color)),
                Some("metafile_pattern_brush"),
            )
        }
    }
}

pub(super) fn map_brush(brush: &mc::Brush) -> (Option<MetafilePaint>, Option<&'static str>) {
    match brush.style {
        mc::BrushStyle::Solid => (Some(MetafilePaint::Solid(rgba(brush.color))), None),
        mc::BrushStyle::Null => (None, None),
        mc::BrushStyle::Hatched(style) => (
            Some(MetafilePaint::Pattern {
                preset: hatch_preset(u32::from(style)),
                fg: rgba(brush.color),
                bg: [255, 255, 255, 255],
            }),
            Some("metafile_hatch"),
        ),
        mc::BrushStyle::Pattern | mc::BrushStyle::Unknown(_) => (
            Some(MetafilePaint::Solid(rgba(brush.color))),
            Some("metafile_pattern_brush"),
        ),
    }
}

/// GDI hatch index (0–5, or GDI+ `HatchStyle`) → DrawingML preset pattern.
fn hatch_preset(style: u32) -> &'static str {
    match style {
        0 => "horz",
        1 => "vert",
        2 => "dnDiag",
        3 => "upDiag",
        4 => "cross",
        _ => "diagCross",
    }
}

/// Deterministic average colour of a decoded bitmap.
fn bitmap_average(bitmap: &mc::Bitmap) -> [u8; 4] {
    let mut sums = [0u64; 3];
    let mut alpha_sum = 0u64;
    let mut count = 0u64;
    for pixel in bitmap.rgba.as_chunks::<4>().0 {
        let alpha = u64::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        for channel in 0..3 {
            sums[channel] += u64::from(pixel[channel]) * alpha;
        }
        alpha_sum += alpha;
        count += 1;
    }
    if alpha_sum == 0 || count == 0 {
        return [0, 0, 0, 255];
    }
    [
        (sums[0] / alpha_sum) as u8,
        (sums[1] / alpha_sum) as u8,
        (sums[2] / alpha_sum) as u8,
        (alpha_sum / count) as u8,
    ]
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

pub(super) fn transform_point(transform: mc::Transform, point: mc::Point) -> (f32, f32) {
    (
        (transform.m11 * point.x + transform.m21 * point.y + transform.dx) as f32,
        (transform.m12 * point.x + transform.m22 * point.y + transform.dy) as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_bitmap() -> mc::Bitmap {
        mc::Bitmap {
            width: 2,
            height: 2,
            rgba: [255, 0, 0, 255].repeat(4),
        }
    }

    #[test]
    fn texture_brushes_approximate_to_an_average_colour() {
        let (paint, tag) = map_paint(&mc::Paint::Texture {
            bitmap: solid_bitmap(),
            transform: mc::Transform::IDENTITY,
            wrap_mode: 0,
            opacity: 0.5,
            do_not_transform: false,
        });
        assert_eq!(tag, Some("metafile_pattern_brush"));
        assert_eq!(
            paint,
            Some(MetafilePaint::Solid([255, 0, 0, 128])),
            "texture fills must paint their average colour instead of vanishing"
        );
    }

    #[test]
    fn gradient_axis_follows_the_brush_transform() {
        let stops = vec![
            mc::GradientStop {
                offset: 0.0,
                color: mc::Color::BLACK,
            },
            mc::GradientStop {
                offset: 1.0,
                color: mc::Color::WHITE,
            },
        ];
        // A quarter-turn transform: the gradient advances along +y.
        let rotate = mc::Transform {
            m11: 0.0,
            m12: 1.0,
            m21: -1.0,
            m22: 0.0,
            dx: 0.0,
            dy: 0.0,
        };
        let (paint, tag) = map_paint(&mc::Paint::LinearGradient {
            start: mc::Point::new(0.0, 0.0),
            end: mc::Point::new(10.0, 10.0),
            stops,
            wrap_mode: 0,
            transform: rotate,
            gamma_corrected: false,
        });
        assert_eq!(tag, None, "a representable gradient is FULL fidelity");
        match paint {
            Some(MetafilePaint::LinearGradient { from, to, .. }) => {
                assert!((to.0 - from.0).abs() < 1.0e-3, "axis must be vertical");
                assert!(to.1 - from.1 > 0.99);
            }
            other => panic!("expected a linear gradient, got {other:?}"),
        }
    }

    #[test]
    fn hatches_map_to_drawingml_presets() {
        let brush = mc::Brush {
            style: mc::BrushStyle::Hatched(0),
            color: mc::Color::rgb(255, 0, 0),
        };
        let (paint, tag) = map_brush(&brush);
        assert_eq!(tag, Some("metafile_hatch"));
        assert_eq!(
            paint,
            Some(MetafilePaint::Pattern {
                preset: "horz",
                fg: [255, 0, 0, 255],
                bg: [255, 255, 255, 255],
            })
        );
    }
}
