//! Custom-geometry (`<a:custGeom>`) execution, ECMA-376 §20.1.9.8.
//!
//! Port of the reference custom-geometry path builder
//! (`packages/core/src/shape/custGeom.ts:99-180`): each authored `<a:path>` is
//! replayed in order into one `VectorPath` with the non-zero fill rule, and the
//! pen is tracked so `<a:arcTo>` can back-solve its ellipse centre.
//!
//! Coordinates arrive unit-normalized (the parser divides every authored point
//! by its `<a:path>` `w`/`h`, `pptx-parser/src/fill.rs`), so a point `(x, y)` is
//! mapped to `(x * w, y * h)` of the element box. Radius and angle fields are
//! normalized the same way: `wr`/`hr` are fractions of the path box and the
//! angles are already degrees (`pptx-parser/src/fill.rs:745-750`).
//!
//! `<a:arcTo>` is *not* a full ellipse: the current point is the arc's start
//! point, and the centre is back-solved from the pen and `stAng`. The angular
//! math is the preset executor's
//! ([`super::preset_exec`], ref `path-executor.ts:96-126`): OOXML angles are
//! *visual* angles, converted to the ellipse's parametric angle with
//! `atan2(wR·sin, hR·cos)` in path-local space, whole revolutions split off so a
//! 360° sweep cannot collapse to `start == end`, and the arc decomposed into
//! cubic Béziers because the vector IR has no arc segment.

use pptx_model::PathCmd;

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;

/// One full revolution in radians.
const TWO_PI: f64 = std::f64::consts::TAU;

/// Radius below which the reference skips an `<a:arcTo>` entirely.
/// ref `path-executor.ts:120`
const ARC_EPSILON: f64 = 1e-6;

/// Replay every authored `<a:path>` of a `<a:custGeom>` into one path.
///
/// `w` and `h` are the element extent in the caller's units (EMU for the
/// compile pipeline); each command's normalized coordinates are scaled by them.
/// The result always uses [`FillRule::NonZero`], matching the reference canvas
/// path (which never sets a fill rule) and the previous behaviour.
pub fn build_custom_geometry(paths: &[Vec<PathCmd>], w: f32, h: f32) -> VectorPath {
    let mut segments = Vec::new();
    for path in paths {
        segments.extend(build_custom_geometry_path(path, w, h).segments);
    }
    VectorPath::new(segments, FillRule::NonZero)
}

/// Replay one authored `<a:path>` into its own vector path.
pub fn build_custom_geometry_path(path_cmds: &[PathCmd], w: f32, h: f32) -> VectorPath {
    let mut segments = Vec::new();
    {
        // ref `custGeom.ts:106`: the pen starts at the origin of every sub-path
        // (it is only meaningful once a `moveTo` has run, but an arc that comes
        // first must still back-solve from a defined origin).
        let mut pen = (0.0f32, 0.0f32);

        for cmd in path_cmds {
            match cmd {
                PathCmd::MoveTo { x, y } => {
                    let pt = scaled(*x, *y, w, h);
                    segments.push(PathSegment::MoveTo { x: pt.0, y: pt.1 });
                    pen = pt;
                }
                PathCmd::LineTo { x, y } => {
                    let pt = scaled(*x, *y, w, h);
                    segments.push(PathSegment::LineTo { x: pt.0, y: pt.1 });
                    pen = pt;
                }
                PathCmd::QuadBezTo { x1, y1, x, y } => {
                    let pt = scaled(*x, *y, w, h);
                    segments.push(PathSegment::QuadTo {
                        x1: *x1 as f32 * w,
                        y1: *y1 as f32 * h,
                        x: pt.0,
                        y: pt.1,
                    });
                    pen = pt;
                }
                PathCmd::CubicBezTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    let pt = scaled(*x, *y, w, h);
                    segments.push(PathSegment::CubicTo {
                        x1: *x1 as f32 * w,
                        y1: *y1 as f32 * h,
                        x2: *x2 as f32 * w,
                        y2: *y2 as f32 * h,
                        x: pt.0,
                        y: pt.1,
                    });
                    pen = pt;
                }
                PathCmd::ArcTo {
                    wr,
                    hr,
                    st_ang,
                    sw_ang,
                } => arc_to(&mut pen, *wr, *hr, *st_ang, *sw_ang, w, h, &mut segments),
                PathCmd::Close => segments.push(PathSegment::Close),
            }
        }
    }

    VectorPath::new(segments, FillRule::NonZero)
}

