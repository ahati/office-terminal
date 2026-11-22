//! `<a:custGeom>` execution tests (ECMA-376 §20.1.9.8).
//!
//! These pin [`build_custom_geometry`] — in particular `<a:arcTo>`, whose
//! current point is the arc's *start*, not its centre. Expectations are derived
//! by hand from the reference builder
//! (`packages/core/src/shape/custGeom.ts:99-180`) and its arc math in
//! `packages/core/src/shape/preset-geometry/path-executor.ts:90-129`, so they do
//! not trust the implementation's own helpers.

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::shapes::{build_custom_geometry, build_shape_paths};
use office_terminal_core::contracts::{FillRule, PathSegment, VectorPath};
use pptx_model::PathCmd;

/// On-curve points of a path in draw order; control points are ignored.
fn endpoints(path: &VectorPath) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y }
            | PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => points.push((*x, *y)),
            PathSegment::Close => {}
        }
    }
    points
}

/// The pen position after the whole path, i.e. the last emitted on-curve point.
fn last_point(path: &VectorPath) -> (f32, f32) {
    *endpoints(path).last().expect("path must emit points")
}

fn cubic_count(path: &VectorPath) -> usize {
    path.segments
        .iter()
        .filter(|segment| matches!(segment, PathSegment::CubicTo { .. }))
        .count()
}

fn assert_close(actual: (f32, f32), expected: (f32, f32), tol: f32, what: &str) {
    assert!(
        (actual.0 - expected.0).abs() < tol && (actual.1 - expected.1).abs() < tol,
        "{what}: expected {expected:?}, got {actual:?}"
    );
}

// ---------------------------------------------------------------------------
// arcTo
// ---------------------------------------------------------------------------

#[test]
fn half_circle_arc_starts_at_the_pen_and_lands_on_the_opposite_point() {
    // Pen (0.5, 0) of a 2×2 box is (1, 0) in local units; wR = hR = 0.5 is a
    // radius of 1. stAng = -90° puts the pen on the circle's top, so the centre
    // must be back-solved to (1, 1) and a +180° sweep ends at (1, 2), i.e. the
    // normalized (0.5, 1). Interpreting the pen as the centre (the old bug)
    // would sweep a circle around (1, 0) and land nowhere near.
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.5, y: 0.0 },
        PathCmd::ArcTo {
            wr: 0.5,
            hr: 0.5,
            st_ang: -90.0,
            sw_ang: 180.0,
        },
    ]];
    let path = build_custom_geometry(&paths, 2.0, 2.0);

    assert!(cubic_count(&path) >= 4, "a half circle needs ≥4 cubics");
    // Every on-curve point after the moveTo lies on the unit circle about (1, 1).
    for (x, y) in &endpoints(&path)[1..] {
        let radius = ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt();
        assert!(
            (radius - 1.0).abs() < 1e-3,
            "({x}, {y}) is {radius} from the centre, expected 1"
        );
    }
    assert_close(last_point(&path), (1.0, 2.0), 1e-3, "arc end");
}

#[test]
fn whole_revolution_arc_closes_back_on_the_start_point() {
    // ref `path-executor.ts:104-115`: atan2 collapses a 360° sweep to
    // start == end, so whole turns are split off; otherwise the arc draws
    // nothing. The pen must end where it began.
    for sweep in [360.0f64, -360.0] {
        let paths = vec![vec![
            PathCmd::MoveTo { x: 0.5, y: 0.0 },
            PathCmd::ArcTo {
                wr: 0.5,
                hr: 0.5,
                st_ang: 0.0,
                sw_ang: sweep,
            },
        ]];
        let path = build_custom_geometry(&paths, 2.0, 2.0);
        assert!(
            cubic_count(&path) >= 8,
            "{sweep}°: expected a decomposed full circle, got {}",
            cubic_count(&path)
        );
        assert_close(
            last_point(&path),
            (1.0, 0.0),
            1e-3,
            &format!("{sweep}° sweep returns to the pen"),
        );
    }
}

#[test]
fn negative_sweep_turns_the_other_way() {
    // Pen (0.5, 0), radius 0.5, stAng 0 → centre (0, 0). A +90° sweep reaches
    // (0, 0.5); a -90° sweep reaches (0, -0.5). Same magnitude, opposite sign.
    let arc = |sweep: f64| {
        let paths = vec![vec![
            PathCmd::MoveTo { x: 0.5, y: 0.0 },
            PathCmd::ArcTo {
                wr: 0.5,
                hr: 0.5,
                st_ang: 0.0,
                sw_ang: sweep,
            },
        ]];
        build_custom_geometry(&paths, 1.0, 1.0)
    };

    assert_close(last_point(&arc(90.0)), (0.0, 0.5), 1e-4, "+90°");
    assert_close(last_point(&arc(-90.0)), (0.0, -0.5), 1e-4, "-90°");

    // Both quarter arcs are on-curve: every emitted end point is radius 0.5
    // from the origin, i.e. |p| = 0.5.
    for sweep in [90.0f64, -90.0] {
        for (x, y) in &endpoints(&arc(sweep))[1..] {
            assert!(
                ((x * x + y * y).sqrt() - 0.5).abs() < 1e-4,
                "{sweep}°: ({x}, {y}) is not on the circle"
            );
        }
    }
}

