//! Path, line and rectangle geometry mapping (issue #40 §7.7.4).

use office_terminal_core::contracts::{
    FillRule as CoreFillRule, PathSegment, VectorCommand, VectorPath,
};
use oxml_layout::path::{FillRule, Path as LayoutPath, PathCommand};
use oxml_layout::{Point, Rect};

use super::paint::{map_fill, map_stroke, rgba_scaled};
use super::{pt_to_emu, BridgeContext, BridgeState};

/// Map one backend-neutral path element.
pub(crate) fn map_path(
    element: &oxml_layout::PathElement,
    acc: &office_terminal_core::contracts::AffineTransform,
    opacity: f32,
    slide: &mut office_terminal_core::contracts::VectorSlide,
    ctx: &BridgeContext<'_>,
    state: &mut BridgeState,
) {
    if element.path.commands.is_empty() {
        return;
    }
    let path = map_path_geometry(&element.path);
    let fill = map_fill(&element.fill, opacity, ctx, "path");
    let stroke = map_stroke_ref(&element.stroke, opacity, ctx, "path");
    if fill.is_none() && stroke.is_none() {
        return;
    }
    let mut effects = office_terminal_core::contracts::ShapeEffects::default();
    if let Some(shadow) = state.pending_shadow.take() {
        effects.outer_shadow = Some(shadow);
    }
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill,
        stroke,
        transform: *acc,
        effects: Box::new(effects),
    });
}

/// Stroke mapping through the shared paint module.
fn map_stroke_ref(
    stroke: &Option<oxml_layout::paint::Stroke>,
    opacity: f32,
    ctx: &BridgeContext<'_>,
    element_id: &str,
) -> Option<office_terminal_core::contracts::VectorStroke> {
    stroke.as_ref().and_then(|s| map_stroke(s, opacity, ctx, element_id))
}

/// Points → EMU path geometry, fill rule preserved.
pub(crate) fn map_path_geometry(layout: &LayoutPath) -> VectorPath {
    let segments = layout
        .commands
        .iter()
        .map(|command| match command {
            PathCommand::MoveTo(point) => PathSegment::MoveTo {
                x: pt_to_emu(point.x),
                y: pt_to_emu(point.y),
            },
            PathCommand::LineTo(point) => PathSegment::LineTo {
                x: pt_to_emu(point.x),
                y: pt_to_emu(point.y),
            },
            PathCommand::CurveTo { c1, c2, to } => PathSegment::CubicTo {
                x1: pt_to_emu(c1.x),
                y1: pt_to_emu(c1.y),
                x2: pt_to_emu(c2.x),
                y2: pt_to_emu(c2.y),
                x: pt_to_emu(to.x),
                y: pt_to_emu(to.y),
            },
            PathCommand::Close => PathSegment::Close,
        })
        .collect();
    VectorPath::new(
        segments,
        match layout.fill_rule {
            FillRule::NonZero => CoreFillRule::NonZero,
            FillRule::EvenOdd => CoreFillRule::EvenOdd,
        },
    )
}

/// Map a `PositionedElement::Line` (borders, underlines, strikethrough) to an
/// open stroked path. Dash lengths convert from absolute points to width
/// multiples; zero-length or zero-width lines carry no ink and are skipped.
#[allow(clippy::too_many_arguments)]
pub(crate) fn map_line(
    start: &Point,
    end: &Point,
    width: f64,
    color: &oxml_layout::Color,
    dash_pattern: Option<(f64, f64)>,
    acc: &office_terminal_core::contracts::AffineTransform,
    opacity: f32,
    slide: &mut office_terminal_core::contracts::VectorSlide,
) {
    let width = if width.is_finite() { width.max(0.0) } else { 0.0 };
    if width <= 0.0 || (start.x == end.x && start.y == end.y) {
        return;
    }
    let dash_segments = dash_pattern.map(|(on, off)| {
        let on = if on.is_finite() { on } else { 0.0 };
        let off = if off.is_finite() { off } else { 0.0 };
        vec![(on / width).max(0.0) as f32, (off / width).max(0.0) as f32]
    });
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo {
                x: pt_to_emu(start.x),
                y: pt_to_emu(start.y),
            },
            PathSegment::LineTo {
                x: pt_to_emu(end.x),
                y: pt_to_emu(end.y),
            },
        ],
        CoreFillRule::NonZero,
    );
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: None,
        stroke: Some(office_terminal_core::contracts::VectorStroke {
            color: rgba_scaled(color, opacity),
            width_emu: pt_to_emu(width) as i64,
            dash_segments,
            line_cap: office_terminal_core::contracts::LineCap::Butt,
            line_join: office_terminal_core::contracts::LineJoin::Miter,
            paint: None,
            miter_limit: office_terminal_core::contracts::DEFAULT_MITER_LIMIT,
            head_end: None,
            tail_end: None,
            compound: office_terminal_core::contracts::CompoundLine::Sng,
        }),
        transform: *acc,
        effects: Box::default(),
    });
}

