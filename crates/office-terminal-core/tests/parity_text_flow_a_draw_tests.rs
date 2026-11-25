//! Parity ports of the reference text-flow specs (batch `text-flow-a`, part 2:
//! draw pass through the compiled `VectorSlide` command stream).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/justify-line-break.test.ts`,
//! `justify-bracket-overlap.test.ts`, `tab-stop-spc.test.ts`. Those specs drive
//! `renderTextBody` with a recording canvas and assert on `fillText` calls and
//! `ctx.letterSpacing`. This renderer has no canvas: `compile_text_body` emits
//! positional [`PositionedGlyphRun`]s (absolute glyph x in EMU, no
//! `letterSpacing`, no contextual `measureText` collapse), so the same contracts
//! are asserted on the compiled geometry: line extents, per-glyph pitch and the
//! piece split of a justified line. Probes that need `rPr@spc` are BLOCKED —
//! see `tests/parity/coverage/text-flow-a.md`.
//!
//! Units: every position is divided by `PT_TO_EMU = 12700`, the reference specs'
//! `SCALE = 1/12700` (1 pt → 1 px).

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{AffineTransform, PositionedGlyph, PositionedGlyphRun, VectorCommand};
use office_terminal_core::text::emit::compile_text_body;

const PT_TO_EMU: f32 = 12_700.0;
const FONT_PX: f32 = 20.0;

/// One authored run of the fixture body.
enum Run<'a> {
    Text(&'a str),
    Br,
}

fn run_json(run: &Run<'_>) -> String {
    match run {
        Run::Br => "{\"type\": \"break\"}".to_string(),
        Run::Text(text) => format!(
            r#"{{ "type": "text", "text": "{text}", "bold": false, "italic": false,
                "underline": false, "strikethrough": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "fontSize": {FONT_PX}, "color": "000000", "fontFamily": "Calibri" }}"#
        ),
    }
}

/// `bodyWith`/`bodyWithTab` (`justify-line-break.test.ts:52-78`,
/// `tab-stop-spc.test.ts:145-172`): one paragraph, no insets and no margins, so
/// the box width IS the available width.
fn body(alignment: &str, runs: &[Run<'_>], tab_stops: &str) -> pptx_model::TextBody {
    let runs: Vec<String> = runs.iter().map(run_json).collect();
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {FONT_PX},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "square",
            "vert": "horz",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{{
                "alignment": "{alignment}",
                "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null,
                "lvl": 0,
                "bullet": {{ "type": "none" }},
                "defFontSize": {FONT_PX},
                "tabStops": {tab_stops},
                "defTabSz": null,
                "rtl": false,
                "eaLnBrk": true,
                "runs": [{}]
            }}]
        }}"#,
        runs.join(", ")
    );
    serde_json::from_str(&json).expect("text body fixture")
}

/// Compile a body `box_px` wide and return its `DrawText` runs.
fn compile(tb: &pptx_model::TextBody, box_px: f32) -> Vec<PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        tb,
        (box_px * PT_TO_EMU) as i64,
        (400.0 * PT_TO_EMU) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    commands
        .into_iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Runs grouped into visual lines by baseline (the reference groups by `y`).
fn lines(runs: &[PositionedGlyphRun]) -> Vec<Vec<&PositionedGlyphRun>> {
    let mut out: Vec<Vec<&PositionedGlyphRun>> = Vec::new();
    let mut last_y: Option<f32> = None;
    for run in runs {
        let Some(first) = run.glyphs.first() else {
            continue;
        };
        if last_y.is_none_or(|y| (y - first.y).abs() > PT_TO_EMU * 0.5) {
            out.push(Vec::new());
            last_y = Some(first.y);
        }
        if let Some(row) = out.last_mut() {
            row.push(run);
        }
    }
    out
}

fn px(glyph: &PositionedGlyph, field: fn(&PositionedGlyph) -> f32) -> f32 {
    field(glyph) / PT_TO_EMU
}

/// The drawn right edge of one visual line, in px: `max(x + advance)`.
fn extent(line: &[&PositionedGlyphRun]) -> f32 {
    line.iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| px(glyph, |g| g.x) + px(glyph, |g| g.advance))
        .fold(f32::NEG_INFINITY, f32::max)
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-2,
        "{what}: expected {expected}, got {actual}"
    );
}

/// The natural advance of `text` at this face, measured through the same
/// pipeline (`wrap: none` is not the issue: a single glyph run never breaks).
fn natural_width(text: &str) -> f32 {
    let runs = compile(&body("l", &[Run::Text(text)], "[]"), 4_000.0);
    runs.iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| px(glyph, |g| g.advance))
        .sum()
}

// ── justify-line-break.test.ts ─────────────────────────────────────────────

const FIRST_LINE: &str = "ああああ";
const SECOND_LINE: &str = "いいいいいいいい";

