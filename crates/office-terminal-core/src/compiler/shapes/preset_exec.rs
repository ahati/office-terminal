//! ECMA-376 `<a:path>` execution for preset geometries.
//!
//! Port of the reference executor
//! (`packages/core/src/shape/preset-geometry/path-executor.ts:37-130`) on top of
//! [`super::preset_eval`]. Each `<a:path>` of a definition is replayed into one
//! [`PresetSubpath`], preserving authored order and the per-path paint flags the
//! reference reads while painting (`index.ts:44-99`): `fill` is a modifier on the
//! shape fill and `stroke` decides whether the path takes the shape outline.
//!
//! Coordinates are emitted in the caller's local space. `w`/`h` are the shape
//! extent; a path that declares its own authoring `w`/`h` scales its points by
//! `shape / authoring`, otherwise by 1 (ref `path-executor.ts:47-49`). Angle
//! tokens (`<a:arcTo>` `stAng`/`swAng`) are 60 000ths of a degree
//! (`evaluator.ts:15-16`), and — like the reference — they are interpreted as
//! *visual* angles, converted to the parametric angle of the ellipse with
//! `atan2(rx·sin, ry·cos)` (ref `path-executor.ts:99-104`).
//!
//! The reference draws arcs with `CanvasRenderingContext2D.ellipse`. The vector
//! IR has no arc segment, so the same parametric arc (same centre, radii, start
//! and end angle) is decomposed into cubic Béziers with
//! [`super::arc::decompose_arc_to_cubic`], the helper the custom-geometry
//! compiler already uses.

use std::collections::HashMap;

use crate::contracts::{FillRule, PathSegment, VectorPath};

use super::arc::decompose_arc_to_cubic;
use super::preset_data::{preset_def, PresetPathDef};
use super::preset_eval::{preset_env, resolve_token};

/// 60 000ths of a degree per full revolution (2π radians).
/// ref `path-executor.ts:56`
const DEG60K_TO_RAD: f64 = std::f64::consts::TAU / 21_600_000.0;

/// One full revolution in radians.
const TWO_PI: f64 = std::f64::consts::TAU;

/// Radius below which the reference skips an `<a:arcTo>` entirely.
/// ref `path-executor.ts:120`
const ARC_EPSILON: f64 = 1e-6;

/// `<a:path fill>` (ST_PathFillMode, ECMA-376 §20.1.10.37).
///
/// The modifier does not change the geometry; the painter fills the subpath
/// with the shape's fill and then overlays a translucent tint
/// (`geometry.ts:391-399`). `Norm` — including a missing attribute — paints no
/// overlay; `None` skips the fill altogether and only strokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathFill {
    /// `norm` (or a missing attribute): plain shape fill.
    Norm,
    /// `none`: no fill, stroke only.
    None,
    /// `lighten`: white overlay at alpha 102/255.
    Lighten,
    /// `lightenLess`: white overlay at alpha 50/255.
    LightenLess,
    /// `darken`: black overlay at alpha 102/255.
    Darken,
    /// `darkenLess`: black overlay at alpha 50/255.
    DarkenLess,
}

impl PathFill {
    /// Map an `<a:path fill>` value.
    ///
    /// A missing value means `norm`; an unrecognised value also falls back to
    /// `norm`, because the reference only special-cases `"none"` when deciding
    /// whether to paint (ref `index.ts:50-52`) and
    /// `pathFillModeOverlay` returns `null` for anything else
    /// (`geometry.ts:391-399`).
    pub fn from_mode(mode: Option<&str>) -> Self {
        match mode {
            Some("none") => Self::None,
            Some("lighten") => Self::Lighten,
            Some("lightenLess") => Self::LightenLess,
            Some("darken") => Self::Darken,
            Some("darkenLess") => Self::DarkenLess,
            _ => Self::Norm,
        }
    }

    /// Whether the reference paints this path with the shape fill.
    /// ref `index.ts:50` (`fillMode !== 'none'`).
    pub fn is_filled(self) -> bool {
        self != Self::None
    }
}

/// One executed `<a:path>`: geometry plus its authored paint flags.
///
/// ref `resolvePresetGeometryPaths` (`geometry.ts:276-286`), which returns
/// `{ path, fill, stroke }` per path.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetSubpath {
    /// The path in local space, with arcs decomposed to cubic Béziers.
    pub path: VectorPath,
    /// `<a:path fill>` modifier; see [`PathFill`].
    pub fill: PathFill,
    /// `<a:path stroke>`: whether the shape outline is stroked along this path.
    pub stroke: bool,
}

