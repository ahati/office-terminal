//! Ports of `line-end-shaft-trim.test.ts` (batch `shapes-b`): the
//! `a:headEnd` / `a:tailEnd` retraction and the `a:ln@cmpd` compound rails.
//!
//! The reference re-strokes a connector leader from its vertices, retracted by
//! `lineEndRetract` (`arrow-geometry.ts:128-139`), and paints the decorations at
//! the *untrimmed* ends (ref renderer.ts:4425-4535). The IR is vector-first, so
//! `compiler::style::stroke::compile_stroked_paths` emits the same resolved
//! geometry into the display list: the trimmed shaft first, then the tail
//! decoration, then the head decoration. These ports assert that geometry and its
//! painted twin (raster + SVG).
//!
//! Loaded as a module of the `parity_shapes_b_tests` target.

use super::*;

// ── line-end-shaft-trim.test.ts / compound lines ────────────────────────────
//
// The reference re-strokes a connector leader from its vertices, retracted by
// `lineEndRetract` (`arrow-geometry.ts:128-139`), and paints the decorations at
// the *untrimmed* ends (ref renderer.ts:4425-4535). The IR is vector-first, so
// `compiler::style::stroke::compile_stroked_paths` emits the same resolved
// geometry into the display list: the trimmed shaft first, then the tail
// decoration, then the head decoration. These ports assert that geometry and the
// painted twin.

/// 9 pt line (`LINE_EMU` in the reference), one px per 9525 EMU.
const LINE_EMU: i64 = 114_300;
/// `6 / sin(atan2(2.5, 4.5))` for a 9 pt `lg`/`lg` open arrow.
fn vertex_px() -> f32 {
    6.0 / (2.5f32).atan2(4.5).sin()
}

fn arrow() -> serde_json::Value {
    serde_json::json!({ "type": "arrow", "w": "lg", "len": "lg" })
}

/// A connector leader fixture at (100, 100) px with the authored stroke ends.
fn connector_leader(geometry: &str, width_px: f32, height_px: f32, stroke: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "shape",
        "x": 100 * 9525,
        "y": 100 * 9525,
        "width": (width_px * 9525.0) as i64,
        "height": (height_px * 9525.0) as i64,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": geometry,
        "fill": null,
        "stroke": stroke,
        "textBody": null,
        "defaultTextColor": null,
        "custGeom": null,
    })
}

/// Absolute device-space point of a segment (shape origin + local EMU).
fn abs_px(origin_px: (f32, f32), x: f32, y: f32) -> (f32, f32) {
    (origin_px.0 + x / 9525.0, origin_px.1 + y / 9525.0)
}

/// The one `a:tailEnd`/`a:headEnd` decoration in the display list.
fn line_end_path(slide: &VectorSlide) -> &VectorPath {
    slide
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawLineEnd { path, .. } => Some(path),
            _ => None,
        })
        .expect("one line-end decoration")
}

