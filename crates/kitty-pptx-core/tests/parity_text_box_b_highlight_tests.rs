//! Text-box parity (`text-box-b`): highlight paint, display equation wrapping
//! and line-end shaft trimming.
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/{text-highlight,display-equation-wrap,
//! line-end-shaft-trim}.test.ts`.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PathSegment, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::SlideCompiler;
use serde_json::{json, Value};

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;
const EMU_PER_PX: f32 = 9_525.0;

fn text_run(text: &str) -> Value {
    json!({
        "type": "text",
        "text": text,
        "bold": null,
        "italic": null,
        "underline": false,
        "strikethrough": false,
        "strikeDouble": false,
        "hyperlinkUsesTextFill": false,
        "fieldType": null,
        "fontSize": FONT_PX,
        "color": "000000",
        "fontFamily": "Arial",
    })
}

/// `run(text, { highlight })` of `text-highlight.test.ts:57`.
fn highlighted_run(text: &str, highlight: &str) -> Value {
    let mut run = text_run(text);
    run["highlight"] = json!(highlight);
    run
}

fn para(runs: Vec<Value>) -> Value {
    json!({
        "alignment": "l",
        "marL": 0, "marR": 0, "indent": 0,
        "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
        "lvl": 0,
        "bullet": { "type": "none" },
        "defFontSize": null, "defColor": null, "defBold": null, "defItalic": null,
        "defFontFamily": null,
        "tabStops": [],
        "defTabSz": null,
        "rtl": false,
        "eaLnBrk": true,
        "runs": runs,
    })
}

fn body(paragraphs: Vec<Value>) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": paragraphs,
        "defaultFontSize": FONT_PX,
        "defaultBold": null,
        "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square",
        "vert": "horz",
        "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
    });
    serde_json::from_value(value).expect("text body fixture")
}

fn compile(text_body: &pptx_model::TextBody, box_w: f32, box_h: f32) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        (box_w * PT) as i64,
        (box_h * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    )
}

fn draw_runs(commands: &[VectorCommand]) -> Vec<PositionedGlyphRun> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.iter().cloned()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn texts(runs: &[PositionedGlyphRun]) -> Vec<String> {
    runs.iter().map(|run| run.text.clone()).collect()
}

fn run_y(run: &PositionedGlyphRun) -> f32 {
    run.glyphs.first().map(|glyph| glyph.y).unwrap_or(f32::NAN)
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

// ── text-highlight.test.ts (§21.1.2.3.4) ───────────────────────────────────
// The reference asserts the resolved `rgba()` highlight of each
// `layoutParagraph(...).segments[i]` and the marker box `paintHighlight` paints.
// `PositionedGlyphRun` has no highlight field and there is no highlight paint,
// so every case is blocked; the run split (the other half of each assertion) is
// still pinned.

/// `:74` "splits adjacent runs whose highlight differs": `['AAA', 'BBB']` with
/// `rgba(255,255,0,1)` / `rgba(0,255,0,1)` — the two markers must not bleed.
#[test]
#[ignore = "blocked: PositionedGlyphRun carries no highlight; no layoutParagraph segment API (sameMeta highlight identity)"]
fn highlight_splits_adjacent_runs_that_differ() {
    let runs = draw_runs(&compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            highlighted_run("BBB", "00FF00"),
        ])]),
        10_000.0,
        400.0,
    ));
    assert_eq!(texts(&runs), ["AAA", "BBB"]);
    // Blocked assertion: seg[0].highlight == rgba(255,255,0,1),
    // seg[1].highlight == rgba(0,255,0,1).
}

/// `:94` "coalesces adjacent runs that share the same highlight": one segment
/// `'AAABBB'` in `rgba(255,255,0,1)`.
#[test]
#[ignore = "blocked: PositionedGlyphRun carries no highlight; the emitter never coalesces adjacent runs"]
fn highlight_coalesces_runs_that_agree() {
    let runs = draw_runs(&compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            highlighted_run("BBB", "FFFF00"),
        ])]),
        10_000.0,
        400.0,
    ));
    assert_eq!(texts(&runs), ["AAABBB"]);
    // Blocked assertion: the single segment's highlight == rgba(255,255,0,1).
}

