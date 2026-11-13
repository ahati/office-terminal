//! Preset-geometry parity tests.
//!
//! `tests/fixtures/presets/expected.json` is produced by an independent Python
//! oracle (`tests/fixtures/presets/gen_presets.py`) that re-implements the
//! reference evaluator from `evaluator.ts` and the `effAdj` substitution from
//! `geometry.ts:164-169`. It holds 186 presets × 3 box sizes × 11 adjust
//! variants = 6138 cases with every guide value.
//!
//! The first test replays every case through `evaluate_preset` and compares
//! guide-for-guide (and effective-adjust-for-effective-adjust) with the oracle.
//! The remaining tests derive their expectations by hand from the shipped
//! `presets.json` definitions and the reference path-executor rules
//! (`preset-geometry/path-executor.ts`), so they validate `build_preset`
//! without trusting the Rust evaluator.

use std::path::PathBuf;

use kitty_pptx_core::compiler::shapes::preset_eval::evaluate_preset;
use kitty_pptx_core::compiler::shapes::preset_exec::{build_preset, PathFill, PresetSubpath};
use kitty_pptx_core::contracts::{FillRule, PathSegment, VectorPath};
use serde_json::Value;

/// Path of a repository file relative to this crate.
fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_json(relative: &str) -> Value {
    let path = repo_file(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

/// The oracle rounds to 6 decimals, so the tolerance is the generator's own
/// comparison rule (`gen_presets.py::_close`).
fn assert_within(actual: f64, expected: f64, context: &str) {
    let tolerance = 1e-6 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

fn assert_xy(actual: (f64, f64), expected: (f64, f64), tolerance: f64, context: &str) {
    assert!(
        (actual.0 - expected.0).abs() <= tolerance && (actual.1 - expected.1).abs() <= tolerance,
        "{context}: expected ({}, {}), got ({}, {})",
        expected.0,
        expected.1,
        actual.0,
        actual.1
    );
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// On-curve points of a path in draw order; control points are ignored.
fn endpoints(path: &VectorPath) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y }
            | PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => points.push((*x as f64, *y as f64)),
            PathSegment::Close => {}
        }
    }
    points
}

/// Split a path into subpaths (one per `MoveTo`) using on-curve points.
fn split_subpaths(path: &VectorPath) -> Vec<Vec<(f64, f64)>> {
    let mut subpaths: Vec<Vec<(f64, f64)>> = Vec::new();
    for segment in &path.segments {
        match segment {
            PathSegment::MoveTo { x, y } => subpaths.push(vec![(*x as f64, *y as f64)]),
            PathSegment::LineTo { x, y }
            | PathSegment::QuadTo { x, y, .. }
            | PathSegment::CubicTo { x, y, .. } => {
                if let Some(current) = subpaths.last_mut() {
                    current.push((*x as f64, *y as f64));
                }
            }
            PathSegment::Close => {}
        }
    }
    subpaths
}

/// Shoelace area of the on-curve polygon; the sign is the winding direction.
fn signed_area(points: &[(f64, f64)]) -> f64 {
    let mut total = 0.0;
    for (index, (x0, y0)) in points.iter().enumerate() {
        let (x1, y1) = points[(index + 1) % points.len()];
        total += x0 * y1 - x1 * y0;
    }
    total / 2.0
}

fn segment_coords(segment: &PathSegment) -> Vec<f32> {
    match segment {
        PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => vec![*x, *y],
        PathSegment::QuadTo { x1, y1, x, y } => vec![*x1, *y1, *x, *y],
        PathSegment::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => vec![*x1, *y1, *x2, *y2, *x, *y],
        PathSegment::Close => Vec::new(),
    }
}

fn fill_from_json(mode: Option<&str>) -> PathFill {
    match mode {
        Some("none") => PathFill::None,
        Some("lighten") => PathFill::Lighten,
        Some("lightenLess") => PathFill::LightenLess,
        Some("darken") => PathFill::Darken,
        Some("darkenLess") => PathFill::DarkenLess,
        _ => PathFill::Norm,
    }
}

fn first_subpath(geometry: &str, w: f64, h: f64, adj: &[Option<f64>]) -> PresetSubpath {
    let mut subpaths =
        build_preset(geometry, w, h, adj).unwrap_or_else(|| panic!("{geometry}: no subpaths"));
    assert!(!subpaths.is_empty(), "{geometry}: empty subpath list");
    subpaths.remove(0)
}

// ---------------------------------------------------------------------------
// 1. Formula/guide parity against the independent oracle
// ---------------------------------------------------------------------------