#[test]
fn connector_tail_end_trims_the_shaft_and_paints_the_open_arrow() {
    // ref line-end-shaft-trim.test.ts:23-27 — a 300 x 200 px `line` from
    // (100, 100) to (400, 300) with an `lg` open arrow on its tail; the shaft
    // stops 12.35 px short of the tip.
    let element = connector_leader(
        "line",
        300.0,
        200.0,
        serde_json::json!({ "color": "000000", "width": LINE_EMU, "tailEnd": arrow() }),
    );
    let slide = compile_slide(vec![element], 960, 720);
    let tip = (400.0f32, 300.0f32);

    let stroked: Vec<&VectorPath> = slide
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path, stroke: Some(_), ..
            } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(stroked.len(), 1, "the shaft is the only stroked path");

    // 1. The shaft is retracted to the arms' vertex, along the line.
    let shaft = stroked[0];
    let (sx, sy) = match shaft.segments.last() {
        Some(PathSegment::LineTo { x, y }) => abs_px((100.0, 100.0), *x, *y),
        other => panic!("shaft must be a line, got {other:?}"),
    };
    let distance = ((sx - tip.0).powi(2) + (sy - tip.1).powi(2)).sqrt();
    assert!(
        (distance - vertex_px()).abs() < 0.05,
        "shaft stops at the vertex, got ({sx}, {sy}) = {distance} px from the tip"
    );
    // It stays on the leader: the retraction does not bend the run.
    assert!(
        ((sy - 100.0) / (sx - 100.0) - 200.0 / 300.0).abs() < 1e-3,
        "shaft keeps the leader slope, got ({sx}, {sy})"
    );
    assert_eq!(
        shaft.segments.len(),
        2,
        "the retraction splits the run, it does not add segments"
    );

    // 2. The decoration is authored on the *untrimmed* tip. An open arrow puts
    //    its arms' vertex `(lw/2)/sin(θ)` behind the tip, so the outer miter
    //    lands exactly on it — the same distance the shaft is pulled back. The
    //    decoration is its own `DrawLineEnd` command, painted in the tip frame
    //    the way the reference's translated `drawArrowHead` does
    //    (ref renderer.ts:4513-4535).
    let arrow_path = line_end_path(&slide);
    let points: Vec<(f32, f32)> = arrow_path
        .segments
        .iter()
        .filter_map(|segment| match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => Some(abs_px((100.0, 100.0), *x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 3, "an open arrow is two arms: {points:?}");
    // The leader runs (100, 100) → (400, 300) px, so its unit vector is (3, 2)/√13.
    let len = (300.0f32 * 300.0 + 200.0 * 200.0).sqrt();
    let unit = (300.0 / len, 200.0 / len);
    let across = (-unit.1, unit.0);
    let vertex = (tip.0 - unit.0 * vertex_px(), tip.1 - unit.1 * vertex_px());
    assert!(
        (points[1].0 - vertex.0).abs() < 0.05 && (points[1].1 - vertex.1).abs() < 0.05,
        "the arms' vertex sits one retraction behind the tip, got {:?}",
        points[1]
    );
    // Arms: 4.5 e along and 2.5 e across the line, behind the vertex.
    let along_px = 4.5 * LINE_EMU as f32 / 9525.0;
    let across_px = 2.5 * LINE_EMU as f32 / 9525.0;
    for (arm, sign) in [(points[0], -1.0), (points[2], 1.0)] {
        let expect = (
            vertex.0 - unit.0 * along_px + sign * across.0 * across_px,
            vertex.1 - unit.1 * along_px + sign * across.1 * across_px,
        );
        assert!(
            (arm.0 - expect.0).abs() < 0.05 && (arm.1 - expect.1).abs() < 0.05,
            "arm {sign}: got {arm:?}, want {expect:?}"
        );
    }

    // 3. Raster twin: the arrowhead inks the tip, nothing paints past it.
    let img = raster(&slide, 960, 720);
    let inked_near_tip = (396..400).any(|x| (297..301).any(|y| at(&img, x, y)[0] < 220));
    assert!(inked_near_tip, "the arrowhead must paint near its tip");
    assert_eq!(at(&img, 404, 303), WHITE, "nothing paints past the tip");

    // 4. SVG twin: the same two paths, the second one stroked.
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &NoAssets, SvgTextMode::SelectableText)
        .expect("serialize svg");
    assert_eq!(svg.matches("<path ").count(), 2, "shaft + decoration paths:\n{svg}");
    assert_eq!(
        svg.matches("stroke=\"rgb(0,0,0)\"").count(),
        2,
        "both are stroked with the line paint:\n{svg}"
    );
}