/// Map one normalized point into the element box.
fn scaled(x: f64, y: f64, w: f32, h: f32) -> (f32, f32) {
    (x as f32 * w, y as f32 * h)
}

/// `<a:arcTo wR hR stAng swAng>`: an elliptical arc whose start is the pen.
///
/// ref `custGeom.ts:149-165` for the centre back-solve and
/// `path-executor.ts:96-126` for the visual→parametric conversion, the
/// whole-revolution split and the degenerate-radius guard.
#[allow(clippy::too_many_arguments)] // pen + 4 arc fields + box + sink, as authored
fn arc_to(
    pen: &mut (f32, f32),
    w_radius_local: f64,
    h_radius_local: f64,
    st_ang_deg: f64,
    sw_ang_deg: f64,
    w: f32,
    h: f32,
    segments: &mut Vec<PathSegment>,
) {
    // Path-local radii stay normalized (the OOXML visual angle is defined in the
    // path's own coordinate system, where the box is 1×1); the box-scaled radii
    // only place the arc in the caller's space.
    // ref `path-executor.ts:90-95`
    let w_radius = w_radius_local * f64::from(w);
    let h_radius = h_radius_local * f64::from(h);
    let start_angle = st_ang_deg.to_radians();
    let sweep = sw_ang_deg.to_radians();

    // ref `path-executor.ts:99-104`: a visual angle is the point
    // (wR·cos θ, hR·sin θ), whose parametric angle is `atan2(wR·sin, hR·cos)`.
    let to_parametric = |visual: f64| {
        (w_radius_local * visual.sin()).atan2(h_radius_local * visual.cos())
    };
    let start = to_parametric(start_angle);
    // `atan2` collapses a full revolution onto its start point, so split the
    // sweep into whole turns plus a remainder whose sign is preserved.
    // ref `path-executor.ts:104-115`
    let full_revolutions = (sweep / TWO_PI).trunc();
    let remainder = sweep - full_revolutions * TWO_PI;
    let mut delta = to_parametric(start_angle + remainder) - start;
    if remainder > 0.0 && delta < 0.0 {
        delta += TWO_PI;
    } else if remainder < 0.0 && delta > 0.0 {
        delta -= TWO_PI;
    }
    let end = start + delta + full_revolutions * TWO_PI;

    // ref `path-executor.ts:118-119`: back-solve the centre from the pen point,
    // which is the arc's *start*, not its centre.
    let center_x = f64::from(pen.0) - w_radius * start.cos();
    let center_y = f64::from(pen.1) - h_radius * start.sin();

    // ref `path-executor.ts:120-125`: a degenerate radius drops the command and
    // leaves the pen untouched. The drawn radii are unsigned (canvas `ellipse`
    // takes absolute radii) while the pen advance uses the signed ones.
    if w_radius.abs() <= ARC_EPSILON || h_radius.abs() <= ARC_EPSILON {
        return;
    }

    decompose_arc_to_cubic(
        center_x as f32,
        center_y as f32,
        w_radius.abs() as f32,
        h_radius.abs() as f32,
        start.to_degrees() as f32,
        (end - start).to_degrees() as f32,
        segments,
    );

    *pen = (
        (center_x + w_radius * end.cos()) as f32,
        (center_y + h_radius * end.sin()) as f32,
    );
}