#[test]
fn arc_angles_are_visual_angles_on_a_non_circular_ellipse() {
    // A visual angle is the *direction* from the ellipse centre, so the point it
    // names is `d·(cos θ, sin θ)` with `d = 1/√(cos²θ/rx² + sin²θ/ry²)`, at the
    // parametric angle `atan2(rx·sin θ, ry·cos θ)`. Treating θ as parametric
    // instead would put the endpoint at a different place entirely.
    let (rx, ry) = (1.0f64, 0.5f64);
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.0, y: 0.0 },
        PathCmd::ArcTo {
            wr: rx,
            hr: ry,
            st_ang: 30.0,
            sw_ang: 120.0,
        },
    ]];
    let path = build_custom_geometry(&paths, 1.0, 1.0);

    // The pen sits on the 30° ray, so the centre is the opposite 210° point;
    // 30° and 150° are symmetric about the x axis and share the same radius `d`,
    // which makes the arc end at −2·d·cos30° on the x axis.
    let d = 1.0 / (0.75f64 / (rx * rx) + 0.25 / (ry * ry)).sqrt();
    let expected = (-2.0 * d * 30.0f64.to_radians().cos()) as f32;
    let end = last_point(&path);
    assert_close(end, (expected, 0.0), 1e-3, "visual 30° → 150°");
    // The 120° *visual* sweep spans only ~82° in the ellipse's parametric
    // angle here (49.1° → 130.9°), so the 45°-per-cubic split yields 2 cubics.
    assert_eq!(cubic_count(&path), 2);
}

#[test]
fn degenerate_radius_drops_the_arc_and_keeps_the_pen() {
    // ref `path-executor.ts:120`: a zero radius skips the command entirely.
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.5, y: 0.0 },
        PathCmd::ArcTo {
            wr: 0.0,
            hr: 0.5,
            st_ang: 0.0,
            sw_ang: 90.0,
        },
        PathCmd::LineTo { x: 1.0, y: 1.0 },
    ]];
    let path = build_custom_geometry(&paths, 10.0, 10.0);
    assert_eq!(
        path.segments,
        vec![
            PathSegment::MoveTo { x: 5.0, y: 0.0 },
            PathSegment::LineTo { x: 10.0, y: 10.0 },
        ]
    );
}

// ---------------------------------------------------------------------------
// Non-arc commands stay unchanged
// ---------------------------------------------------------------------------

#[test]
fn line_path_after_a_leading_move_to_is_unchanged() {
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.0, y: 0.0 },
        PathCmd::LineTo { x: 0.5, y: 1.0 },
        PathCmd::LineTo { x: 1.0, y: 0.0 },
        PathCmd::Close,
    ]];
    let path = build_custom_geometry(&paths, 100.0, 40.0);
    assert_eq!(
        path.segments,
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 40.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::Close,
        ]
    );
    assert_eq!(path.fill_rule, FillRule::NonZero);
}

#[test]
fn bezier_commands_scale_by_the_element_box() {
    let paths = vec![
        vec![
            PathCmd::MoveTo { x: 0.0, y: 0.0 },
            PathCmd::QuadBezTo {
                x1: 0.5,
                y1: 0.25,
                x: 1.0,
                y: 1.0,
            },
        ],
        vec![
            PathCmd::MoveTo { x: 0.0, y: 1.0 },
            PathCmd::CubicBezTo {
                x1: 0.25,
                y1: 0.0,
                x2: 0.75,
                y2: 1.0,
                x: 1.0,
                y: 0.0,
            },
            PathCmd::Close,
        ],
    ];
    let path = build_custom_geometry(&paths, 200.0, 100.0);
    assert_eq!(
        path.segments,
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::QuadTo {
                x1: 100.0,
                y1: 25.0,
                x: 200.0,
                y: 100.0,
            },
            // Every sub-path restarts the pen at the origin (ref custGeom.ts:106).
            PathSegment::MoveTo { x: 0.0, y: 100.0 },
            PathSegment::CubicTo {
                x1: 50.0,
                y1: 0.0,
                x2: 150.0,
                y2: 100.0,
                x: 200.0,
                y: 0.0,
            },
            PathSegment::Close,
        ]
    );
}

#[test]
fn dispatch_replays_cust_geom_without_warning() {
    // `build_shape_paths` must reach the same builder and keep the
    // norm-fill / stroke paint flags for a custom geometry.
    let paths = vec![vec![
        PathCmd::MoveTo { x: 0.5, y: 0.0 },
        PathCmd::ArcTo {
            wr: 0.5,
            hr: 0.5,
            st_ang: -90.0,
            sw_ang: 180.0,
        },
    ]];
    let (tx, rx) = unbounded();
    let subpaths = build_shape_paths("custGeom", 2.0, 2.0, &[], Some(&paths), &tx);
    assert!(rx.try_recv().is_err(), "custGeom must not warn");
    assert_eq!(subpaths.len(), 1);
    assert_close(last_point(&subpaths[0].path), (1.0, 2.0), 1e-3, "dispatched arc");
}