#[test]
fn compound_double_connector_paints_two_trimmed_rails() {
    // ref line-end-shaft-trim.test.ts:103-113 — `cmpd="dbl"` splits the run into
    // two w/3 rails, offset ±w/3, and both keep the tail retraction.
    let element = connector_leader(
        "straightConnector1",
        300.0,
        0.0,
        serde_json::json!({
            "color": "000000", "width": LINE_EMU, "cmpd": "dbl", "tailEnd": arrow()
        }),
    );
    let slide = compile_slide(vec![element], 960, 720);

    let stroked: Vec<&VectorPath> = slide
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path, stroke: Some(_), ..
            } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(stroked.len(), 2, "two compound rails, no centre line");

    let mut offsets = Vec::new();
    for rail in &stroked {
        let (x, y) = match rail.segments.last() {
            Some(PathSegment::LineTo { x, y }) => abs_px((100.0, 100.0), *x, *y),
            other => panic!("a rail is a line, got {other:?}"),
        };
        assert!(
            (x - (400.0 - vertex_px())).abs() < 0.05,
            "each rail stops at the vertex, got {x}"
        );
        offsets.push(y - 100.0);
    }
    // `dbl`: offsets ∓w/3, so the two rails straddle the centre line.
    let third = LINE_EMU as f32 / 3.0 / 9525.0;
    offsets.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert!(
        (offsets[0] + third).abs() < 0.05 && (offsets[1] - third).abs() < 0.05,
        "rails at ∓w/3: {offsets:?}"
    );

    // Rail widths are w/3 each (the reference rewrites `lineWidth`).
    let widths: Vec<i64> = slide
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                stroke: Some(stroke), ..
            } => Some(stroke.width_emu),
            _ => None,
        })
        .collect();
    assert_eq!(widths[..2].to_vec(), vec![LINE_EMU / 3, LINE_EMU / 3]);

    // Raster: two ink lines 4 px either side of the leader, none on the centre.
    let img = raster(&slide, 960, 720);
    assert!(at(&img, 250, 96)[0] < 200, "upper rail painted");
    assert!(at(&img, 250, 104)[0] < 200, "lower rail painted");
    assert_eq!(at(&img, 250, 100), WHITE, "the centre line is erased");
}

#[test]
fn both_arrow_ends_consume_a_short_compound_run() {
    // ref line-end-shaft-trim.test.ts:95-101 — a 15 px run with an arrow at each
    // end is fully consumed, so no shaft remains (only the two decorations).
    let element = connector_leader(
        "straightConnector1",
        15.0,
        0.0,
        serde_json::json!({
            "color": "000000", "width": LINE_EMU, "cmpd": "dbl",
            "headEnd": arrow(), "tailEnd": arrow()
        }),
    );
    let slide = compile_slide(vec![element], 960, 720);

    // Every stroked path is a rail: the consumed run paints no shaft, and both
    // decorations are `DrawLineEnd` commands rather than shaft strokes.
    let mut decorations = 0usize;
    for command in &slide.commands {
        match command {
            VectorCommand::DrawPath {
                path, stroke: Some(_), ..
            } => {
                let starts_at_origin = matches!(
                    path.segments.first(),
                    Some(PathSegment::MoveTo { x, y })
                        if abs_px((100.0, 100.0), *x, *y).0 < 100.5
                            && (abs_px((100.0, 100.0), *x, *y).1 - 100.0).abs() < 0.5
                );
                assert!(!starts_at_origin, "the consumed run must paint no shaft: {path:?}");
            }
            VectorCommand::DrawLineEnd { .. } => decorations += 1,
            _ => {}
        }
    }
    assert_eq!(decorations, 2, "both arrowheads still paint");
}

#[test]
fn curved_connector_shaft_stops_at_the_vertex_on_its_curve() {
    // ref line-end-shaft-trim.test.ts:66-68 — "trims a curved connector leader
    // along its curve": the cut is measured straight from the tip, and the curve
    // is split (de Casteljau) rather than straightened.
    let element = connector_leader(
        "curvedConnector3",
        300.0,
        200.0,
        serde_json::json!({ "color": "000000", "width": LINE_EMU, "tailEnd": arrow() }),
    );
    let slide = compile_slide(vec![element], 960, 720);
    let tip = (400.0f32, 300.0f32);

    let shaft = slide
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawPath {
                path, stroke: Some(_), ..
            } => Some(path.clone()),
            _ => None,
        })
        .expect("the leader is stroked");
    let (x, y) = match shaft.segments.last() {
        Some(PathSegment::CubicTo { x, y, .. }) => abs_px((100.0, 100.0), *x, *y),
        other => panic!("the trimmed curve keeps its cubic, got {other:?}"),
    };
    let distance = ((x - tip.0).powi(2) + (y - tip.1).powi(2)).sqrt();
    assert!(
        (distance - vertex_px()).abs() < 0.05,
        "cut point must be one retraction from the tip, got {distance} px"
    );
    assert!(
        shaft
            .segments
            .iter()
            .any(|segment| matches!(segment, PathSegment::CubicTo { .. })),
        "the curve survives the trim: {:?}",
        shaft.segments
    );
}

