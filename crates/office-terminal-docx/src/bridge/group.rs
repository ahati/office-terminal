//! Group recursion: transforms, clips, opacity folding and effects
//! (issue #40 §7.7.6).

use office_terminal_core::contracts::{
    AffineTransform, VectorCommand, VectorSlide, VectorOuterShadow,
};
use oxml_layout::{Effect, GroupElement};

use super::{core_transform, map_elements, BridgeContext, BridgeState};

/// Recurse into one group element.
///
/// - `transform` composes child-local → page (the core `post_concat` order
///   matches `group.then(accumulated)` in the layout IR);
/// - a group clip is pre-transformed into page space, because `PushClip`
///   carries no transform of its own;
/// - group opacity has no IR command: it multiplies into descendant paint
///   alphas with a one-time warning (exact for non-overlapping children,
///   approximate where children overlap);
/// - `OuterShadow` attaches to the group's next emitted path, and only ever
///   to a path this group's own children emitted (never to unrelated page
///   ink); a nested group's shadow displaces a pending outer one with a
///   warning; `LinearOpacityMask` is not representable and drops with a
///   warning.
#[allow(clippy::too_many_arguments)]
pub(crate) fn map_group(
    group: &GroupElement,
    acc: &AffineTransform,
    opacity: f32,
    slide: &mut VectorSlide,
    ctx: &BridgeContext<'_>,
    state: &mut BridgeState,
) {
    let group_core = core_transform(&group.transform);
    let child_acc = acc.post_concat(&group_core);
    let child_opacity = (opacity * clamp01(group.opacity)).max(0.0);
    if child_opacity < 0.999 && clamp01(group.opacity) < 0.999 {
        ctx.warn_once("docx-group-opacity", "group".to_string());
    }

    for effect in &group.effects {
        match effect {
            Effect::OuterShadow {
                dx,
                dy,
                blur,
                color,
            } => {
                // A nested group's shadow is the closer one and wins; the
                // outer shadow it displaces would otherwise vanish silently.
                if state.pending_shadow.take().is_some() {
                    ctx.warn_once("docx-shadow-displaced", "group".to_string());
                }
                state.pending_shadow = Some(VectorOuterShadow {
                    color: super::paint::rgba_scaled(color, 1.0),
                    blur_radius_emu: super::pt_to_emu(*blur) as i64,
                    distance_emu: super::pt_to_emu(hypot(*dx, *dy)) as i64,
                    direction_deg: dy.atan2(*dx).to_degrees(),
                    scale_x: 1.0,
                    scale_y: 1.0,
                    skew_x_deg: 0.0,
                    skew_y_deg: 0.0,
                    rotate_with_shape: false,
                });
            }
            Effect::LinearOpacityMask { .. } => {
                ctx.warn_once("docx-opacity-mask", "group".to_string());
            }
            // `Effect` is `#[non_exhaustive]`: unknown effects degrade, they
            // never fail the page.
            _ => {
                ctx.warn_once("docx-effect-unknown", "group".to_string());
            }
        }
    }

    // `PushClip` carries no transform of its own, so the clip geometry is
    // pre-transformed from child-local into page space (issue #40 §7.7.4).
    let pushed_clip = if let Some(clip) = group.clip.as_ref() {
        let clip_path = pretransformed_clip(clip, &child_acc);
        if !clip_path.segments.is_empty() {
            slide.commands.push(VectorCommand::PushClip(clip_path));
            true
        } else {
            false
        }
    } else {
        false
    };

    // Only a path emitted by this group's own children may inherit an
    // unconsumed shadow: attaching to unrelated earlier page ink (a table
    // border, a cell fill) would paint a shadow where none was authored.
    let first_child_command = slide.commands.len();
    map_elements(
        &group.children,
        &child_acc,
        child_opacity,
        slide,
        ctx,
        state,
    );

    if let Some(unused) = state.pending_shadow.take() {
        if !attach_shadow_to_last_path(slide, first_child_command, unused) {
            ctx.warn_once("docx-shadow-unattached", "group".to_string());
        }
    }

    if pushed_clip {
        slide.commands.push(VectorCommand::PopClip);
    }
}