/// Execute every `<a:path>` of a preset geometry in local space.
///
/// `w` and `h` are the shape extent in the caller's units (EMU for the compile
/// pipeline), `adj` the raw adjust values (1/100 000 units, `None` for the
/// preset default). One [`PresetSubpath`] is returned per authored `<a:path>`,
/// in declaration order.
///
/// Returns `None` for an unknown preset or a command token the evaluator cannot
/// resolve — the reference throws in that case (`evaluator.ts:113-118`).
pub fn build_preset(
    geometry: &str,
    w: f64,
    h: f64,
    adj: &[Option<f64>],
) -> Option<Vec<PresetSubpath>> {
    let def = preset_def(geometry)?;
    let env = preset_env(geometry, w, h, adj)?;

    let mut subpaths = Vec::with_capacity(def.paths.len());
    for source in &def.paths {
        subpaths.push(execute_path(source, w, h, &env)?);
    }
    Some(subpaths)
}

/// Replay one `<a:path>` into a [`PresetSubpath`].
fn execute_path(
    source: &PresetPathDef,
    w: f64,
    h: f64,
    env: &HashMap<&'static str, f64>,
) -> Option<PresetSubpath> {
    // ref `path-executor.ts:47-49`: flat 1:1 mapping when the path declares no
    // authoring size, otherwise scale to the shape box.
    let mut pen = Pen {
        scale_x: source.w.map_or(1.0, |path_w| w / path_w),
        scale_y: source.h.map_or(1.0, |path_h| h / path_h),
        x: 0.0,
        y: 0.0,
        segments: Vec::new(),
    };

    for cmd in &source.cmds {
        pen.command(cmd, env)?;
    }

    Some(PresetSubpath {
        path: VectorPath::new(pen.segments, FillRule::NonZero),
        fill: PathFill::from_mode(source.fill.as_deref()),
        stroke: source.stroke,
    })
}

/// Current point and accumulated segments of one `<a:path>`.
///
/// The pen is tracked in the caller's space (not in path-local space) because
/// `<a:arcTo>` back-solves the ellipse centre from it, exactly like the
/// reference does (`path-executor.ts:44-45`, `:118-119`).
struct Pen {
    scale_x: f64,
    scale_y: f64,
    x: f64,
    y: f64,
    segments: Vec<PathSegment>,
}

impl Pen {
    /// Apply one command. ref `path-executor.ts:51-129`.
    fn command(
        &mut self,
        cmd: &[String],
        env: &HashMap<&'static str, f64>,
    ) -> Option<()> {
        match cmd.first()?.as_str() {
            "m" => {
                let (x, y) = self.point(cmd, 1, env)?;
                self.segments.push(PathSegment::MoveTo {
                    x: x as f32,
                    y: y as f32,
                });
                self.move_pen(x, y);
            }
            "l" => {
                let (x, y) = self.point(cmd, 1, env)?;
                self.segments.push(PathSegment::LineTo {
                    x: x as f32,
                    y: y as f32,
                });
                self.move_pen(x, y);
            }
            "C" => {
                let (x1, y1) = self.point(cmd, 1, env)?;
                let (x2, y2) = self.point(cmd, 3, env)?;
                let (x, y) = self.point(cmd, 5, env)?;
                self.segments.push(PathSegment::CubicTo {
                    x1: x1 as f32,
                    y1: y1 as f32,
                    x2: x2 as f32,
                    y2: y2 as f32,
                    x: x as f32,
                    y: y as f32,
                });
                self.move_pen(x, y);
            }
            "Q" => {
                let (x1, y1) = self.point(cmd, 1, env)?;
                let (x, y) = self.point(cmd, 3, env)?;
                self.segments.push(PathSegment::QuadTo {
                    x1: x1 as f32,
                    y1: y1 as f32,
                    x: x as f32,
                    y: y as f32,
                });
                self.move_pen(x, y);
            }
            "a" => self.arc(cmd, env)?,
            "c" => self.segments.push(PathSegment::Close),
            // The reference's switch ignores commands it does not know; treat
            // one as malformed data instead, since the table only ships
            // m/l/C/Q/a/c (ref `path-executor.ts:51-129`).
            _ => return None,
        }
        Some(())
    }