#[test]
fn bent_connector_short_leg_stops_at_the_vertex() {
    // ref line-end-shaft-trim.test.ts:82-86 — a 1 px terminal leg is shorter
    // than the retraction, so the trim crosses onto the preceding run.
    let element = connector_leader(
        "bentConnector2",
        300.0,
        1.0,
        serde_json::json!({ "color": "000000", "width": LINE_EMU, "tailEnd": arrow() }),
    );
    let slide = compile_slide(vec![element], 960, 720);
    let tip = (400.0f32, 101.0f32);

    let shaft = slide
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawPath {
                path, stroke: Some(_), ..
            } => Some(path.clone()),
            _ => None,
        })
        .expect("the leader is stroked");
    let (x, y) = match shaft.segments.last() {
        Some(PathSegment::LineTo { x, y }) => abs_px((100.0, 100.0), *x, *y),
        other => panic!("the trimmed leader is a polyline, got {other:?}"),
    };
    let distance = ((x - tip.0).powi(2) + (y - tip.1).powi(2)).sqrt();
    assert!(
        (distance - vertex_px()).abs() < 0.05,
        "cut point must be one retraction from the tip, got ({x}, {y}) = {distance} px"
    );
    // The cut lands on the horizontal run, not on the 1 px leg.
    assert!((y - 100.0).abs() < 0.05, "cut is on the top run, got y = {y}");
}

// ── unit ports: the retraction table, the cubic trim, the miter limit ───────

use office_terminal_core::compiler::style::line_end;
use office_terminal_core::contracts::{CompoundLine, LineEnd, LineEndKind, LineEndSize};

/// `lineEndRetract` for every kind at the three measured line widths
/// (0.5 px, 12 px, 27 px) — ref arrow-geometry.ts:128-139.
#[test]
fn retraction_matches_the_reference_table_across_line_widths() {
    const EMU_PER_PX: f32 = 9_525.0;
    const PT_TO_EMU: f32 = 12_700.0;
    for width_px in [0.5f32, 12.0, 27.0] {
        let width_emu = (width_px * EMU_PER_PX).round() as i64;
        let e = (width_emu as f32).max(2.0 * PT_TO_EMU);
        let lw = (width_emu as f32).max(0.5);
        let lg = |kind| LineEnd::new(kind, LineEndSize::Lg, LineEndSize::Lg);

        // triangle: `len - e/2`; `lg` length is 5e.
        let triangle = line_end::retract(width_emu, &lg(LineEndKind::Triangle));
        assert!(
            (triangle - (5.0 * e - e / 2.0)).abs() < 0.01,
            "triangle at {width_px} px: {triangle}"
        );
        // stealth: the `lg` notch, 3e behind the tip.
        let stealth = line_end::retract(width_emu, &lg(LineEndKind::Stealth));
        assert!((stealth - 3.0 * e).abs() < 0.01, "stealth at {width_px} px: {stealth}");
        // arrow: `(lw/2) / sin(atan2(2.5e, 4.5e))`.
        let open = line_end::retract(width_emu, &lg(LineEndKind::Arrow));
        let expected = (lw / 2.0) / (2.5f32).atan2(4.5).sin();
        assert!(
            (open - expected).abs() < 0.01,
            "arrow at {width_px} px: {open} vs {expected}"
        );
        // diamond/oval run to the centre.
        for kind in [LineEndKind::Diamond, LineEndKind::Oval] {
            assert_eq!(line_end::retract(width_emu, &lg(kind)), 0.0);
        }
        // The 2 pt floor only bites below 2 pt.
        let med_triangle = line_end::retract(
            width_emu,
            &LineEnd::new(LineEndKind::Triangle, LineEndSize::Med, LineEndSize::Med),
        );
        let base = if width_px < 2.0 {
            2.0 * PT_TO_EMU
        } else {
            width_emu as f32
        };
        assert!((med_triangle - (3.0 * base - base / 2.0)).abs() < 0.01);
    }
}