/// `:104` "splits a highlighted run from an unhighlighted neighbour".
#[test]
#[ignore = "blocked: PositionedGlyphRun carries no highlight (the second segment must carry none)"]
fn highlight_splits_a_highlighted_run_from_its_neighbour() {
    let runs = draw_runs(&compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            text_run("BBB"),
        ])]),
        10_000.0,
        400.0,
    ));
    assert_eq!(texts(&runs), ["AAA", "BBB"]);
    // Blocked assertion: seg[0].highlight == rgba(255,255,0,1), seg[1] has none.
}

/// `:120` "fills one rect with the highlight colour then restores the glyph
/// colour": the box is `(5, 100 − 20·0.85, 30, 20·1.1)` in the highlight
/// colour, with `fillStyle` left on the glyph colour for the caller's
/// `fillText`.
#[test]
#[ignore = "blocked: no paintHighlight equivalent (marker box geometry 0.85 em / 1.1 em absent)"]
fn highlight_box_is_painted_behind_the_glyphs() {
    let (baseline, size) = (100.0f32, 20.0f32);
    close(baseline - size * 0.85, 83.0, "box top");
    close(size * 1.1, 22.0, "box height");
    // The rect is painted at x = 5 with width 30 (the run's advance).
}

/// `:141` "uses the face glyph box ending at the run line box bottom": Arial at
/// 48 pt paints the marker from `baseline − 42.75` over `53.64`, i.e. the usWin
/// extent ends at the bottom of the 1.2 × size line box.
#[test]
#[ignore = "blocked: no PowerPoint face usWin extent accessor for the highlight box"]
fn highlight_uses_the_face_glyph_box() {
    let (baseline, size) = (100.0f32, 48.0f32);
    close(baseline - 42.75, 57.25, "box top");
    close(size * 1.1, 52.8, "1.2 em line box proxy");
    close(-42.75 + 53.64, 10.89, "usWin extent");
}

/// `:161` "draws the box under a glyph painted afterwards (background then
/// glyph)": the recorded ops are `['rect', 'text']`, the glyph in the text
/// colour.
#[test]
#[ignore = "blocked: no ordered paint-op recording for highlights"]
fn highlight_box_precedes_the_glyph() {}

// ── display-equation-wrap.test.ts ──────────────────────────────────────────

/// `:20` "starts and ends a visual line around a display equation in a PPTX
/// paragraph": the three visual lines are `['before', '[equation]', 'after']`,
/// one 1.2 em (20 pt) line box apart.
#[test]
fn display_equation_starts_and_ends_its_own_visual_line() {
    let runs = draw_runs(&compile(
        &body(vec![para(vec![
            text_run("before"),
            json!({
                "type": "math",
                "display": true,
                "fontSize": FONT_PX,
                "nodes": [{ "kind": "run", "text": "x", "style": "italic" }],
            }),
            text_run("after"),
        ])]),
        200.0,
        400.0,
    ));
    let before = runs
        .iter()
        .find(|run| run.text == "before")
        .expect("before run");
    let equation = runs
        .iter()
        .find(|run| run.text == "x")
        .expect("equation run");
    let after = runs
        .iter()
        .find(|run| run.text == "after")
        .expect("after run");
    let line_box = 1.2 * 20.0 * PT;
    close(
        run_y(equation) - run_y(before),
        line_box,
        "equation line pitch",
    );
    close(
        run_y(after) - run_y(equation),
        line_box,
        "after line pitch",
    );
}

// ── line-end-shaft-trim.test.ts ────────────────────────────────────────────
// `EMU_PER_PX = 9525` (960 px across 9 144 000 EMU). The shaft is a 9 pt
// (114 300 EMU = 12 px) line with an `lg`/`lg` open arrow: arms 4.5 × 2.5 line
// widths, so the arms' vertex sits `6 / sin(atan(2.5 / 4.5)) ≈ 10.62 px` behind
// the tip. The core compiles `Stroke::head_end` / `tail_end` to nothing, so no
// arrowhead geometry and no shaft retraction exist yet.

const LINE_EMU: i64 = 114_300;
const VERTEX_PX: f32 = 12.354_756; // 6 / sin(atan2(2.5, 4.5)), 9 pt line

/// `6 / sin(atan2(2.5, 4.5))` — the reference's `VERTEX_PX`.
fn vertex_retraction() -> f32 {
    6.0 / (2.5f32).atan2(4.5).sin()
}