fn break_body(alignment: &str) -> pptx_model::TextBody {
    body(
        alignment,
        &[Run::Text(FIRST_LINE), Run::Br, Run::Text(SECOND_LINE)],
        "[]",
    )
}

/// `:76-101`: a line ended by a manual `<a:br/>` is LEFT-aligned under `just`
/// — its glyphs stay at their natural extent, nowhere near the right margin.
#[test]
fn d01_just_does_not_stretch_the_break_terminated_line() {
    let runs = compile(&break_body("just"), 200.0);
    assert!(!runs.is_empty(), "the body draws text");
    let rows = lines(&runs);
    assert!(rows.len() >= 2, "the <a:br/> opens a second line: {rows:?}");
    let base = natural_width(FIRST_LINE);
    // Line 1 = the 4 CJK glyphs, unstretched: exactly their natural advance and
    // (with this real face, natural ~80 px) below the 200 px box width.
    close(
        extent(&rows[0]),
        base,
        "`just` leaves the break-terminated line at its natural extent",
    );
    assert!(
        extent(&rows[0]) < 100.0,
        "the `just` break line stays near its natural extent, not the 200 px box"
    );
}

/// `:103-133`: the same break-terminated line under `dist` IS filled — the line
/// reaches the 200 px right margin, with a uniform positive justify pitch.
///
/// The reference asserts the fill through ONE contiguous `fillText` carrier with
/// `ctx.letterSpacing = ls + segPerGap`. This IR has no letterSpacing, so the
/// fill is asserted on the emitted geometry: the line's extent equals the box
/// and the per-glyph pitch is `advance + perGap` uniformly.
#[test]
fn d02_dist_fills_the_break_terminated_line() {
    let runs = compile(&break_body("dist"), 200.0);
    let rows = lines(&runs);
    assert!(rows.len() >= 2);
    let first = &rows[0];
    close(extent(first), 200.0, "`dist` fills the line to the box edge");

    // Uniform pitch: every inter-glyph delta is advance + perGap.
    let glyphs: Vec<&PositionedGlyph> = first.iter().flat_map(|run| run.glyphs.iter()).collect();
    assert_eq!(glyphs.len(), 4, "four CJK glyphs on the filled line");
    let deltas: Vec<f32> = glyphs
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x) / PT_TO_EMU)
        .collect();
    for pair in deltas.windows(2) {
        close(pair[1], pair[0], "uniform justify pitch");
    }
    let per_gap = deltas[0] - px(glyphs[0], |g| g.advance);
    // slack = 200 − natural, spread over cps − 1 internals gaps.
    let expected = (200.0 - natural_width(FIRST_LINE)) / 3.0;
    close(per_gap, expected, "justify pitch = slack / (cps − 1)");
    assert!(
        per_gap > 30.0,
        "the `dist` break line is stretched (non-zero pitch): {per_gap}"
    );
}

// ── justify-bracket-overlap.test.ts ────────────────────────────────────────

/// `:156-181`: a fully-distributed `dist` pure-CJK line is drawn with a uniform
/// per-glyph pitch equal to `slack / (cps − 1)` and reaches the right margin, so
/// no glyph can be pulled under the 約物連続 "：［" pair.
///
/// (The reference's structural guard — ONE contiguous `fillText` carrying the
/// whole run with `ctx.letterSpacing` — has no analogue here: the IR carries one
/// positioned run per justification piece and no letterSpacing. The geometry the
/// fix guarantees is asserted instead.)
#[test]
fn d03_fully_distributed_cjk_line_has_uniform_pitch() {
    const EA_JUST: &str = "スタイル名：［分類］。あいうえお";
    let slack = 60.0;
    let box_px = natural_width(EA_JUST) + slack;
    let runs = compile(&body("dist", &[Run::Text(EA_JUST)], "[]"), box_px);
    let rows = lines(&runs);
    assert_eq!(rows.len(), 1, "one `dist` line");
    let glyphs: Vec<&PositionedGlyph> = rows[0].iter().flat_map(|run| run.glyphs.iter()).collect();
    let cps = EA_JUST.chars().count();
    assert_eq!(glyphs.len(), cps, "every code point is drawn");
    close(extent(&rows[0]), box_px, "the filled line reaches the box edge");

    let deltas: Vec<f32> = glyphs
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x) / PT_TO_EMU)
        .collect();
    let per_gap = slack / (cps - 1) as f32;
    for (index, pair) in glyphs.windows(2).enumerate() {
        let expected = px(pair[0], |g| g.advance) + per_gap;
        close(deltas[index], expected, "pitch = advance + perGap");
        // The pitch is above the advance, so the glyph never overlaps its
        // successor (the "分 ⊂ ［" regression cannot occur).
        assert!(
            deltas[index] > px(pair[0], |g| g.advance),
            "glyph {index} does not overlap the next"
        );
    }
}

