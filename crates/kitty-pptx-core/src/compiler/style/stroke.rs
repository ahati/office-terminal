//! Display-list expansion of one stroked subpath.
//!
//! `a:headEnd` / `a:tailEnd` (ECMA-376 §20.1.8.38 / §20.1.8.57) and
//! `a:ln@cmpd` (§20.1.8.42) change the *painted geometry* of a line, not its
//! style: the shaft is retracted so it stops where Office stops it and the
//! decorations are added paths. The reference does this at paint time
//! (`renderer.ts:4380-4535`, `7429-7530`); the IR is vector-first, so the
//! compiler emits the resolved geometry and every backend stays identical.

use crate::contracts::{
    AffineTransform, CompoundLine, LineCap, LineJoin, LineEnd, PathSegment, ShapeEffects,
    VectorCommand, VectorPaint, VectorPath, VectorStroke,
};

use super::line_end;

/// Which line-end decorations a subpath may paint.
///
/// `a:headEnd` / `a:tailEnd` decorate a connector's leader and custom-geometry
/// path ends only — the reference returns before painting them for any other
/// geometry (`paintLineDecorations`, ref renderer.ts:4413-4535).
pub struct StrokeScope {
    /// The subpath is the shape's leader / custom path, so it carries the line
    /// ends. Callers set this for the last stroked subpath only: a callout's
    /// text rectangle and accent bar share the same `<a:ln>`.
    pub decorate: bool,
}

/// Whether `geometry` is one of the reference's decorated leader kinds
/// (`CONNECTOR_GEOMS` / `CALLOUT_GEOMS`, ref renderer.ts:4217-4234).
pub fn geometry_decorates_line_ends(geometry: &str) -> bool {
    let name = geometry.to_ascii_lowercase();
    const CONNECTORS: &[&str] = &[
        "line",
        "straightconnector1",
        "bentconnector2",
        "bentconnector3",
        "bentconnector4",
        "bentconnector5",
        "curvedconnector2",
        "curvedconnector3",
        "curvedconnector4",
        "curvedconnector5",
    ];
    const CALLOUTS: &[&str] = &[
        "callout1",
        "callout2",
        "callout3",
        "bordercallout1",
        "bordercallout2",
        "bordercallout3",
        "accentcallout1",
        "accentcallout2",
        "accentcallout3",
        "accentbordercallout1",
        "accentbordercallout2",
        "accentbordercallout3",
    ];
    name == "custgeom" || CONNECTORS.contains(&name.as_str()) || CALLOUTS.contains(&name.as_str())
}

/// One `DrawPath`/`DrawLineEnd` set for a stroked subpath, expanded into
/// everything the reference paints with it:
///
/// 1. the shaft, shortened by the authored ends' retraction
///    (`trimGeometryPathEnds`, ref arrow-geometry.ts:326-333) — or, for a
///    compound straight line, its parallel rails (ref renderer.ts:4494-4512);
/// 2. the `a:tailEnd` / `a:headEnd` decorations at the *untrimmed* line ends,
///    each its own [`VectorCommand::DrawLineEnd`] because the reference paints
///    them in a translated tip frame (ref renderer.ts:4513-4535).
///
/// Decorations are emitted after the shaft, in the reference's tail-then-head
/// order. They carry no effects of their own: the reference composites them
/// into the same target, while the IR gives each command its own effect layer.
pub fn compile_stroked_paths(
    path: &VectorPath,
    fill: Option<VectorPaint>,
    stroke: Option<&VectorStroke>,
    scope: &StrokeScope,
    transform: AffineTransform,
    effects: ShapeEffects,
) -> Vec<VectorCommand> {
    let Some(vector_stroke) = stroke else {
        // No outline: the fill keeps the shape's effects (PRD 08).
        return vec![VectorCommand::DrawPath {
            path: path.clone(),
            fill,
            stroke: None,
            transform,
            effects: Box::new(effects),
        }];
    };

    let head = scope.decorate.then_some(vector_stroke.head_end).flatten();
    let tail = scope.decorate.then_some(vector_stroke.tail_end).flatten();
    let head_trim = head.map_or(0.0, |end| line_end::retract(vector_stroke.width_emu, &end));
    let tail_trim = tail.map_or(0.0, |end| line_end::retract(vector_stroke.width_emu, &end));

    // §20.1.8.42 compound straight line: the centre line is erased and each
    // rail painted at its own offset (ref renderer.ts:4479-4512).
    if vector_stroke.compound.is_compound() && line_end::straight_run(path).is_some() {
        let mut out = compound_rail_commands(
            path,
            fill,
            vector_stroke,
            head_trim,
            tail_trim,
            transform,
            effects,
        );
        out.extend(line_end_decorations(
            path,
            vector_stroke,
            head,
            tail,
            transform,
        ));
        return out;
    }

    let shaft = line_end::trim_path_ends(path, head_trim, tail_trim);
    let has_shaft = shaft
        .segments
        .iter()
        .any(|segment| !matches!(segment, PathSegment::Close));
    let mut out = Vec::new();
    if has_shaft {
        out.push(VectorCommand::DrawPath {
            path: shaft,
            fill,
            stroke: Some(vector_stroke.clone()),
            transform,
            effects: Box::new(effects),
        });
    }
    out.extend(line_end_decorations(
        path,
        vector_stroke,
        head,
        tail,
        transform,
    ));
    out
}