fn clamp01(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0) as f32
    } else {
        1.0
    }
}

fn hypot(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt()
}

/// Pre-transform a child-local clip path into page-space EMU.
fn pretransformed_clip(
    clip: &oxml_layout::path::Path,
    child_acc: &AffineTransform,
) -> office_terminal_core::contracts::VectorPath {
    let mut segments = Vec::with_capacity(clip.commands.len());
    for command in &clip.commands {
        use oxml_layout::path::PathCommand;
        let point_emu = |point: &oxml_layout::Point| {
            let (x, y) = child_acc.transform_point(
                super::pt_to_emu(point.x),
                super::pt_to_emu(point.y),
            );
            (x, y)
        };
        match command {
            PathCommand::MoveTo(point) => {
                let (x, y) = point_emu(point);
                segments.push(office_terminal_core::contracts::PathSegment::MoveTo { x, y });
            }
            PathCommand::LineTo(point) => {
                let (x, y) = point_emu(point);
                segments.push(office_terminal_core::contracts::PathSegment::LineTo { x, y });
            }
            PathCommand::CurveTo { c1, c2, to } => {
                let (x1, y1) = point_emu(c1);
                let (x2, y2) = point_emu(c2);
                let (x, y) = point_emu(to);
                segments.push(office_terminal_core::contracts::PathSegment::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
            }
            PathCommand::Close => {
                segments.push(office_terminal_core::contracts::PathSegment::Close)
            }
        }
    }
    office_terminal_core::contracts::VectorPath::new(
        segments,
        match clip.fill_rule {
            oxml_layout::path::FillRule::NonZero => {
                office_terminal_core::contracts::FillRule::NonZero
            }
            oxml_layout::path::FillRule::EvenOdd => {
                office_terminal_core::contracts::FillRule::EvenOdd
            }
        },
    )
}

/// Fold a group shadow onto the most recently emitted path at or after
/// `from` (the group's first child command). Returns `false` when no path
/// in that range exists.
fn attach_shadow_to_last_path(
    slide: &mut VectorSlide,
    from: usize,
    shadow: VectorOuterShadow,
) -> bool {
    for command in slide.commands[from..].iter_mut().rev() {
        if let VectorCommand::DrawPath { effects, .. } = command {
            effects.outer_shadow = Some(shadow);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_terminal_core::contracts::WarningEvent;
    use crate::test_support::{compile, letter_page, text_run, FixtureEngine};
    use oxml_layout::paint::Paint;
    use oxml_layout::path::{FillRule, Path as LayoutPath, PathCommand};
    use oxml_layout::{
        Color, GroupElement, PathElement, Point, PositionedElement, Rect,
    };

    fn clip_path() -> LayoutPath {
        LayoutPath {
            commands: vec![
                PathCommand::MoveTo(Point { x: 10.0, y: 10.0 }),
                PathCommand::LineTo(Point { x: 200.0, y: 10.0 }),
                PathCommand::LineTo(Point { x: 200.0, y: 300.0 }),
                PathCommand::LineTo(Point { x: 10.0, y: 300.0 }),
                PathCommand::Close,
            ],
            fill_rule: FillRule::NonZero,
        }
    }

    fn filled_rect_command() -> PositionedElement {
        PositionedElement::FilledRect {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 500.0,
                height: 500.0,
            },
            color: Color::BLACK,
        }
    }

    #[test]
    fn group_transform_composes_onto_children() {
        let group = GroupElement {
            transform: oxml_layout::Transform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 36.0,
                f: 18.0,
            },
            clip: None,
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![PositionedElement::Text(text_run("X", 72.0, 90.0, 0, 12.0))],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new()).with_font(0, "Carlito");
        let (slide, _warnings) = compile(&page, &engine);

        let VectorCommand::DrawText { transform, .. } = &slide.commands[0] else {
            panic!("expected a text command");
        };
        assert_eq!(transform.tx, 36.0 * 12_700.0);
        assert_eq!(transform.ty, 18.0 * 12_700.0);
    }

    #[test]
    fn group_clips_wrap_children_in_page_space() {
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: Some(clip_path()),
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![filled_rect_command()],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);

        assert!(matches!(slide.commands[0], VectorCommand::PushClip(_)));
        assert!(matches!(slide.commands[1], VectorCommand::DrawPath { .. }));
        assert!(matches!(slide.commands[2], VectorCommand::PopClip));
    }

    #[test]
    fn group_opacity_folds_into_child_paint_alpha() {
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 0.5,
            effects: Vec::new(),
            children: vec![filled_rect_command()],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);

        let VectorCommand::DrawPath { fill, .. } = &slide.commands[0] else {
            panic!("expected a path command");
        };
        let Some(office_terminal_core::contracts::VectorPaint::Solid(color)) = fill else {
            panic!("expected a solid fill");
        };
        assert_eq!(color[3], 128, "0.5 opacity folds into alpha");
        assert!(warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded {
                feature: "docx-group-opacity",
                ..
            }
        )));
    }

    #[test]
    fn outer_shadows_attach_to_the_groups_path_child() {
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![Effect::OuterShadow {
                dx: 2.0,
                dy: 3.0,
                blur: 4.0,
                color: Color::BLACK,
            }],
            children: vec![PositionedElement::Path(PathElement {
                path: LayoutPath {
                    commands: vec![
                        PathCommand::MoveTo(Point { x: 0.0, y: 0.0 }),
                        PathCommand::LineTo(Point { x: 10.0, y: 0.0 }),
                        PathCommand::LineTo(Point { x: 10.0, y: 10.0 }),
                        PathCommand::Close,
                    ],
                    fill_rule: FillRule::NonZero,
                },
                fill: Some(Paint::Solid(Color::BLACK)),
                stroke: None,
            })],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, _warnings) = compile(&page, &engine);

        let VectorCommand::DrawPath { effects, .. } = &slide.commands[0] else {
            panic!("expected a path command");
        };
        let shadow = effects.outer_shadow.as_ref().expect("shadow attached");
        assert_eq!(shadow.blur_radius_emu, 4 * 12_700);
        assert_eq!(shadow.distance_emu, (13.0f64.sqrt() * 12_700.0).round() as i64);
    }

    fn shadow_effect(blur: f64) -> Effect {
        Effect::OuterShadow {
            dx: 2.0,
            dy: 3.0,
            blur,
            color: Color::BLACK,
        }
    }

    #[test]
    fn unconsumed_shadows_stay_on_the_groups_own_ink_never_unrelated_page_ink() {
        // A border-like path emitted before the group must not inherit the
        // group's shadow when the group has no path child of its own.
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![shadow_effect(4.0)],
            children: vec![PositionedElement::Text(text_run("X", 0.0, 0.0, 0, 12.0))],
        };
        let page = letter_page(vec![filled_rect_command(), PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new()).with_font(0, "Carlito");
        let (slide, warnings) = compile(&page, &engine);

        let VectorCommand::DrawPath { effects, .. } = &slide.commands[0] else {
            panic!("expected the unrelated path");
        };
        assert!(effects.outer_shadow.is_none(), "unrelated ink stays shadow-free");
        assert!(warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded {
                feature: "docx-shadow-unattached",
                ..
            }
        )));
    }

    #[test]
    fn filled_rect_children_inherit_the_groups_shadow() {
        // A `FilledRect` is a path child too: the fallback must find it
        // inside the group's own command range.
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![shadow_effect(4.0)],
            children: vec![filled_rect_command()],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);

        let VectorCommand::DrawPath { effects, .. } = &slide.commands[0] else {
            panic!("expected the rect path");
        };
        assert_eq!(
            effects.outer_shadow.as_ref().map(|s| s.blur_radius_emu),
            Some(4 * 12_700)
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn nested_group_shadows_displace_the_outer_one_with_a_warning() {
        let inner = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![shadow_effect(9.0)],
            children: vec![PositionedElement::Path(PathElement {
                path: LayoutPath {
                    commands: vec![
                        PathCommand::MoveTo(Point { x: 0.0, y: 0.0 }),
                        PathCommand::LineTo(Point { x: 10.0, y: 0.0 }),
                        PathCommand::LineTo(Point { x: 10.0, y: 10.0 }),
                        PathCommand::Close,
                    ],
                    fill_rule: FillRule::NonZero,
                },
                fill: Some(Paint::Solid(Color::BLACK)),
                stroke: None,
            })],
        };
        let outer = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![shadow_effect(2.0)],
            children: vec![PositionedElement::Group(inner)],
        };
        let page = letter_page(vec![PositionedElement::Group(outer)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);

        let VectorCommand::DrawPath { effects, .. } = &slide.commands[0] else {
            panic!("expected the path");
        };
        // The closest (inner) shadow wins.
        assert_eq!(
            effects.outer_shadow.as_ref().map(|s| s.blur_radius_emu),
            Some(9 * 12_700)
        );
        assert!(warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded {
                feature: "docx-shadow-displaced",
                ..
            }
        )));
    }

    #[test]
    fn opacity_masks_drop_with_a_warning() {
        let group = GroupElement {
            transform: oxml_layout::Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            effects: vec![Effect::LinearOpacityMask {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 100.0, y: 0.0 },
                stops: vec![
                    oxml_layout::GradientStop {
                        offset: 0.0,
                        color: Color::WHITE,
                    },
                    oxml_layout::GradientStop {
                        offset: 1.0,
                        color: Color::BLACK,
                    },
                ],
            }],
            children: vec![filled_rect_command()],
        };
        let page = letter_page(vec![PositionedElement::Group(group)]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert_eq!(slide.commands.len(), 1);
        assert!(warnings.iter().any(|w| matches!(
            w,
            WarningEvent::FidelityDegraded {
                feature: "docx-opacity-mask",
                ..
            }
        )));
    }

    #[test]
    fn marked_content_recurses_without_ink() {
        let page = letter_page(vec![PositionedElement::MarkedContent {
            structure: None,
            children: vec![PositionedElement::Text(text_run("tagged", 0.0, 0.0, 0, 12.0))],
        }]);
        let engine = FixtureEngine::new(Vec::new()).with_font(0, "Carlito");
        let (slide, warnings) = compile(&page, &engine);
        assert_eq!(slide.commands.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn nested_group_transforms_compose_in_order() {
        let inner = GroupElement {
            transform: oxml_layout::Transform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 10.0,
                f: 0.0,
            },
            clip: None,
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![PositionedElement::Text(text_run("deep", 0.0, 0.0, 0, 12.0))],
        };
        let outer = GroupElement {
            transform: oxml_layout::Transform {
                a: 2.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 100.0,
                f: 0.0,
            },
            clip: None,
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![PositionedElement::Group(inner)],
        };
        let page = letter_page(vec![PositionedElement::Group(outer)]);
        let engine = FixtureEngine::new(Vec::new()).with_font(0, "Carlito");
        let (slide, _warnings) = compile(&page, &engine);

        let VectorCommand::DrawText { transform, .. } = &slide.commands[0] else {
            panic!("expected a text command");
        };
        // Outer scale 2 on x and translate 100 pt; inner translate 10 pt.
        assert_eq!(transform.sx, 2.0);
        assert_eq!(transform.tx, (100.0 + 2.0 * 10.0) * 12_700.0);
    }
}
