//! Text-box parity (`text-box-b`): highlight paint, display equation wrapping
//! and line-end shaft trimming.
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/{text-highlight,display-equation-wrap,
//! line-end-shaft-trim}.test.ts`.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_box_b_highlight_tests.rs.txt`; the
//! ledger keeps their rows.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{
    AffineTransform, PathSegment, PositionedGlyphRun, RectF32, VectorCommand, VectorPaint,
};
use kitty_pptx_core::text::emit::compile_text_body;
use serde_json::{json, Value};

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;

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

/// One emitted highlight marker: its box in slide-local EMU and its resolved
/// colour — the reference's `fillRect` op in `text-highlight.test.ts:12-30`.
#[derive(Clone, Debug)]
struct HighlightBox {
    rect: RectF32,
    color: [u8; 4],
}

/// The highlight boxes this compile paints, in paint order. A
/// `paintHighlight` call is the only solid `DrawPath` the text compiler emits
/// for a body without equations.
fn highlight_boxes(commands: &[VectorCommand]) -> Vec<HighlightBox> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path,
                fill: Some(VectorPaint::Solid(color)),
                ..
            } => Some(HighlightBox {
                rect: path_bounds(path),
                color: *color,
            }),
            _ => None,
        })
        .collect()
}

/// Axis-aligned bounds of a rect path, in EMU.
fn path_bounds(path: &kitty_pptx_core::contracts::VectorPath) -> RectF32 {
    let (mut left, mut top, mut right, mut bottom) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for segment in &path.segments {
        let (x, y) = match segment {
            PathSegment::MoveTo { x, y } | PathSegment::LineTo { x, y } => (*x, *y),
            PathSegment::QuadTo { x, y, .. } | PathSegment::CubicTo { x, y, .. } => (*x, *y),
            PathSegment::Close => continue,
        };
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    RectF32::new(left, top, right - left, bottom - top)
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
fn highlight_splits_adjacent_runs_that_differ() {
    let commands = compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            highlighted_run("BBB", "00FF00"),
        ])]),
        10_000.0,
        400.0,
    );
    assert_eq!(texts(&draw_runs(&commands)), ["AAA", "BBB"]);
    // The highlight is part of segment identity (`sameStyle`, renderer.ts:2002),
    // so each run keeps its own resolved marker colour.
    let boxes = highlight_boxes(&commands);
    assert_eq!(boxes.len(), 2, "one box per segment: {boxes:?}");
    assert_eq!(boxes[0].color, [255, 255, 0, 255], "seg[0] yellow");
    assert_eq!(boxes[1].color, [0, 255, 0, 255], "seg[1] green");
    // AAA is one glyph wider than nothing: the boxes abut, never overlap.
    close(
        boxes[0].rect.right(),
        boxes[1].rect.x,
        "the two markers abut",
    );
}

/// `:94` "coalesces adjacent runs that share the same highlight": one segment
/// `'AAABBB'` in `rgba(255,255,0,1)`.
#[test]
fn highlight_coalesces_runs_that_agree() {
    let commands = compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            highlighted_run("BBB", "FFFF00"),
        ])]),
        10_000.0,
        400.0,
    );
    assert_eq!(texts(&draw_runs(&commands)), ["AAABBB"]);
    let boxes = highlight_boxes(&commands);
    assert_eq!(boxes.len(), 1, "one marker for the merged segment: {boxes:?}");
    assert_eq!(boxes[0].color, [255, 255, 0, 255], "merged yellow");
}

/// `:104` "splits a highlighted run from an unhighlighted neighbour".
#[test]
fn highlight_splits_a_highlighted_run_from_its_neighbour() {
    let commands = compile(
        &body(vec![para(vec![
            highlighted_run("AAA", "FFFF00"),
            text_run("BBB"),
        ])]),
        10_000.0,
        400.0,
    );
    let runs = draw_runs(&commands);
    assert_eq!(texts(&runs), ["AAA", "BBB"]);
    let boxes = highlight_boxes(&commands);
    assert_eq!(boxes.len(), 1, "only AAA carries a marker: {boxes:?}");
    assert_eq!(boxes[0].color, [255, 255, 0, 255], "seg[0] yellow");
    // The box spans AAA's glyph advance only — it must not reach into BBB.
    let aaa = &runs[0];
    close(boxes[0].rect.x, aaa.glyphs[0].x, "box left = AAA's origin");
    let aaa_right = aaa
        .glyphs
        .iter()
        .map(|glyph| glyph.advance)
        .sum::<f32>();
    close(boxes[0].rect.w, aaa_right, "box width = AAA's advance");
}

/// `:120` "fills one rect with the highlight colour then restores the glyph
/// colour": the box is `top = baseline − 0.85·em`, `height = 1.1·em`
/// (`highlightBox`, `packages/core/src/text/highlight-box.ts:26-32`) at the
/// run's advance.
#[test]
fn highlight_box_is_painted_behind_the_glyphs() {
    let commands = compile(
        &body(vec![para(vec![highlighted_run("AAA", "FFFF00")])]),
        10_000.0,
        400.0,
    );
    let runs = draw_runs(&commands);
    let run = &runs[0];
    let baseline = run.glyphs[0].y;
    let size = run.font_size_pt * PT;
    let boxes = highlight_boxes(&commands);
    assert_eq!(boxes.len(), 1, "one marker box: {boxes:?}");
    let rect = boxes[0].rect;
    close(rect.x, run.glyphs[0].x, "box x = run origin");
    close(
        rect.w,
        run.glyphs.iter().map(|glyph| glyph.advance).sum::<f32>(),
        "box width = run advance",
    );
    close(rect.y, baseline - size * 0.85, "box top = baseline - 0.85 em");
    close(rect.h, size * 1.1, "box height = 1.1 em");
}

/// `:161` "draws the box under a glyph painted afterwards (background then
/// glyph)": the recorded ops are `['rect', 'text']`, the glyph in the text
/// colour.
#[test]
fn highlight_box_precedes_the_glyph() {
    let commands = compile(
        &body(vec![para(vec![highlighted_run("AAA", "FFFF00")])]),
        10_000.0,
        400.0,
    );
    // The marker rect is its own command and the glyphs ship in one DrawText
    // that comes after it, so a display-list renderer paints background first.
    let rect_at = commands
        .iter()
        .position(|command| matches!(command, VectorCommand::DrawPath { .. }))
        .expect("a marker rect");
    let text_at = commands
        .iter()
        .position(|command| matches!(command, VectorCommand::DrawText { .. }))
        .expect("the glyph run");
    assert!(rect_at < text_at, "background then glyph: {rect_at} < {text_at}");
    let boxes = highlight_boxes(&commands);
    assert_eq!(boxes[0].color, [255, 255, 0, 255], "box in the highlight colour");
    let runs = draw_runs(&commands);
    assert_eq!(runs[0].color, [0, 0, 0, 255], "glyph in the text colour");
}

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

const VERTEX_PX: f32 = 12.354_756; // 6 / sin(atan2(2.5, 4.5)), 9 pt line

/// `6 / sin(atan2(2.5, 4.5))` — the reference's `VERTEX_PX`.
fn vertex_retraction() -> f32 {
    6.0 / (2.5f32).atan2(4.5).sin()
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