    /// Resolve the two coordinate tokens at `base` and `base + 1`.
    fn point(
        &self,
        cmd: &[String],
        base: usize,
        env: &HashMap<&'static str, f64>,
    ) -> Option<(f64, f64)> {
        Some((
            resolve_token(cmd.get(base)?, env)? * self.scale_x,
            resolve_token(cmd.get(base + 1)?, env)? * self.scale_y,
        ))
    }

    /// `<a:arcTo wR hR stAng swAng>`: an elliptical arc starting at the pen.
    ///
    /// ref `path-executor.ts:96-126`. The radii are converted to visual space in
    /// *path-local* coordinates (where the OOXML angle is defined), while the
    /// scaled radii place the arc in the caller's space. Full revolutions are
    /// split into whole turns plus a remainder, because `atan2` collapses a
    /// 360° sweep to `start == end` and would draw nothing.
    fn arc(&mut self, cmd: &[String], env: &HashMap<&'static str, f64>) -> Option<()> {
        let w_radius_local = token(cmd, 1, env)?;
        let h_radius_local = token(cmd, 2, env)?;
        let w_radius = w_radius_local * self.scale_x;
        let h_radius = h_radius_local * self.scale_y;
        let start_angle = token(cmd, 3, env)? * DEG60K_TO_RAD;
        let sweep = token(cmd, 4, env)? * DEG60K_TO_RAD;

        // ref `path-executor.ts:99-104`: visual angle -> parametric angle.
        let to_parametric = |visual: f64| {
            (w_radius_local * visual.sin()).atan2(h_radius_local * visual.cos())
        };
        let start = to_parametric(start_angle);
        let full_revolutions = (sweep / TWO_PI).trunc();
        let remainder = sweep - full_revolutions * TWO_PI;
        let mut delta = to_parametric(start_angle + remainder) - start;
        if remainder > 0.0 && delta < 0.0 {
            delta += TWO_PI;
        } else if remainder < 0.0 && delta > 0.0 {
            delta -= TWO_PI;
        }
        let end = start + delta + full_revolutions * TWO_PI;

        // ref `path-executor.ts:118-119`: back-solve the centre from the pen.
        let center_x = self.x - w_radius * start.cos();
        let center_y = self.y - h_radius * start.sin();

        // ref `path-executor.ts:120-125`: a degenerate radius drops the whole
        // command, leaving the pen untouched. The drawn radii are the absolute
        // values (canvas `ellipse` takes unsigned radii) while the pen advance
        // uses the signed ones, exactly as the reference computes them.
        if w_radius.abs() > ARC_EPSILON && h_radius.abs() > ARC_EPSILON {
            let mut segments = Vec::new();
            decompose_arc_to_cubic(
                center_x as f32,
                center_y as f32,
                w_radius.abs() as f32,
                h_radius.abs() as f32,
                start.to_degrees() as f32,
                (end - start).to_degrees() as f32,
                &mut segments,
            );
            self.segments.extend(segments);
            self.x = center_x + w_radius * end.cos();
            self.y = center_y + h_radius * end.sin();
        }
        Some(())
    }

    /// Advance the pen to an absolute point.
    fn move_pen(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }
}