fn line_shape(geometry: &str, width_px: f32, height_px: f32, tail: bool, head: bool) -> Value {
    let arrow = json!({ "type": "arrow", "w": "lg", "len": "lg" });
    json!({
        "type": "shape",
        "x": 100.0 * 9525.0, "y": 100.0 * 9525.0,
        "width": width_px * 9525.0,
        "height": height_px * 9525.0,
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
/// `shaftEnds` recorder.
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
            VectorCommand::DrawPath { path, stroke, .. } if stroke.is_some() => path
                .segments
                .iter()
                .rev()
                .find_map(|segment| match segment {
                    PathSegment::LineTo { x, y } | PathSegment::MoveTo { x, y } => {
                        Some((*x / EMU_PER_PX, *y / EMU_PER_PX))
                    }
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

fn expect_stops_at_vertex(ends: &[(f32, f32)], tip: (f32, f32)) {
    assert!(!ends.is_empty(), "a shaft was stroked");
    for (x, y) in ends {
        let distance = ((x - tip.0).powi(2) + (y - tip.1).powi(2)).sqrt();
        close(distance, vertex_retraction(), "distance to the tip");
    }
}

/// `:66` "trims a curved connector leader along its curve" — tip `(400, 300)`.
#[test]
#[ignore = "blocked: Stroke::tail_end / head_end are not compiled (no arrowhead geometry, no shaft trim)"]
fn curved_connector_shaft_stops_at_the_vertex() {
    expect_stops_at_vertex(
        &shaft_ends(line_shape("curvedConnector3", 300.0, 200.0, true, false)),
        (400.0, 300.0),
    );
}

/// `:71` "trims a custom-geometry cubic path" — `(0,0) → C(0.5,0)(0.5,1)(1,1)`,
/// tip `(400, 300)`.
#[test]
#[ignore = "blocked: Stroke::tail_end / head_end are not compiled (no arrowhead geometry, no shaft trim)"]
fn custom_geometry_cubic_shaft_stops_at_the_vertex() {
    let element = json!({
        "type": "shape",
        "x": 100.0 * 9525.0, "y": 100.0 * 9525.0,
        "width": 300.0 * 9525.0, "height": 200.0 * 9525.0,
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
#[ignore = "blocked: Stroke::tail_end / head_end are not compiled (no arrowhead geometry, no shaft trim)"]
fn custom_geometry_arc_shaft_stays_on_its_ellipse() {
    let element = json!({
        "type": "shape",
        "x": 100.0 * 9525.0, "y": 100.0 * 9525.0,
        "width": 300.0 * 9525.0, "height": 200.0 * 9525.0,
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
        assert!(
            (on_ellipse - 1.0).abs() < 1e-6,
            "on the ellipse: {on_ellipse}"
        );
    }
}

/// `:82` "trims a bent connector across a terminal leg shorter than the
/// retraction": `bentConnector2` runs right along the top edge then a 1 px leg
/// down to `(400, 101)`.
#[test]
#[ignore = "blocked: Stroke::tail_end / head_end are not compiled (no arrowhead geometry, no shaft trim)"]
fn bent_connector_short_leg_stops_at_the_vertex() {
    expect_stops_at_vertex(
        &shaft_ends(line_shape("bentConnector2", 300.0, 1.0, true, false)),
        (400.0, 101.0),
    );
}

/// `:95` "paints no compound shaft when both ends consume a short connector":
/// a 15 px double line with an arrow at each end paints no shaft at all.
#[test]
#[ignore = "blocked: Stroke::cmpd / head_end / tail_end are not compiled"]
fn both_arrow_ends_consume_the_shaft() {
    let element = json!({
        "type": "shape",
        "x": 100.0 * 9525.0, "y": 100.0 * 9525.0,
        "width": 15.0 * 9525.0, "height": 0.0,
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
#[ignore = "blocked: Stroke::cmpd / tail_end are not compiled (no compound shaft, no trim)"]
fn compound_straight_connector_trims_both_lines() {
    let element = json!({
        "type": "shape",
        "x": 100.0 * 9525.0, "y": 100.0 * 9525.0,
        "width": 300.0 * 9525.0, "height": 0.0,
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

/// The reference's constant itself (`line-end-shaft-trim.test.ts:19`).
#[test]
fn vertex_retraction_matches_the_reference_constant() {
    assert!(
        (vertex_retraction() - VERTEX_PX).abs() < 1e-4,
        "6 / sin(atan2(2.5, 4.5)) = 12.3548, got {}",
        vertex_retraction()
    );
}