#[test]
fn every_oracle_case_matches_guide_for_guide() {
    let doc = load_json("tests/fixtures/presets/expected.json");
    let cases = doc["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 6138, "expected.json case matrix");

    let mut guides_checked = 0usize;
    let mut non_finite_guides = 0usize;
    for case in cases {
        let geometry = case["geometry"].as_str().expect("geometry name");
        let w = case["w"].as_f64().expect("w");
        let h = case["h"].as_f64().expect("h");
        let adj: Vec<Option<f64>> = case["adj"]
            .as_array()
            .expect("adj vector")
            .iter()
            .map(Value::as_f64)
            .collect();

        let actual = evaluate_preset(geometry, w, h, &adj)
            .unwrap_or_else(|| panic!("{geometry}: evaluate_preset returned None"));

        let expected_guides = case["guides"].as_object().expect("guides object");
        assert_eq!(
            actual.gd.len(),
            expected_guides.len(),
            "{geometry} w={w} h={h} adj={adj:?}: guide count"
        );
        for (name, expected) in expected_guides {
            let value = *actual
                .gd
                .get(name)
                .unwrap_or_else(|| panic!("{geometry}: missing guide {name}"));
            let context = format!("{geometry} w={w} h={h} adj={adj:?} guide {name}");
            match expected.as_f64() {
                Some(expected) => assert_within(value, expected, &context),
                None => {
                    // `null` means "the reference yields a non-finite float"
                    // (e.g. `nosmoking` with a degenerate inner radius).
                    assert!(expected.is_null(), "{context}: not a number");
                    assert!(
                        !value.is_finite(),
                        "{context}: expected a non-finite value, got {value}"
                    );
                    non_finite_guides += 1;
                }
            }
            guides_checked += 1;
        }

        let expected_adjusts = case["effAdj"].as_object().expect("effAdj object");
        assert_eq!(
            actual.adj.len(),
            expected_adjusts.len(),
            "{geometry} w={w} h={h}: effective adjust count"
        );
        for (name, expected) in expected_adjusts {
            let value = *actual
                .adj
                .get(name)
                .unwrap_or_else(|| panic!("{geometry}: missing adjust {name}"));
            let context = format!("{geometry} w={w} h={h} adj={adj:?} effAdj {name}");
            match expected.as_f64() {
                Some(expected) => assert_within(value, expected, &context),
                None => assert!(
                    !value.is_finite(),
                    "{context}: expected a non-finite value, got {value}"
                ),
            }
        }
    }

    assert_eq!(guides_checked, 119_130, "guide comparisons");
    assert_eq!(non_finite_guides, 506, "non-finite guide values");
}

// ---------------------------------------------------------------------------
// 2. Geometry spot checks derived from presets.json
// ---------------------------------------------------------------------------

#[test]
fn hexagon_slant_follows_the_adjust() {
    // hexagon: adj = 25000, vf = 115470.
    //   maxAdj = 50000·w/ss = 50000, a = pin 0 adj maxAdj = 25000
    //   x1 = ss·a/100000 = 25, x2 = r − x1 = 75
    //   dy1 = hd2·vf/100000·sin(3600000) = 50·1.1547·sin 60° = 50
    //   y1 = vc − dy1 = 0, y2 = vc + dy1 = 100
    let (w, h) = (100.0, 100.0);
    let subpath = first_subpath("hexagon", w, h, &[]);
    let points = endpoints(&subpath.path);
    assert_eq!(points.len(), 6, "hexagon has six vertices");

    let x1 = w * 25_000.0 / 100_000.0;
    let y1 = h / 2.0 - (h / 2.0) * 1.15470 * 60.0_f64.to_radians().sin();
    assert_xy(points[0], (0.0, h / 2.0), 1e-3, "hexagon left point");
    assert_xy(points[1], (x1, y1), 1e-3, "hexagon top-left vertex");
    assert_xy(points[2], (w - x1, y1), 1e-3, "hexagon top-right vertex");
    assert_xy(points[3], (w, h / 2.0), 1e-3, "hexagon right point");
    assert_xy(points[4], (w - x1, h - y1), 1e-3, "hexagon bottom-right vertex");
    assert_xy(points[5], (x1, h - y1), 1e-3, "hexagon bottom-left vertex");

    // The slanted left edge drops from the mid-height point to the top vertex:
    // run = x1, rise = −dy1 (≈ h/2).
    let slant_run = points[1].0 - points[0].0;
    let slant_rise = points[1].1 - points[0].1;
    assert!((slant_run - x1).abs() <= 1e-3, "slant run {slant_run}");
    assert!(
        (slant_rise + (h / 2.0) * 1.15470 * 60.0_f64.to_radians().sin()).abs() <= 1e-3,
        "slant rise {slant_rise}"
    );
}

#[test]
fn chevron_notch_follows_the_adjust() {
    // chevron: adj = 50000; w = 200, h = 100 → ss = 100.
    //   maxAdj = 100000·w/ss = 200000, a = 50000
    //   x1 = ss·a/100000 = 50 (notch depth), x2 = r − x1 = 150
    // Path: top run to x2, tip at (r, vc), bottom run back to l, notch at
    // (x1, vc).
    let (w, h) = (200.0, 100.0);
    let subpath = first_subpath("chevron", w, h, &[]);
    let points = endpoints(&subpath.path);
    assert_eq!(points.len(), 6, "chevron has six vertices");

    let x1 = h * 50_000.0 / 100_000.0; // ss = h here
    let x2 = w - x1;
    assert_xy(points[0], (0.0, 0.0), 1e-6, "chevron top-left");
    assert_xy(points[1], (x2, 0.0), 1e-6, "chevron top run end");
    assert_xy(points[2], (w, h / 2.0), 1e-6, "chevron tip");
    assert_xy(points[3], (x2, h), 1e-6, "chevron bottom run end");
    assert_xy(points[4], (0.0, h), 1e-6, "chevron bottom-left");
    assert_xy(points[5], (x1, h / 2.0), 1e-6, "chevron notch");

    // The notch is the concave vertex: it sits x1 in from the left edge while
    // the top/bottom runs stop x2 out (x2 != x1 on a non-square box).
    assert!((points[5].0 - x1).abs() <= 1e-6);
    assert!((points[1].0 - x2).abs() <= 1e-6);
    assert!((x2 - x1).abs() > 1.0, "notch and tip must differ");
}

#[test]
fn right_arrow_head_follows_the_adjusts() {
    // `prst="rightArrow"` lowercases to rightarrow (geometry.ts:88). Defaults
    // adj1 = adj2 = 50000; w = h = 100 → ss = 100.
    //   maxAdj2 = 100000·w/ss = 100000; a1 = a2 = 50000
    //   dx1 = ss·a2/100000 = 50          (head length)
    //   x1 = r − dx1 = 50                (head base x)
    //   dy1 = h·a1/200000 = 25           (half shaft height)
    //   y1 = vc − dy1 = 25, y2 = 75
    //   dx2 = y1·dx1/hd2 = 25, x2 = x1 − dx2 = 25 (shaft/tail notch)
    let (w, h) = (100.0, 100.0);
    let subpath = first_subpath("rightarrow", w, h, &[]);
    let points = endpoints(&subpath.path);
    assert_eq!(points.len(), 7, "rightarrow has seven vertices");

    let head_length = w * 50_000.0 / 100_000.0;
    let base_x = w - head_length;
    let half_shaft = h * 50_000.0 / 200_000.0;
    let x2 = base_x - (h / 2.0 - half_shaft) * head_length / (h / 2.0);

    assert_xy(points[0], (0.0, h / 2.0 - half_shaft), 1e-6, "arrow shaft top-left");
    assert_xy(points[1], (base_x, h / 2.0 - half_shaft), 1e-6, "head base top");
    assert_xy(points[2], (base_x, 0.0), 1e-6, "head top corner");
    assert_xy(points[3], (w, h / 2.0), 1e-6, "head tip");
    assert_xy(points[4], (base_x, h), 1e-6, "head bottom corner");
    assert_xy(points[5], (base_x, h / 2.0 + half_shaft), 1e-6, "head base bottom");
    assert_xy(points[6], (0.0, h / 2.0 + half_shaft), 1e-6, "arrow shaft bottom-left");

    // Head geometry: tip on the right edge at mid-height, base spanning the
    // full height (both corners), length dx1.
    assert!((points[3].0 - w).abs() <= 1e-6 && (points[3].1 - h / 2.0).abs() <= 1e-6);
    assert!((points[2].0 - points[4].0).abs() <= 1e-6);
    assert!((w - points[2].0 - head_length).abs() <= 1e-6);
    // Tail notch (x2) sits inside the head base.
    assert!(points[0].0 < x2 && x2 < base_x, "tail notch {x2} vs base {base_x}");
}

#[test]
fn star8_inner_radius_follows_the_adjust() {
    // star8: adj = 37500 → a = 37500 (≤ 50000).
    //   iwd2 = wd2·a/50000 = 37.5, ihd2 = hd2·a/50000 = 37.5
    //   inner vertices at (hc ± iwd2·92388/100000, vc ± ihd2·38268/100000),
    //   i.e. on the circle of radius iwd2.
    let (w, h) = (100.0, 100.0);
    let subpath = first_subpath("star8", w, h, &[]);
    let points = endpoints(&subpath.path);
    let centre = (w / 2.0, h / 2.0);
    let inner_radius = (w / 2.0) * 37_500.0 / 50_000.0; // = 37.5
    let outer_radius = w / 2.0;

    // point 0 = left outer tip, point 1 = first inner vertex, point 2 = the
    // 45° outer tip (the 2700000 = 45° diagonal).
    assert!((distance(points[0], centre) - outer_radius).abs() <= 1e-6);
    let inner = distance(points[1], centre);
    assert!(
        (inner - inner_radius).abs() <= 1e-3,
        "inner radius: expected {inner_radius}, got {inner}"
    );
    let outer = distance(points[2], centre);
    assert!(
        (outer - outer_radius).abs() <= 1e-3,
        "outer radius: expected {outer_radius}, got {outer}"
    );
    assert!(inner < outer, "star points alternate inner/outer");
    assert_eq!(points.len(), 16, "star8 has eight points and eight valleys");
}

#[test]
fn donut_hole_winds_opposite_to_the_outer_ring() {
    // donut: adj = 25000, w = h = 100.
    //   a = 25000, dr = ss·a/100000 = 25
    //   iwd2 = wd2 − dr = 25, ihd2 = hd2 − dr = 25
    // One <a:path> with two subpaths: the outer ring's four <a:arcTo> sweep
    // +90° (cd4), the hole's four sweep −90° (ref path-executor.ts:118-125
    // keeps the sign), so the hole winds the other way and cuts out under the
    // nonzero fill rule.
    let (w, h) = (100.0, 100.0);
    let paths = build_preset("donut", w, h, &[]).expect("donut");
    assert_eq!(paths.len(), 1, "donut is a single <a:path>");

    let subpaths = split_subpaths(&paths[0].path);
    assert_eq!(subpaths.len(), 2, "outer ring and hole");

    let areas: Vec<f64> = subpaths.iter().map(|points| signed_area(points)).collect();
    assert!(
        areas[0].abs() > 1.0 && areas[1].abs() > 1.0,
        "degenerate ring: {areas:?}"
    );
    assert!(
        areas[0] * areas[1] < 0.0,
        "hole must wind opposite to the ring: {areas:?}"
    );
    assert!(
        areas[0].abs() > areas[1].abs(),
        "hole must be smaller than the ring: {areas:?}"
    );

    // The arc endpoints sit on the two radii: wd2 = 50 and iwd2 = 25.
    let centre = (w / 2.0, h / 2.0);
    let dr = h * 25_000.0 / 100_000.0;
    for (index, radius) in [(0usize, w / 2.0), (1usize, w / 2.0 - dr)] {
        for point in &subpaths[index] {
            let r = distance(*point, centre);
            assert!(
                (r - radius).abs() <= 0.01,
                "subpath {index}: point {point:?} is {r} from the centre, expected {radius}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Path execution details
// ---------------------------------------------------------------------------

#[test]
fn path_local_coordinates_scale_to_the_shape_box() {
    // flowchartdisplay declares w = h = 6 and starts
    //   m 0 3, l 1 0, l 5 0, a 1 3 3cd4 cd2, l 1 6, c
    // On a 60 × 120 box that is sx = 10, sy = 20 (ref path-executor.ts:47-49).
    let (w, h) = (60.0, 120.0);
    let subpath = first_subpath("flowchartdisplay", w, h, &[]);
    let points = endpoints(&subpath.path);

    assert_xy(points[0], (0.0, h / 2.0), 1e-6, "scaled left mid point");
    assert_xy(points[1], (w / 6.0, 0.0), 1e-6, "scaled top-left");
    assert_xy(points[2], (w * 5.0 / 6.0, 0.0), 1e-6, "scaled top-right");

    // The arc starts at (5·sx, 0) with start angle 3cd4 (270°) and a 180°
    // sweep, so its ellipse is centred at (5·sx, h/2) with radii (1·sx, 3·sy)
    // and ends at the bottom of that ellipse: (5·sx, h). The 1:3 radii prove
    // the radii scale per axis; the arc itself is decomposed to Béziers.
    let arc_end = subpath
        .path
        .segments
        .iter()
        .rev()
        .find_map(|segment| match segment {
            PathSegment::CubicTo { x, y, .. } => Some((*x as f64, *y as f64)),
            _ => None,
        })
        .expect("decomposed arc");
    assert_xy(arc_end, (w * 5.0 / 6.0, h), 1e-6, "arc end on the bottom edge");
    assert!(
        matches!(&subpath.path.segments[3], PathSegment::CubicTo { .. }),
        "the arc is decomposed into cubic Béziers"
    );
    let last = *points.last().expect("last point");
    assert_xy(last, (w / 6.0, h), 1e-6, "scaled bottom-left");
}

#[test]
fn whole_revolution_arcs_close_the_loop() {
    // cloudcallout path 1 is `m x23 yPos; a g26 g26 0 21600000; c`: a full
    // 360° arc. Without the whole-revolution split, `atan2` collapses
    // start == end and the bubble disappears (ref path-executor.ts:104-115).
    let (w, h) = (100.0, 100.0);
    let subpaths = build_preset("cloudcallout", w, h, &[]).expect("cloudcallout");
    let bubble = &subpaths[1];
    let points = endpoints(&bubble.path);
    let cubics = bubble
        .path
        .segments
        .iter()
        .filter(|segment| matches!(segment, PathSegment::CubicTo { .. }))
        .count();
    assert!(cubics >= 8, "expected a full circle, got {cubics} cubics");
    assert!(
        !bubble
            .path
            .segments
            .iter()
            .any(|segment| matches!(segment, PathSegment::LineTo { .. })),
        "a full-turn arc is curve segments only"
    );

    let start = points[0];
    let last = *points.last().expect("arc endpoints");
    assert!(
        distance(start, last) <= 1e-3,
        "full revolution must return to the start: {start:?} vs {last:?}"
    );
    let reached = points
        .iter()
        .map(|point| distance(*point, start))
        .fold(0.0_f64, f64::max);
    assert!(reached > 1.0, "the loop must have a radius, got {reached}");
}

// ---------------------------------------------------------------------------
// 4. Table coverage and per-path paint flags
// ---------------------------------------------------------------------------

#[test]
fn every_preset_in_the_table_executes() {
    let table = load_json("src/compiler/shapes/presets.json");
    let definitions = table.as_object().expect("preset table object");
    assert_eq!(definitions.len(), 186, "presets.json definitions");

    // 1 in × 0.75 in in EMU.
    let (w, h) = (914_400.0, 685_800.0);
    let mut executed = 0usize;
    for (name, definition) in definitions {
        let subpaths =
            build_preset(name, w, h, &[]).unwrap_or_else(|| panic!("{name}: build_preset None"));
        let declared = definition["paths"].as_array().expect("paths array");
        assert_eq!(subpaths.len(), declared.len(), "{name}: subpath count");

        for (index, (subpath, source)) in subpaths.iter().zip(declared).enumerate() {
            assert!(
                subpath.path.segments.len() >= 2,
                "{name} path {index}: too few segments"
            );
            assert!(
                matches!(&subpath.path.segments[0], PathSegment::MoveTo { .. }),
                "{name} path {index}: must start with MoveTo"
            );
            assert_eq!(
                subpath.path.fill_rule,
                FillRule::NonZero,
                "{name} path {index}: preset paths fill with the nonzero rule"
            );
            assert_eq!(
                subpath.fill,
                fill_from_json(source["fill"].as_str()),
                "{name} path {index}: fill mode"
            );
            assert_eq!(
                subpath.stroke,
                source["stroke"].as_bool().expect("stroke flag"),
                "{name} path {index}: stroke flag"
            );
            for segment in &subpath.path.segments {
                for value in segment_coords(segment) {
                    assert!(
                        value.is_finite(),
                        "{name} path {index}: non-finite coordinate {value}"
                    );
                }
            }
        }
        executed += 1;
    }
    assert_eq!(executed, 186, "every preset name resolved");
}

#[test]
fn unknown_preset_is_rejected() {
    assert!(build_preset("not-a-preset", 100.0, 100.0, &[]).is_none());
    // Ref `hasPreset` lowercases, so the table lookup is case-insensitive.
    let upper = build_preset("RoundRect", 100.0, 100.0, &[]).expect("RoundRect");
    let lower = build_preset("roundrect", 100.0, 100.0, &[]).expect("roundrect");
    assert_eq!(upper, lower);
}
