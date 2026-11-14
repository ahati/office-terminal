//! `line-end-shaft-trim.test.ts` (batch `text-box-b`): the `a:headEnd` /
//! `a:tailEnd` retraction and the `a:ln@cmpd` compound rails.
//!
//! Loaded as a module of the `parity_text_box_b_highlight_tests` target.

use super::*;

// ── line-end-shaft-trim.test.ts ────────────────────────────────────────────
// `EMU_PER_PX = 9525` (960 px across 9 144 000 EMU). The shaft is a 9 pt
// (114 300 EMU = 12 px) line with an `lg`/`lg` open arrow: arms 4.5 × 2.5 line
// widths, so the arms' vertex sits `6 / sin(atan(2.5 / 4.5)) ≈ 12.35 px` behind
// the tip. `compiler::style::stroke::compile_stroked_paths` emits the retracted
// shaft as a `DrawPath` and each decoration as its own `DrawLineEnd`, which is
// the IR form of the reference's translated tip frame — so a recorder that reads
// `DrawPath` sees shafts only (`shaftEnds` skips the translated frame,
// ref line-end-shaft-trim.test.ts:44-45).

const VERTEX_PX: f32 = 12.354_756; // 6 / sin(atan2(2.5, 4.5)), 9 pt line

/// One reference pixel in EMU (`EMU_PER_PX`, ref `:11`).
const EMU_PER_PX: f32 = 9_525.0;
/// 9 pt line (`LINE_EMU`, ref `:12`).
const LINE_EMU: i64 = 114_300;

/// `6 / sin(atan2(2.5, 4.5))` — the reference's `VERTEX_PX`.
fn vertex_retraction() -> f32 {
    6.0 / (2.5f32).atan2(4.5).sin()
}

/// The reference's `{ type: 'arrow', w: 'lg', len: 'lg' }`.
fn arrow_end() -> Value {
    json!({ "type": "arrow", "w": "lg", "len": "lg" })
}

/// `shape({...})` of `line-end-shaft-trim.test.ts:21-28`: a leader at (100, 100)
/// px carrying the authored stroke ends.
fn line_shape(geometry: &str, width_px: f32, height_px: f32, tail: bool, head: bool) -> Value {
    let arrow = arrow_end();
    json!({
        "type": "shape",
        "x": 100 * 9525, "y": 100 * 9525,
        "width": (width_px * 9525.0) as i64,
        "height": (height_px * 9525.0) as i64,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": geometry,
        "fill": null,
        "stroke": {
            "color": "000000",
            "width": LINE_EMU,
            "headEnd": if head { arrow.clone() } else { Value::Null },
            "tailEnd": if tail { arrow } else { Value::Null },
        },
        "textBody": null,
        "defaultTextColor": null,
        "custGeom": null,
    })
}

/// Slide-space end point (px) of every stroked subpath — the reference's
/// `shaftEnds` recorder (`:41-58`). Line-end decorations are `DrawLineEnd`
/// commands, so they are never mistaken for a shaft.
fn shaft_ends(element: Value) -> Vec<(f32, f32)> {
    let slide_json = json!({
        "index": 0,
        "slideNumber": 1,
        "background": null,
        "elements": [element],
        "elementSources": [],
        "comments": [],
    });
    let slide: pptx_model::Slide = serde_json::from_value(slide_json).expect("slide fixture");
    let (tx, _rx) = unbounded();
    let vector_slide = kitty_pptx_core::DefaultSlideCompiler::new()
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile");
    vector_slide
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path,
                stroke: Some(_),
                transform,
                ..
            } => last_point(path, transform),
            _ => None,
        })
        .collect()
}

/// Slide-space end point of a path's last drawn segment, in reference px. A
/// trimmed curve keeps its `CubicTo`/`QuadTo`, so the endpoint (not a control
/// point) is the recorded end; the command transform carries the shape origin
/// the reference's `ctx.translate` had already applied.
fn last_point(
    path: &kitty_pptx_core::contracts::VectorPath,
    transform: &AffineTransform,
) -> Option<(f32, f32)> {
    path.segments.iter().rev().find_map(|segment| {
        let (x, y) = match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => (*x, *y),
            PathSegment::QuadTo { x, y, .. } | PathSegment::CubicTo { x, y, .. } => (*x, *y),
            PathSegment::Close => return None,
        };
        let (tx, ty) = transform.transform_point(x, y);
        Some((tx / EMU_PER_PX, ty / EMU_PER_PX))
    })
}

fn expect_stops_at_vertex(ends: &[(f32, f32)], tip: (f32, f32)) {
    assert!(!ends.is_empty(), "a shaft was stroked");
    for (x, y) in ends {
        let distance = ((x - tip.0).powi(2) + (y - tip.1).powi(2)).sqrt();
        close(distance, vertex_retraction(), "distance to the tip");
    }
}

/// The reference's constant itself (`line-end-shaft-trim.test.ts:19`).
#[test]
fn vertex_retraction_matches_the_reference_constant() {
    assert!(
        (vertex_retraction() - VERTEX_PX).abs() < 1e-4,
        "6 / sin(atan2(2.5, 4.5)) = 12.3548, got {}",
        vertex_retraction()
    );
}