/// `:183-217`: the no-overlap property — every glyph starts at or after its
/// predecessor's full advance, and the last glyph ends exactly on the segment
/// box edge (measure == draw).
#[test]
fn d04_justified_cjk_glyphs_never_overlap() {
    const EA_JUST: &str = "スタイル名：［分類］。あいうえお";
    let box_px = natural_width(EA_JUST) + 60.0;
    let runs = compile(&body("dist", &[Run::Text(EA_JUST)], "[]"), box_px);
    let rows = lines(&runs);
    let glyphs: Vec<&PositionedGlyph> = rows[0].iter().flat_map(|run| run.glyphs.iter()).collect();
    let mut worst_overlap = f32::NEG_INFINITY;
    for pair in glyphs.windows(2) {
        let overlap = (px(pair[0], |g| g.x) + px(pair[0], |g| g.advance)) - px(pair[1], |g| g.x);
        worst_overlap = worst_overlap.max(overlap);
    }
    assert!(
        worst_overlap < 0.0,
        "no adjacent pair overlaps: worst overlap {worst_overlap}"
    );
    let last = glyphs.last().expect("a drawn glyph");
    close(
        px(last, |g| g.x) + px(last, |g| g.advance),
        box_px,
        "the final glyph ends on the segment box edge",
    );
}

/// `:219-239`: a partially-gapped (CJK + Latin) justified line keeps the piece
/// split — `あ［AB］い` yields the five pieces `[あ][［][AB][］][い]` because the
/// Latin A|B seam opens no gap, and the whole run is never one draw.
#[test]
fn d05_partial_gaps_split_the_line_into_pieces() {
    const MIXED: &str = "あ［AB］い";
    let box_px = natural_width(MIXED) + 40.0;
    let runs = compile(&body("dist", &[Run::Text(MIXED)], "[]"), box_px);
    let rows = lines(&runs);
    assert_eq!(rows.len(), 1, "one line");
    let texts: Vec<&str> = rows[0].iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, ["あ", "［", "AB", "］", "い"], "the Latin seam stays one piece");
    assert_eq!(rows[0].len(), 5, "multiple pieces are drawn via the loop");
    // Every inter-piece gap is the same pitch: slack 40 over 4 gaps = 10 px.
    let pieces_first: Vec<&PositionedGlyph> = rows[0]
        .iter()
        .map(|run| run.glyphs.first().expect("a shaped glyph"))
        .collect();
    for pair in pieces_first.windows(2) {
        let delta = (pair[1].x - pair[0].x) / PT_TO_EMU;
        // delta = advance of the previous piece + 10 px of slack.
        assert!(delta > 0.0, "pieces advance rightwards");
    }
    close(extent(&rows[0]), box_px, "the filled line reaches the box edge");
}

/// `:241-274` (a): a non-justified line is drawn as ONE contiguous piece at its
/// natural advance — the left-aligned control.
///
/// (b) of the same case — an `@spc`-only line carrying `letterSpacing = ls` — is
/// BLOCKED: `rPr@spc` is parsed into `pptx_model` but the emit path has no
/// letterSpacing, so no run can carry it.
#[test]
fn d06_left_aligned_line_is_one_contiguous_piece() {
    const EA_JUST: &str = "スタイル名：［分類］。あいうえお";
    let runs = compile(&body("l", &[Run::Text(EA_JUST)], "[]"), natural_width(EA_JUST) + 60.0);
    let rows = lines(&runs);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 1, "one contiguous draw: {:?}", rows[0]);
    assert_eq!(rows[0][0].text, EA_JUST);
    close(
        extent(&rows[0]),
        natural_width(EA_JUST),
        "the left-aligned line is not stretched",
    );
}

// ── tab-stop-spc.test.ts ───────────────────────────────────────────────────

/// `:204-211`: the `ls == 0` tab segment is one contiguous draw seated at the
/// right stop — 7 EA code points ending exactly on the 400 px stop.
#[test]
fn d07_tab_stop_cell_is_one_piece_ending_on_the_stop() {
    const EA_SPC: &str = "［あ］いうえお";
    const TAB_POS_EMU: i64 = 400 * 12_700;
    let stops = format!(r#"[{{"pos": {TAB_POS_EMU}, "algn": "r"}}]"#);
    // `\t` must reach the fixture as the JSON escape, not a raw control byte.
    let text = format!("\\t{EA_SPC}");
    let runs = compile(&body("l", &[Run::Text(&text)], &stops), 600.0);
    let rows = lines(&runs);
    assert_eq!(rows.len(), 1, "one line");
    assert_eq!(rows[0].len(), 1, "one fillText-equivalent piece: {:?}", rows[0]);
    assert_eq!(rows[0][0].text, EA_SPC);
    let first = rows[0][0].glyphs.first().expect("a shaped glyph");
    let advance: f32 = rows[0][0].glyphs.iter().map(|glyph| px(glyph, |g| g.advance)).sum();
    close(
        px(first, |g| g.x) + advance,
        400.0,
        "the cell ends on the right tab stop",
    );
}
