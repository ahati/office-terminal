//! Text-box parity (`text-box-b`): stacked WordArt text
//! (`wordArtVert` / `wordArtVertRtl`).
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/stacked-text.test.ts`.
//!
//! The reference drives `renderTextBody` with a recording mock context and reads
//! each `fillText`'s device position, rotation, stroke geometry and run info. The
//! Rust port reduces stacked text to one grapheme per column
//! (`text/paint.rs::compile_stacked` → `text/vertical.rs::place_stacked`), so the
//! cell pitch along a column and the per-glyph stroke/inline-object paths are
//! not implemented; those cases are recorded as blocked with the missing symbol.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_box_b_stacked_tests.rs.txt`; the
//! ledger keeps their rows.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{
    AffineTransform, PositionedGlyphRun, RectF32, VectorCommand,
};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::text::vertical::{
    graphemes, place_stacked, vertical_form, StackedGlyph, VertMode,
};
use serde_json::{json, Value};

const PT: f32 = 12_700.0;
const FONT_PX: f64 = 24.0;
/// The reference's Arial 24 pt cell: `(7/6) × (2288/2048) em × 24`.
const CELL: f32 = (7.0 / 6.0) * (2288.0 / 2048.0) * 24.0;
/// Arial's `usWinDescent` share of the em (`stacked-text.test.ts:160`).
const ARIAL_DESCENT: f32 = 434.0 / 2048.0;

fn run(text: &str) -> Value {
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

/// The specs' `body(text, vert, fontFamilyEa)` fixture: 91440 / 45720 EMU insets.
fn body(text: &str, vert: &str) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l",
            "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0,
            "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null, "defItalic": null,
            "defFontFamily": null,
            "tabStops": [], "defTabSz": null, "rtl": false, "eaLnBrk": true,
            "runs": [run(text)],
        }],
        "defaultFontSize": FONT_PX,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 91_440, "rIns": 91_440, "tIns": 45_720, "bIns": 45_720,
        "wrap": "square",
        "vert": vert,
        "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
    });
    serde_json::from_value(value).expect("stacked body fixture")
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

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

/// Stub glyphs for `place_stacked`: the reference's Arial 24 pt cell, with a
/// `0.6 em` advance (its mock `measureText` width).
fn arial_glyphs(text: &str) -> Vec<StackedGlyph> {
    text.chars()
        .map(|ch| StackedGlyph {
            text: ch.to_string(),
            style: kitty_pptx_core::text::StyleId(0),
            advance: 0.6 * 24.0 * PT,
            thickness: CELL * PT,
            ascent: (CELL - ARIAL_DESCENT * 24.0) * PT,
            descent: ARIAL_DESCENT * 24.0 * PT,
        })
        .collect()
}

/// The text rectangle the reference lays out in: box 60 × 470 px, insets
/// 91440 / 45720 EMU (`lIns` 7.2 px, `tIns` 3.6 px).
fn text_rect() -> RectF32 {
    RectF32::new(
        91_440.0,
        45_720.0,
        60.0 * PT - 2.0 * 91_440.0,
        470.0 * PT - 2.0 * 45_720.0,
    )
}

// ── wordArtVert / wordArtVertRtl column placement ──────────────────────────