/// `:66` "trims a curved connector leader along its curve" — tip `(400, 300)`.
#[test]
fn curved_connector_shaft_stops_at_the_vertex() {
    expect_stops_at_vertex(
        &shaft_ends(line_shape("curvedConnector3", 300.0, 200.0, true, false)),
        (400.0, 300.0),
    );
}

/// `:71` "trims a custom-geometry cubic path" — `(0,0) → C(0.5,0)(0.5,1)(1,1)`,
/// tip `(400, 300)`.
#[test]
fn custom_geometry_cubic_shaft_stops_at_the_vertex() {
    let element = json!({
        "type": "shape",
        "x": 100 * 9525, "y": 100 * 9525,
        "width": 300 * 9525, "height": 200 * 9525,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "custGeom",
        "fill": null,
        "stroke": { "color": "000000", "width": LINE_EMU,
                    "headEnd": null,
                    "tailEnd": { "type": "arrow", "w": "lg", "len": "lg" } },
        "textBody": null, "defaultTextColor": null,
        "custGeom": [[
            { "cmd": "moveTo", "x": 0.0, "y": 0.0 },
            { "cmd": "cubicBezTo", "x1": 0.5, "y1": 0.0, "x2": 0.5, "y2": 1.0, "x": 1.0, "y": 1.0 }
        ]],
    });
    expect_stops_at_vertex(&shaft_ends(element), (400.0, 300.0));
}

/// `:76` "trims a custom-geometry arc and keeps it on its ellipse": a quarter
/// ellipse from the top-left corner to `(400, 300)`, centre `(100, 300)` with
/// radii `300 × 200`.
#[test]
fn custom_geometry_arc_shaft_stays_on_its_ellipse() {
    let element = json!({
        "type": "shape",
        "x": 100 * 9525, "y": 100 * 9525,
        "width": 300 * 9525, "height": 200 * 9525,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "custGeom",
        "fill": null,
        "stroke": { "color": "000000", "width": LINE_EMU,
                    "headEnd": null,
                    "tailEnd": { "type": "arrow", "w": "lg", "len": "lg" } },
        "textBody": null, "defaultTextColor": null,
        "custGeom": [[
            { "cmd": "moveTo", "x": 0.0, "y": 0.0 },
            { "cmd": "arcTo", "wr": 1.0, "hr": 1.0, "stAng": -90.0, "swAng": 90.0 }
        ]],
    });
    let ends = shaft_ends(element);
    expect_stops_at_vertex(&ends, (400.0, 300.0));
    for (x, y) in ends {
        let on_ellipse = ((x - 100.0) / 300.0).powi(2) + ((y - 300.0) / 200.0).powi(2);
        // The reference asserts `toBeCloseTo(1, 6)` on Canvas' exact `ellipse`.
        // The IR has no arc segment, so `custgeom::arc_to` flattens the arc to
        // cubics; the endpoint then sits ≤1e-3 px off the true ellipse, which is
        // 3.5e-6 in this normalized metric (measured: 1.0000035).
        assert!(
            (on_ellipse - 1.0).abs() < 1e-5,
            "on the ellipse: {on_ellipse}"
        );
    }
}

/// `:82` "trims a bent connector across a terminal leg shorter than the
/// retraction": `bentConnector2` runs right along the top edge then a 1 px leg
/// down to `(400, 101)`.
#[test]
fn bent_connector_short_leg_stops_at_the_vertex() {
    expect_stops_at_vertex(
        &shaft_ends(line_shape("bentConnector2", 300.0, 1.0, true, false)),
        (400.0, 101.0),
    );
}

/// `:95` "paints no compound shaft when both ends consume a short connector":
/// a 15 px double line with an arrow at each end paints no shaft at all.
#[test]
fn both_arrow_ends_consume_the_shaft() {
    let element = json!({
        "type": "shape",
        "x": 100 * 9525, "y": 100 * 9525,
        "width": 15 * 9525, "height": 0,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "straightConnector1",
        "fill": null,
        "stroke": { "color": "000000", "width": LINE_EMU, "cmpd": "dbl",
                    "headEnd": { "type": "arrow", "w": "lg", "len": "lg" },
                    "tailEnd": { "type": "arrow", "w": "lg", "len": "lg" } },
        "textBody": null, "defaultTextColor": null, "custGeom": null,
    });
    assert_eq!(shaft_ends(element), Vec::<(f32, f32)>::new());
}

/// `:103` "trims both lines of a compound straight connector": each line ends
/// at `400 − VERTEX_PX`.
#[test]
fn compound_straight_connector_trims_both_lines() {
    let element = json!({
        "type": "shape",
        "x": 100 * 9525, "y": 100 * 9525,
        "width": 300 * 9525, "height": 0,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "straightConnector1",
        "fill": null,
        "stroke": { "color": "000000", "width": LINE_EMU, "cmpd": "dbl",
                    "headEnd": null,
                    "tailEnd": { "type": "arrow", "w": "lg", "len": "lg" } },
        "textBody": null, "defaultTextColor": null, "custGeom": null,
    });
    let ends = shaft_ends(element);
    assert!(ends.len() > 1, "both composite lines are stroked");
    for (x, _) in ends {
        close(x, 400.0 - vertex_retraction(), "compound shaft end");
    }
}