/// Map a `PositionedElement::FilledRect` (shading, highlights) to a closed
/// rectangle path.
pub(crate) fn map_filled_rect(
    rect: &Rect,
    color: &oxml_layout::Color,
    acc: &office_terminal_core::contracts::AffineTransform,
    opacity: f32,
    slide: &mut office_terminal_core::contracts::VectorSlide,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let x = pt_to_emu(rect.x);
    let y = pt_to_emu(rect.y);
    let w = pt_to_emu(rect.width);
    let h = pt_to_emu(rect.height);
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x, y },
            PathSegment::LineTo { x: x + w, y },
            PathSegment::LineTo { x: x + w, y: y + h },
            PathSegment::LineTo { x, y: y + h },
            PathSegment::Close,
        ],
        CoreFillRule::NonZero,
    );
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: Some(office_terminal_core::contracts::VectorPaint::Solid(rgba_scaled(
            color, opacity,
        ))),
        stroke: None,
        transform: *acc,
        effects: Box::default(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{compile, letter_page, FixtureEngine};
    use oxml_layout::paint::Paint;
    use oxml_layout::path::{FillRule as LayoutFillRule, Path as LayoutPath, PathCommand};
    use oxml_layout::{Color, PathElement, Point, PositionedElement, Rect};

    fn path_element(fill: Option<Paint>, stroke: Option<oxml_layout::paint::Stroke>) -> PositionedElement {
        let path = LayoutPath {
            commands: vec![
                PathCommand::MoveTo(Point { x: 10.0, y: 10.0 }),
                PathCommand::LineTo(Point { x: 110.0, y: 10.0 }),
                PathCommand::LineTo(Point { x: 110.0, y: 60.0 }),
                PathCommand::Close,
            ],
            fill_rule: LayoutFillRule::EvenOdd,
        };
        PositionedElement::Path(PathElement { path, fill, stroke })
    }

    #[test]
    fn filled_paths_keep_geometry_fill_rule_and_transform() {
        let page = letter_page(vec![path_element(
            Some(Paint::Solid(Color::BLACK)),
            None,
        )]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());
        assert_eq!(slide.commands.len(), 1);
        let VectorCommand::DrawPath { path, fill, .. } = &slide.commands[0] else {
            panic!("expected a path command");
        };
        assert_eq!(path.segments.len(), 4);
        assert_eq!(path.fill_rule, CoreFillRule::EvenOdd);
        assert_eq!(fill, &Some(office_terminal_core::contracts::VectorPaint::Solid([0, 0, 0, 255])));
    }

    #[test]
    fn lines_become_open_stroked_paths_with_dash_multiples() {
        let page = letter_page(vec![PositionedElement::Line {
            start: Point { x: 10.0, y: 10.0 },
            end: Point { x: 100.0, y: 10.0 },
            width: 2.0,
            color: Color::BLACK,
            dash_pattern: Some((6.0, 2.0)),
        }]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);
        let VectorCommand::DrawPath { path, stroke, .. } = &slide.commands[0] else {
            panic!("expected a line command");
        };
        assert_eq!(path.segments.len(), 2);
        let stroke = stroke.as_ref().expect("stroke");
        assert_eq!(stroke.width_emu, 2 * 12_700);
        assert_eq!(stroke.dash_segments.as_deref(), Some(&[3.0f32, 1.0][..]));
    }

    #[test]
    fn zero_width_and_zero_length_lines_are_skipped() {
        let page = letter_page(vec![
            PositionedElement::Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 10.0, y: 0.0 },
                width: 0.0,
                color: Color::BLACK,
                dash_pattern: None,
            },
            PositionedElement::Line {
                start: Point { x: 0.0, y: 5.0 },
                end: Point { x: 0.0, y: 5.0 },
                width: 1.0,
                color: Color::BLACK,
                dash_pattern: None,
            },
        ]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);
        assert!(slide.commands.is_empty());
    }

    #[test]
    fn filled_rects_become_closed_rect_paths() {
        let page = letter_page(vec![PositionedElement::FilledRect {
            rect: Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
            },
            color: Color::WHITE,
        }]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);
        let VectorCommand::DrawPath { path, fill, .. } = &slide.commands[0] else {
            panic!("expected a rect command");
        };
        assert_eq!(path.segments.len(), 5);
        assert_eq!(
            fill,
            &Some(office_terminal_core::contracts::VectorPaint::Solid([
                255, 255, 255, 255
            ]))
        );
    }

    #[test]
    fn paths_without_paint_are_skipped() {
        let page = letter_page(vec![path_element(None, None)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);
        assert!(slide.commands.is_empty());
    }
}