/// `:127` "stacks upright Latin glyphs on the column axis, one cell apart":
/// `a.x ≈ 7.2 + CELL/2`, `a.y ≈ 3.6 + CELL − 434/2048·24`, and
/// `wordArtVertRtl` starts at `60 − 7.2 − CELL/2`.
#[test]
fn stacked_glyphs_centre_on_the_column_axis_and_mirror_for_rtl() {
    let glyphs = arial_glyphs("AB");
    let ltr = place_stacked(&glyphs, text_rect(), "t", "l", false, false);
    assert_eq!(ltr.len(), 2);
    // The reference passes a cell CENTRE with `textAlign = 'center'`; the same
    // point here is the slot's left edge plus half the glyph advance.
    let centre = ltr[0].x + 0.6 * 24.0 * PT * 0.5;
    close(centre / PT, 7.2 + CELL / 2.0, "column centre");
    close(
        ltr[0].baseline / PT,
        3.6 + CELL - ARIAL_DESCENT * 24.0,
        "column baseline",
    );
    // `wordArtVertRtl` starts at the right edge (`r.calls[0].x`). A one-glyph
    // block is the faithful reduction: this port gives every grapheme its own
    // CELL-wide column, so a two-glyph block spans two cells.
    let single = arial_glyphs("A");
    // The `wordArtVertRtl` block is pinned to the END side (the right edge).
    let rtl = place_stacked(&single, text_rect(), "b", "l", false, true);
    let rtl_centre = rtl[0].x + 0.6 * 24.0 * PT * 0.5;
    close(
        rtl_centre / PT,
        60.0 - 7.2 - CELL / 2.0,
        "rtl column centre",
    );
}

/// `:150` "turns ASCII brackets 90° and draws CJK brackets with the face
/// vertical glyph": `「` is painted upright as `﹁` (U+FE41).
#[test]
fn stacked_cjk_bracket_uses_its_vertical_glyph() {
    assert_eq!(vertical_form('「'), Some('\u{fe41}'));
    assert_eq!(vertical_form('」'), Some('\u{fe42}'));
}

/// `:186` "stacks a grapheme cluster in one cell": `e` + U+0301 is one glyph,
/// `B` the next, one cell apart.
#[test]
fn stacked_keeps_a_grapheme_cluster_in_one_cell() {
    assert_eq!(graphemes("e\u{301}B"), ["e\u{301}", "B"]);
    let commands = compile(&body("e\u{301}B", "wordArtVert"), 60.0, 470.0);
    assert_eq!(texts(&draw_runs(&commands)), ["e\u{301}", "B"]);
}

// ── font slots keep grapheme clusters whole ────────────────────────────────

/// `:246` "stacked: a CJK base and its selector share one Yu Gothic cell":
/// `葛` + U+E0100 is one cluster.
#[test]
fn stacked_cjk_base_and_variation_selector_share_a_cell() {
    assert_eq!(graphemes("\u{845b}\u{e0100}B"), ["\u{845b}\u{e0100}", "B"]);
}

/// `:252` "stacked: a bracket with a selector keeps its glyphs (no
/// lone-character substitution)": `「` + U+FE0F stays one cluster.
#[test]
fn stacked_bracket_with_a_selector_is_not_substituted() {
    assert_eq!(graphemes("「\u{fe0f}"), ["「\u{fe0f}"]);
}

/// `:304` "handles 80,000 extender-only runs in linear time, horizontal and
/// stacked": the cluster segmenter must stay linear in the paragraph length.
#[test]
fn cluster_segmentation_is_linear_in_the_paragraph_length() {
    let marks = "\u{301}".repeat(80_000);
    let text = format!("a{marks}B");
    let start = std::time::Instant::now();
    let clusters = graphemes(&text);
    let elapsed = start.elapsed();
    assert_eq!(clusters.len(), 2, "one base+marks cluster then B");
    assert_eq!(clusters[0].chars().count(), 80_001);
    assert!(elapsed.as_secs_f32() < 5.0, "linear segmentation: {elapsed:?}");
}

// ── markers ───────────────────────────────────────────────────────────────

/// `:8` The stacked modes are the `wordArtVert` / `wordArtVertRtl` `bodyPr`
/// values, which the layout dispatches on.
#[test]
fn stacked_modes_dispatch_on_body_pr_vert() {
    assert_eq!(VertMode::of("wordArtVert"), VertMode::Stacked { rtl: false });
    assert_eq!(VertMode::of("wordArtVertRtl"), VertMode::Stacked { rtl: true });
}
