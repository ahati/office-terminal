//! `oxml_layout::Paint` / `Stroke` → core `VectorPaint` / `VectorStroke`
//! (issue #40 §7.7.4).

use office_terminal_core::contracts::{
    VectorGradient, VectorPaint, VectorStroke, DEFAULT_MITER_LIMIT,
};
use oxml_layout::paint::{GradientStop, LineCap, LineJoin, Paint, Stroke};

use super::BridgeContext;

/// Clamp a 0..1 float component into a byte.
fn channel(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Opaque RGBA conversion of a layout colour.
pub(crate) fn rgba(color: &oxml_layout::Color) -> [u8; 4] {
    rgba_with_opacity(color.r, color.g, color.b, color.a, 1.0)
}

/// RGBA conversion with an explicit opacity multiplier (group opacity folds
/// into leaf paint alpha — no group-opacity command exists in the IR).
pub(crate) fn rgba_with_opacity(
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    opacity: f32,
) -> [u8; 4] {
    let alpha = channel(a) as f32 * opacity.clamp(0.0, 1.0);
    [
        channel(r),
        channel(g),
        channel(b),
        alpha.round().clamp(0.0, 255.0) as u8,
    ]
}

pub(crate) fn rgba_scaled(color: &oxml_layout::Color, opacity: f32) -> [u8; 4] {
    rgba_with_opacity(color.r, color.g, color.b, color.a, opacity)
}

/// Gradient stops with folded opacity; offsets clamped into [0, 1].
pub(crate) fn map_gradient_stops(stops: &[GradientStop], opacity: f32) -> Vec<(f32, [u8; 4])> {
    stops
        .iter()
        .map(|stop| {
            (
                (if stop.offset.is_finite() {
                    stop.offset
                } else {
                    0.0
                })
                .clamp(0.0, 1.0) as f32,
                rgba_scaled(&stop.color, opacity),
            )
        })
        .collect()
}

/// DrawingML-style angle (degrees clockwise from east, y down).
fn gradient_angle_deg(start: &oxml_layout::Point, end: &oxml_layout::Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let mut degrees = dy.atan2(dx).to_degrees();
    if degrees < 0.0 {
        degrees += 360.0;
    }
    degrees as f32
}

/// Map one paint for a fill or stroke use.
///
/// Solid and linear paints map exactly; radial projects onto the core's
/// gradient model with a warning; grid flattens to its mean colour; tile
/// (engine-internal media reference) degrades to `None` with a warning.
pub(crate) fn map_paint(
    paint: &Paint,
    opacity: f32,
    ctx: &BridgeContext<'_>,
    element_id: &str,
) -> Option<VectorPaint> {
    match paint {
        Paint::Solid(color) => Some(VectorPaint::Solid(rgba_scaled(color, opacity))),
        Paint::Linear {
            start, end, stops, ..
        } => Some(VectorPaint::LinearGradient(VectorGradient::new(
            gradient_angle_deg(start, end),
            map_gradient_stops(stops, opacity),
        ))),
        Paint::Radial { stops, .. } => {
            ctx.warn_once("docx-paint-radial", element_id.to_string());
            Some(VectorPaint::RadialGradient(VectorGradient::new(
                90.0,
                map_gradient_stops(stops, opacity),
            )))
        }
        Paint::Grid { colors, .. } => {
            ctx.warn_once("docx-paint-grid", element_id.to_string());
            Some(VectorPaint::Solid(average_color(colors, opacity)))
        }
        Paint::Tile { .. } => {
            ctx.warn_once("docx-paint-tile", element_id.to_string());
            None
        }
    }
    // `Paint` is exhaustive: a future variant fails this match at compile
    // time and forces a conscious mapping here.
}

/// Map an optional fill paint.
pub(crate) fn map_fill(
    fill: &Option<Paint>,
    opacity: f32,
    ctx: &BridgeContext<'_>,
    element_id: &str,
) -> Option<VectorPaint> {
    fill.as_ref().and_then(|paint| map_paint(paint, opacity, ctx, element_id))
}

/// Map a layout stroke: widths in points become EMU, dash lengths are
/// absolute points in the source and width multiples in the IR, so they are
/// divided by the stroke width (dropped for hairlines).
pub(crate) fn map_stroke(
    stroke: &Stroke,
    opacity: f32,
    ctx: &BridgeContext<'_>,
    element_id: &str,
) -> Option<VectorStroke> {
    let width_pt = if stroke.width.is_finite() {
        stroke.width.max(0.0)
    } else {
        0.0
    };
    if width_pt <= 0.0 {
        return None;
    }

    let paint = map_paint(&stroke.paint, opacity, ctx, element_id);
    let (color, gradient_paint) = match &paint {
        Some(VectorPaint::Solid(color)) => (*color, None),
        Some(other) => {
            // The core stroke keeps a flat `color` plus the richer paint; the
            // flat value only backs backends that ignore `paint`.
            let representative = first_solid_color(other).unwrap_or([0, 0, 0, 255]);
            (representative, Some(Box::new(other.clone())))
        }
        None => ([0, 0, 0, 255], None),
    };

    let dash_segments = stroke.dash.as_ref().map(|pattern| {
        pattern
            .iter()
            .map(|length| {
                let length = if length.is_finite() { *length } else { 0.0 };
                (length / width_pt).max(0.0) as f32
            })
            .collect::<Vec<f32>>()
    });

    Some(VectorStroke {
        color,
        width_emu: super::pt_to_emu(width_pt) as i64,
        dash_segments: dash_segments.filter(|segments: &Vec<f32>| !segments.is_empty()),
        line_cap: match stroke.cap {
            LineCap::Butt => office_terminal_core::contracts::LineCap::Butt,
            LineCap::Round => office_terminal_core::contracts::LineCap::Round,
            LineCap::Square => office_terminal_core::contracts::LineCap::Square,
        },
        line_join: match stroke.join {
            LineJoin::Miter => office_terminal_core::contracts::LineJoin::Miter,
            LineJoin::Round => office_terminal_core::contracts::LineJoin::Round,
            LineJoin::Bevel => office_terminal_core::contracts::LineJoin::Bevel,
        },
        paint: gradient_paint,
        miter_limit: DEFAULT_MITER_LIMIT,
        head_end: None,
        tail_end: None,
        compound: office_terminal_core::contracts::CompoundLine::Sng,
    })
}

/// Representative flat colour of a non-solid paint (first gradient stop).
fn first_solid_color(paint: &VectorPaint) -> Option<[u8; 4]> {
    match paint {
        VectorPaint::LinearGradient(gradient)
        | VectorPaint::RadialGradient(gradient) => gradient.stops.first().map(|(_, color)| *color),
        VectorPaint::Solid(color) => Some(*color),
        _ => None,
    }
}

/// Mean colour of a grid paint with folded opacity.
fn average_color(colors: &[oxml_layout::Color], opacity: f32) -> [u8; 4] {
    if colors.is_empty() {
        return rgba_scaled(&oxml_layout::Color::BLACK, opacity);
    }
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let mut a = 0.0;
    for color in colors {
        r += color.r;
        g += color.g;
        b += color.b;
        a += color.a;
    }
    let count = colors.len() as f64;
    rgba_with_opacity(r / count, g / count, b / count, a / count, opacity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{compile, letter_page, FixtureEngine};
    use office_terminal_core::contracts::{VectorCommand, VectorPaint, VectorStroke, WarningEvent};
    use oxml_layout::paint::{LineCap, LineJoin};
    use oxml_layout::path::{FillRule, Path as LayoutPath, PathCommand};
    use oxml_layout::{Color, PathElement, Point, PositionedElement};

    fn stroked_page(stroke: Stroke) -> (office_terminal_core::contracts::VectorSlide, Vec<WarningEvent>) {
        let path = LayoutPath {
            commands: vec![
                PathCommand::MoveTo(Point { x: 10.0, y: 10.0 }),
                PathCommand::LineTo(Point { x: 100.0, y: 10.0 }),
                PathCommand::Close,
            ],
            fill_rule: FillRule::NonZero,
        };
        let page = letter_page(vec![PositionedElement::Path(PathElement {
            path,
            fill: None,
            stroke: Some(stroke),
        })]);
        let engine = FixtureEngine::new(Vec::new());
        compile(&page, &engine)
    }

    fn only_stroke(slide: &office_terminal_core::contracts::VectorSlide) -> &VectorStroke {
        let VectorCommand::DrawPath { stroke, .. } = &slide.commands[0] else {
            panic!("expected a stroked path command");
        };
        stroke.as_ref().expect("stroke present")
    }

    #[test]
    fn solid_paints_fold_group_opacity_into_alpha() {
        let (slide, warnings) = stroked_page(Stroke::new(Paint::Solid(Color::BLACK), 2.0));
        assert!(warnings.is_empty());
        assert_eq!(only_stroke(&slide).color, [0, 0, 0, 255]);
    }

    #[test]
    fn stroke_dash_lengths_convert_to_width_multiples() {
        let mut stroke = Stroke::new(Paint::Solid(Color::BLACK), 2.0);
        stroke.dash = Some(vec![6.0, 3.0]);
        let (slide, _warnings) = stroked_page(stroke);
        let mapped = only_stroke(&slide);
        assert_eq!(mapped.width_emu, 2 * 12_700);
        assert_eq!(mapped.dash_segments.as_deref(), Some(&[3.0f32, 1.5][..]));
        assert_eq!(mapped.line_cap, office_terminal_core::contracts::LineCap::Butt);
        assert_eq!(mapped.line_join, office_terminal_core::contracts::LineJoin::Miter);
    }

    #[test]
    fn hairline_strokes_are_dropped() {
        let (slide, _warnings) = stroked_page(Stroke::new(Paint::Solid(Color::BLACK), 0.0));
        assert!(slide.commands.is_empty());
    }

    #[test]
    fn non_solid_stroke_paints_carry_both_representations() {
        let stroke = Stroke::new(
            Paint::linear(
                Point { x: 0.0, y: 0.0 },
                Point { x: 10.0, y: 0.0 },
                vec![
                    GradientStop { offset: 0.0, color: Color::BLACK },
                    GradientStop { offset: 1.0, color: Color::WHITE },
                ],
                (false, false),
            ),
            1.5,
        );
        let (slide, _warnings) = stroked_page(stroke);
        let mapped = only_stroke(&slide);
        assert_eq!(mapped.color, [0, 0, 0, 255]);
        assert!(mapped.paint.is_some());
    }

    #[test]
    fn radial_paints_warn_once_per_document() {
        let radial = Paint::radial(
            Point { x: 0.0, y: 0.0 },
            5.0,
            Point { x: 0.0, y: 0.0 },
            vec![
                GradientStop { offset: 0.0, color: Color::WHITE },
                GradientStop { offset: 1.0, color: Color::BLACK },
            ],
            (false, false),
        );
        let (first, warnings) = stroked_page(Stroke::new(radial.clone(), 2.0));
        let (second, more) = stroked_page(Stroke::new(radial, 2.0));
        assert!(matches!(
            only_stroke(&first).paint.as_deref(),
            Some(VectorPaint::RadialGradient(_))
        ));
        assert!(matches!(
            only_stroke(&second).paint.as_deref(),
            Some(VectorPaint::RadialGradient(_))
        ));
        // Each `compile` in this test builds a fresh dedup set (as a fresh
        // document would), so both pages warn — but a real document's pages
        // share one set and fire once (covered in source.rs).
        assert_eq!(warnings.len(), 1);
        assert_eq!(more.len(), 1);
    }

    #[test]
    fn grid_paints_flatten_to_their_mean_colour() {
        let grid = Paint::grid(
            oxml_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            2,
            2,
            vec![Color::WHITE, Color::BLACK, Color::BLACK, Color::WHITE],
        )
        .expect("valid grid");
        let (slide, _warnings) = stroked_page(Stroke::new(grid, 2.0));
        assert_eq!(only_stroke(&slide).color, [128, 128, 128, 255]);
    }

    #[test]
    fn stroke_mapping_covers_the_cap_and_join_vocabulary() {
        for (cap, expected) in [
            (LineCap::Butt, office_terminal_core::contracts::LineCap::Butt),
            (LineCap::Round, office_terminal_core::contracts::LineCap::Round),
            (LineCap::Square, office_terminal_core::contracts::LineCap::Square),
        ] {
            let mut stroke = Stroke::new(Paint::Solid(Color::BLACK), 1.0);
            stroke.cap = cap;
            let (slide, _warnings) = stroked_page(stroke);
            assert_eq!(only_stroke(&slide).line_cap, expected);
        }
        for (join, expected) in [
            (LineJoin::Miter, office_terminal_core::contracts::LineJoin::Miter),
            (LineJoin::Round, office_terminal_core::contracts::LineJoin::Round),
            (LineJoin::Bevel, office_terminal_core::contracts::LineJoin::Bevel),
        ] {
            let mut stroke = Stroke::new(Paint::Solid(Color::BLACK), 1.0);
            stroke.join = join;
            let (slide, _warnings) = stroked_page(stroke);
            assert_eq!(only_stroke(&slide).line_join, expected);
        }
    }
}
