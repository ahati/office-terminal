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
fn connector_leader(
    geometry: &str,
    width_px: f32,
    height_px: f32,
    stroke: serde_json::Value,
) -> serde_json::Value {
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
                path,
                stroke: Some(_),
                ..
            } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(stroked.len(), 2, "shaft + one open-arrow decoration");

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
    //    lands exactly on it — the same distance the shaft is pulled back.
    let arrow_path = stroked[1];
    let points: Vec<(f32, f32)> = arrow_path
        .segments
        .iter()
        .filter_map(|segment| match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => {
                Some(abs_px((100.0, 100.0), *x, *y))
            }
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
    assert_eq!(
        svg.matches("<path ").count(),
        2,
        "shaft + decoration paths:\n{svg}"
    );
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
                path,
                stroke: Some(_),
                ..
            } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(stroked.len(), 3, "two rails + one decoration");

    let mut offsets = Vec::new();
    for rail in &stroked[..2] {
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
                stroke: Some(stroke),
                ..
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

    // Every stroked path is a decoration: none starts on the run's start point.
    let mut decorations = 0usize;
    for command in &slide.commands {
        if let VectorCommand::DrawPath {
            path,
            stroke: Some(_),
            ..
        } = command
        {
            let starts_at_origin = matches!(
                path.segments.first(),
                Some(PathSegment::MoveTo { x, y })
                    if abs_px((100.0, 100.0), *x, *y).0 < 100.5
                        && (abs_px((100.0, 100.0), *x, *y).1 - 100.0).abs() < 0.5
            );
            assert!(
                !starts_at_origin,
                "the consumed run must paint no shaft: {path:?}"
            );
            decorations += 1;
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
                path,
                stroke: Some(_),
                ..
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
                path,
                stroke: Some(_),
                ..
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