/// The parallel rails of one compound straight run.
///
/// Both ends trim the same run; when they consume it no line remains and the
/// reference paints nothing (ref renderer.ts:4494-4497). The first rail keeps
/// the shape's fill and effects so a compound outline still carries its shadow.
fn compound_rail_commands(
    path: &VectorPath,
    fill: Option<VectorPaint>,
    stroke: &VectorStroke,
    head_trim: f32,
    tail_trim: f32,
    transform: AffineTransform,
    effects: ShapeEffects,
) -> Vec<VectorCommand> {
    let trimmed = line_end::trim_path_ends(path, head_trim, tail_trim);
    let Some((start, end)) = line_end::straight_run(&trimmed) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, (rail_start, rail_end, width)) in line_end::compound_rails(
        start,
        end,
        stroke.width_emu as f32,
        stroke.compound,
    )
    .into_iter()
    .enumerate()
    {
        let rail = VectorPath::new(
            vec![
                PathSegment::MoveTo {
                    x: rail_start.0,
                    y: rail_start.1,
                },
                PathSegment::LineTo {
                    x: rail_end.0,
                    y: rail_end.1,
                },
            ],
            path.fill_rule,
        );
        let mut rail_stroke = stroke.clone();
        rail_stroke.width_emu = width.max(1.0) as i64;
        // A rail is a painted line, not a compound line.
        rail_stroke.compound = CompoundLine::Sng;
        rail_stroke.head_end = None;
        rail_stroke.tail_end = None;
        out.push(VectorCommand::DrawPath {
            path: rail,
            fill: if index == 0 { fill.clone() } else { None },
            stroke: Some(rail_stroke),
            transform,
            effects: Box::new(if index == 0 {
                effects.clone()
            } else {
                ShapeEffects::default()
            }),
        });
    }
    out
}

/// `a:tailEnd` then `a:headEnd`, each at its authored (untrimmed) line end
/// (ref renderer.ts:4513-4535).
fn line_end_decorations(
    path: &VectorPath,
    stroke: &VectorStroke,
    head: Option<LineEnd>,
    tail: Option<LineEnd>,
    transform: AffineTransform,
) -> Vec<VectorCommand> {
    if head.is_none() && tail.is_none() {
        return Vec::new();
    }
    let Some((head_end, tail_end)) = line_end::path_ends(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (end, placement) in [(tail, tail_end), (head, head_end)] {
        let Some(end) = end.filter(|end| !end.is_none()) else {
            continue;
        };
        let Some(geometry) = line_end::arrow_geometry(stroke.width_emu, &end) else {
            continue;
        };
        // An open arrow is two arms stroked at the line width with round caps
        // and a miter join; the closed outlines are filled with the line paint
        // (ref arrow-geometry.ts:186-188).
        let placed = line_end::place(&geometry, placement);
        if geometry.filled {
            let paint = stroke
                .paint
                .as_deref()
                .cloned()
                .unwrap_or(VectorPaint::Solid(stroke.color));
            out.push(VectorCommand::draw_line_end(
                placed,
                Some(paint),
                None,
                transform,
            ));
        } else {
            let arm = VectorStroke {
                color: stroke.color,
                width_emu: geometry.stroke_width.max(1.0) as i64,
                dash_segments: None,
                line_cap: LineCap::Round,
                line_join: LineJoin::Miter,
                paint: stroke.paint.clone(),
                miter_limit: stroke.effective_miter_limit(),
                head_end: None,
                tail_end: None,
                compound: CompoundLine::Sng,
            };
            out.push(VectorCommand::draw_line_end(
                placed,
                None,
                Some(arm),
                transform,
            ));
        }
    }
    out
}