/// The trim splits the curve with de Casteljau: the endpoint stays on the path,
/// one retraction from the tip, and the last control point keeps the tangent
/// (ref arrow-geometry.ts:300-343).
#[test]
fn trimming_a_cubic_keeps_its_endpoint_and_tangent() {
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::CubicTo {
                x1: 50_000.0,
                y1: 0.0,
                x2: 50_000.0,
                y2: 100_000.0,
                x: 100_000.0,
                y: 100_000.0,
            },
        ],
        FillRule::NonZero,
    );
    let trimmed = line_end::trim_path_ends(&path, 0.0, 10_000.0);
    let (cx2, cy2, x, y) = match trimmed.segments.last() {
        Some(PathSegment::CubicTo { x2, y2, x, y, .. }) => (*x2, *y2, *x, *y),
        other => panic!("the trimmed curve keeps its cubic, got {other:?}"),
    };
    assert!(cx2 < x && cy2 < y, "the control point precedes the cut");
    let distance = ((x - 100_000.0).powi(2) + (y - 100_000.0).powi(2)).sqrt();
    assert!(
        (distance - 10_000.0).abs() < 1.0,
        "cut one retraction from the tip, got {distance}"
    );

    // A collinear cubic pins the tangent: the cut endpoint and its control point
    // stay on the 45° line, so the split preserves the direction of travel.
    let straight = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::CubicTo {
                x1: 33_333.0,
                y1: 33_333.0,
                x2: 66_667.0,
                y2: 66_667.0,
                x: 100_000.0,
                y: 100_000.0,
            },
        ],
        FillRule::NonZero,
    );
    let trimmed = line_end::trim_path_ends(&straight, 0.0, 10_000.0);
    let (cx2, cy2, sx, sy) = match trimmed.segments.last() {
        Some(PathSegment::CubicTo { x2, y2, x, y, .. }) => (*x2, *y2, *x, *y),
        other => panic!("the trimmed cubic stays cubic, got {other:?}"),
    };
    assert!((sx - sy).abs() < 1.0, "endpoint on the 45° line: ({sx}, {sy})");
    assert!(
        (cx2 - cy2).abs() < 1.0,
        "control point keeps the tangent: ({cx2}, {cy2})"
    );
    assert!(((sx - 100_000.0).powi(2) + (sy - 100_000.0).powi(2)).sqrt() > 9_000.0);
}

/// `a:ln@miterLim / 100000` reaches the IR and both backends'
/// `stroke-miterlimit` (the reference resolves `stroke.miterLimit ?? 10`,
/// ref stroke-geometry.ts:21-25).
#[test]
fn authored_miter_limit_reaches_the_ir_and_the_svg() {
    let element = connector_leader(
        "line",
        300.0,
        200.0,
        serde_json::json!({ "color": "000000", "width": LINE_EMU, "miterLimit": 2.0 }),
    );
    let slide = compile_slide(vec![element], 960, 720);
    let stroke = slide
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawPath {
                stroke: Some(stroke), ..
            } => Some(stroke.clone()),
            _ => None,
        })
        .expect("the leader is stroked");
    assert_eq!(stroke.miter_limit, 2.0);
    assert_eq!(stroke.compound, CompoundLine::Sng);
    assert_eq!(stroke.head_end, None);
    assert_eq!(stroke.tail_end, None);

    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &NoAssets, SvgTextMode::SelectableText)
        .expect("serialize svg");
    assert!(svg.contains(r#"stroke-miterlimit="2""#), "authored miter limit:\n{svg}");
}