/// Resolve one command token through the evaluator environment
/// (ref `evaluator.ts:113-118`).
fn token(cmd: &[String], index: usize, env: &HashMap<&'static str, f64>) -> Option<f64> {
    resolve_token(cmd.get(index)?, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_preset_and_unknown_token_are_rejected() {
        assert!(build_preset("not-a-preset", 100.0, 100.0, &[]).is_none());

        // A guide the preset never defines cannot be resolved, so the whole
        // path fails instead of silently drawing at the origin.
        let env = HashMap::new();
        let mut pen = Pen {
            scale_x: 1.0,
            scale_y: 1.0,
            x: 0.0,
            y: 0.0,
            segments: Vec::new(),
        };
        let cmd = vec!["l".to_string(), "missing".to_string(), "0".to_string()];
        assert!(pen.command(&cmd, &env).is_none());
        assert!(pen.segments.is_empty());
    }

    #[test]
    fn fill_modes_map_like_the_reference() {
        assert_eq!(PathFill::from_mode(None), PathFill::Norm);
        assert_eq!(PathFill::from_mode(Some("norm")), PathFill::Norm);
        assert_eq!(PathFill::from_mode(Some("none")), PathFill::None);
        assert_eq!(PathFill::from_mode(Some("lighten")), PathFill::Lighten);
        assert_eq!(
            PathFill::from_mode(Some("lightenLess")),
            PathFill::LightenLess
        );
        assert_eq!(PathFill::from_mode(Some("darken")), PathFill::Darken);
        assert_eq!(
            PathFill::from_mode(Some("darkenLess")),
            PathFill::DarkenLess
        );
        assert_eq!(PathFill::from_mode(Some("bogus")), PathFill::Norm);
        assert!(!PathFill::None.is_filled());
        assert!(PathFill::Norm.is_filled());
    }

    #[test]
    fn sun_full_turn_arc_emits_curves() {
        // `sun` ends with a full-turn arc; without the whole-revolution split
        // the sweep collapses to start == end and no segments are emitted.
        let paths = build_preset("sun", 100.0, 100.0, &[]).expect("sun");
        let cubic = paths
            .iter()
            .flat_map(|subpath| subpath.path.segments.iter())
            .filter(|segment| matches!(segment, PathSegment::CubicTo { .. }))
            .count();
        assert!(cubic >= 8, "expected a decomposed full circle, got {cubic}");
    }

    /// Execute one synthetic `<a:arcTo>` from a pen at the origin and return the
    /// advanced pen (the arc's endpoint) plus the emitted segments.
    fn arc_from_origin(rx: &str, ry: &str, start: &str, sweep: &str) -> (f64, f64, Vec<PathSegment>) {
        let mut pen = Pen {
            scale_x: 1.0,
            scale_y: 1.0,
            x: 0.0,
            y: 0.0,
            segments: Vec::new(),
        };
        let cmd: Vec<String> = ["a", rx, ry, start, sweep]
            .iter()
            .map(|token| token.to_string())
            .collect();
        pen.command(&cmd, &HashMap::new()).expect("arc command");
        (pen.x, pen.y, pen.segments)
    }

    #[test]
    fn arc_interprets_angles_as_visual_angles() {
        // ref `path-executor.ts:99-104`: the parametric angle of a visual angle
        // is atan2(rx·sin, ry·cos), so a 100° sweep on a 100 × 50 ellipse ends
        // at the visual angle 100°, not at the parametric angle 100°.
        let (rx, ry) = (100.0_f64, 50.0_f64);
        let to_parametric = |visual: f64| (rx * visual.sin()).atan2(ry * visual.cos());
        let t0 = to_parametric(0.0);

        for sweep_deg in [100.0_f64, -100.0] {
            let (end_x, end_y, segments) = arc_from_origin(
                "100",
                "50",
                "0",
                &(sweep_deg * 60_000.0).to_string(),
            );
            assert!(!segments.is_empty(), "{sweep_deg}° sweep should emit curves");
            let t1 = to_parametric(sweep_deg.to_radians());
            // The pen starts at the origin, so the endpoint is the difference
            // between the end point and the start point on the ellipse.
            let expected_x = rx * t1.cos() - rx * t0.cos();
            let expected_y = ry * t1.sin() - ry * t0.sin();
            assert!(
                (end_x - expected_x).abs() < 1e-2 && (end_y - expected_y).abs() < 1e-2,
                "{sweep_deg}°: expected ({expected_x}, {expected_y}), got ({end_x}, {end_y})"
            );
        }
    }

    #[test]
    fn arc_splits_whole_revolutions() {
        // A ±360° sweep must be split into whole turns plus a remainder, else
        // `atan2` collapses it and nothing is drawn (ref path-executor.ts:104-115).
        for sweep in ["21600000", "-21600000"] {
            let (end_x, end_y, segments) = arc_from_origin("50", "50", "0", sweep);
            let cubics = segments
                .iter()
                .filter(|segment| matches!(segment, PathSegment::CubicTo { .. }))
                .count();
            assert!(cubics >= 8, "{sweep}: expected a full circle, got {cubics}");
            assert!(
                end_x.abs() < 1e-3 && end_y.abs() < 1e-3,
                "{sweep}: the pen must return to the start, got ({end_x}, {end_y})"
            );
        }

        // ref `path-executor.ts:120`: a degenerate radius drops the command and
        // leaves the pen untouched.
        let (end_x, end_y, segments) = arc_from_origin("0", "50", "0", "5400000");
        assert!(segments.is_empty());
        assert!(end_x.abs() < f64::EPSILON && end_y.abs() < f64::EPSILON);
    }
}
