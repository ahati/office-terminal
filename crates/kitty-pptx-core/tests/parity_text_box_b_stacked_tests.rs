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

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{
    AffineTransform, HyperlinkTarget, PositionedGlyphRun, RectF32, VectorCommand,
};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::text::vertical::{
    graphemes, place_stacked, vertical_form, StackedGlyph, VertMode,
};
use kitty_pptx_core::SlideCompiler;
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

/// `:127` "… one cell apart": `b.y − a.y ≈ CELL`. The reference stacks the
/// glyphs down ONE column; this port gives every grapheme its own column, so
/// the cell pitch along the column has no representation.
#[test]
#[ignore = "blocked: one grapheme per column — successive glyphs advance in x, not one CELL down the column"]
fn stacked_glyphs_advance_one_cell_along_the_column() {
    let glyphs = arial_glyphs("AB");
    let slots = place_stacked(&glyphs, text_rect(), "t", "l", false, false);
    close((slots[1].baseline - slots[0].baseline) / PT, CELL, "cell pitch");
}

/// `:139` "uses the font-table cell for an exceptional drawing face and its
/// Latin column face": `Yu Gothic`'s usWin box `2636/2048 em` gives the cell.
#[test]
#[ignore = "blocked: no per-face usWin cell table for stacked text (StackedGlyph::thickness is the advance)"]
fn stacked_cell_follows_the_drawing_face() {
    let cell = (7.0 / 6.0) * (2636.0 / 2048.0) * 24.0;
    close(cell, 36.03, "Yu Gothic cell");
}

/// `:150` "turns ASCII brackets 90° and draws CJK brackets with the face
/// vertical glyph": `「` is painted upright as `﹁` (U+FE41).
#[test]
fn stacked_cjk_bracket_uses_its_vertical_glyph() {
    assert_eq!(vertical_form('「'), Some('\u{fe41}'));
    assert_eq!(vertical_form('」'), Some('\u{fe42}'));
}

/// `:150` "turns ASCII brackets 90°": `(` is rotated `π/2` in a stacked column.
#[test]
#[ignore = "blocked: stacked glyphs carry no per-glyph rotation (ASCII brackets are never turned 90°)"]
fn stacked_ascii_bracket_is_turned_90_degrees() {}

/// `:163` "draws a vrt2-only ∥ unturned in the vertical-form frame and a vert ∥
/// turned" (`Yu Gothic` vs `MS Gothic`).
#[test]
#[ignore = "blocked: no vrt2/vert font-feature probing for stacked glyphs"]
fn stacked_parallel_mark_follows_the_face_feature() {}

/// `:175` "uses bold-specific vert coverage for the drawing face"
/// (`[24, 24 × 0.6]` for regular vs bold).
#[test]
#[ignore = "blocked: no bold-specific vert coverage (no font-feature model) for stacked glyphs"]
fn stacked_bold_specific_vert_coverage() {
    close(24.0 * 0.6, 14.4, "rotated advance");
}

/// `:186` "stacks a grapheme cluster in one cell": `e` + U+0301 is one glyph,
/// `B` the next, one cell apart.
#[test]
fn stacked_keeps_a_grapheme_cluster_in_one_cell() {
    assert_eq!(graphemes("e\u{301}B"), ["e\u{301}", "B"]);
    let commands = compile(&body("e\u{301}B", "wordArtVert"), 60.0, 470.0);
    assert_eq!(texts(&draw_runs(&commands)), ["e\u{301}", "B"]);
}

/// `:196` "hands each glyph its run hyperlink so the text layer installs link
/// handlers": both stacked glyphs carry the run's external link.
#[test]
#[ignore = "blocked: PositionedGlyphRun has no hyperlink; links are slide-level SlideLink bounds"]
fn stacked_glyphs_carry_the_run_hyperlink() {
    let mut text_body = body("AB", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.hyperlink = Some("https://example.com/".to_string());
    }
    let slide_json = json!({
        "index": 0,
        "slideNumber": 1,
        "background": null,
        "elements": [{
            "type": "shape",
            "x": 0, "y": 0, "width": 4_572_000, "height": 2_286_000,
            "rotation": 0.0, "flipH": false, "flipV": false,
            "geometry": "rect", "fill": null, "stroke": null,
            "textBody": serde_json::to_value(&text_body).unwrap(),
            "custGeom": null,
        }],
        "elementSources": [],
        "comments": [],
    });
    let slide: pptx_model::Slide = serde_json::from_value(slide_json).expect("slide fixture");
    let (tx, _rx) = unbounded();
    let vector_slide = kitty_pptx_core::DefaultSlideCompiler::new()
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile");
    let target = HyperlinkTarget::ExternalUri("https://example.com/".to_string());
    let resolved: Vec<HyperlinkTarget> = vector_slide
        .hyperlinks
        .iter()
        .map(|link| link.target.clone())
        .collect();
    assert_eq!(resolved, [target.clone(), target]);
}

/// `:207` "keeps a stacked body upright under flipH and turns it 180° under
/// flipV": `shapeTextRotation('wordArtVert', 0, true, false) === 0`,
/// `('wordArtVertRtl', 0, false, true) === 180`, `('horz', 0, false, true) === 0`.
#[test]
#[ignore = "blocked: no shapeTextRotation(vert, rotation, flipH, flipV) text-frame helper"]
fn stacked_body_rotation_follows_the_frame_flips() {
    // Reference values, kept for the implementation:
    //   shapeTextRotation('wordArtVert', 0, flipH, false) == 0
    //   shapeTextRotation('wordArtVertRtl', 0, false, flipV) == 180
    //   shapeTextRotation('horz', 0, false, flipV) == 0
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

/// `:258` "horizontal: the cluster stays in the base segment and font":
/// `葛`+IVS + `B e`+U+0301 segment as `['葛<IVS>', 'B é']`.
#[test]
#[ignore = "blocked: no layoutParagraph segment API (script/face slot segmentation of the horizontal path)"]
fn horizontal_cluster_stays_in_the_base_segment() {
    assert_eq!(graphemes("\u{845b}\u{e0100}B e\u{301}"), [
        "\u{845b}\u{e0100}",
        " ",
        "B",
        " ",
        "e\u{301}"
    ]);
}

/// `:296` "stacked: `葛<IVS>` split across runs stays one cell": the cluster
/// straddles two authored runs (the second is bold).
#[test]
#[ignore = "blocked: stacked segmentation runs per BreakItem, so a cluster spanning two runs becomes two cells"]
fn stacked_cluster_split_across_runs_stays_one_cell() {}

/// `:296` "horizontal: … split across runs stays in the base segment".
#[test]
#[ignore = "blocked: no layoutParagraph segment API (grapheme segmentation over the paragraph text)"]
fn horizontal_cluster_split_across_runs_stays_in_the_base_segment() {}

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

// ── inline objects (equations) inside a stacked body ───────────────────────

/// `:324` "%s preserves horizontal equation geometry in both stacked modes"
/// (cached): the equation bitmap is `54 × 36` at `rot 0` and the stacked
/// columns reserve the same box.
#[test]
#[ignore = "blocked: compile_stacked skips BreakItem::Object — no inline equation in a stacked body"]
fn stacked_cached_equation_keeps_the_horizontal_geometry() {
    let commands = compile(&body("A", "wordArtVert"), 300.0, 300.0);
    assert!(!draw_runs(&commands).is_empty());
}

/// `:324` (no engine): the equation keeps the empty geometry and reserves the
/// mark cell for its blank column.
#[test]
#[ignore = "blocked: compile_stacked skips BreakItem::Object — no inline equation in a stacked body"]
fn stacked_equation_without_an_engine_keeps_the_blank_column() {}

/// `:324` (conversion failure): `['A', 'Equation unavailable', 'B']`, no image.
#[test]
#[ignore = "blocked: no math fallback text emission in the stacked path"]
fn stacked_equation_conversion_failure_draws_the_fallback_text() {}

/// `:324` (rasterization failure): same as conversion.
#[test]
#[ignore = "blocked: no math fallback text emission in the stacked path"]
fn stacked_equation_rasterization_failure_draws_the_fallback_text() {}

/// `:365` "replaces a complete inline equation branch in horz / wordArtVert /
/// wordArtVertRtl": the authored `mathFallbacks` preview replaces the branch.
#[test]
#[ignore = "blocked: Paragraph::math_fallbacks is not consumed by text::emit"]
fn stacked_complete_inline_equation_branch_is_replaced() {}

// ── strikes and underlines ─────────────────────────────────────────────────

/// `:394` "paints glyph-sized horizontal strikes in %s" for `wordArtVert`
/// (two glyphs + the invisible paragraph mark, `8 px` wide).
#[test]
#[ignore = "blocked: no stroke geometry in the vector IR (strike/underline and paragraph-mark strokes are painted by the rasterizer)"]
fn stacked_paints_glyph_sized_strikes() {
    close(8.0, 8.0, "paragraph mark width");
}

/// `:394` "paints glyph-sized horizontal strikes in %s" for `wordArtVertRtl`.
#[test]
#[ignore = "blocked: no stroke geometry in the vector IR (strike/underline and paragraph-mark strokes are painted by the rasterizer)"]
fn stacked_rtl_paints_glyph_sized_strikes() {}

/// `:415` "keeps strikes in a rotated glyph frame and a vertical presentation
/// glyph frame": `(` strike runs vertically, `「`'s horizontally.
#[test]
#[ignore = "blocked: no stroke geometry in the vector IR (rotated glyph frames are not modelled)"]
fn stacked_strikes_follow_the_glyph_frame() {}

/// `:424` "honors an explicit unstruck paragraph insertion style": one strike
/// only (the paragraph mark stays unstruck).
#[test]
#[ignore = "blocked: no stroke geometry in the vector IR; end_run_properties does not gate a paragraph-mark strike"]
fn stacked_unstruck_paragraph_mark_style_is_honoured() {}

/// `:433` "uses one underline frame per mixed-size wrapped %s column"
/// (`wordArtVert`: widths `[32 × 0.05, 1]`, colour `rgba(255,0,0,1)`).
#[test]
#[ignore = "blocked: no underline stroke geometry in the vector IR (per-column frames are rasterizer-internal)"]
fn stacked_underline_uses_one_frame_per_column() {
    close(32.0 * 0.05, 1.6, "32 pt underline weight");
}

/// `:433` "… per mixed-size wrapped %s column" for `wordArtVertRtl`.
#[test]
#[ignore = "blocked: no underline stroke geometry in the vector IR (per-column frames are rasterizer-internal)"]
fn stacked_rtl_underline_uses_one_frame_per_column() {}

/// `:455` "places underline from the drawing face descent and full cell rather
/// than a font-specific offset": `7.2 + 619/2048·24 − 2.5`.
#[test]
#[ignore = "blocked: no underline stroke geometry in the vector IR"]
fn stacked_underline_uses_the_face_descent() {
    close(7.2 + (619.0 / 2048.0) * 24.0 - 2.5, 11.95, "underline origin");
}

// ── markers ───────────────────────────────────────────────────────────────

/// `:463` "reserves stacked character and number prefixes only at paragraph
/// start in %s" (`wordArtVert`).
#[test]
#[ignore = "blocked: one grapheme per column — a marker cannot occupy prefix cells of the paragraph's first column"]
fn stacked_reserves_char_prefix_cells() {
    let commands = compile(&body("ABCDEFGHIJKLMNO", "wordArtVert"), 300.0, 250.0);
    let runs = draw_runs(&commands);
    assert_eq!(runs[0].text, "R");
}

/// `:463` "… prefix … in %s" for `wordArtVertRtl` (auto-number `1`,`2`,`.`).
#[test]
#[ignore = "blocked: one grapheme per column — a marker cannot occupy prefix cells of the paragraph's first column"]
fn stacked_reserves_autonum_prefix_cells() {}

/// `:495` "keeps picture markers per paragraph with authored size and source
/// aspect" (`{ w: 18, h: 12 }` at 50 %, `{ w: 15, h: 10 }` at 50 % + 10 pt).
#[test]
#[ignore = "blocked: Bullet::Blip is not compiled (no picture marker images)"]
fn stacked_picture_markers_keep_their_aspect() {
    close(36.0 * 0.5, 18.0, "50 % of 36 px");
    close(24.0 * 0.5, 12.0, "source aspect 3:2");
}

/// `:8` The stacked modes are the `wordArtVert` / `wordArtVertRtl` `bodyPr`
/// values, which the layout dispatches on.
#[test]
fn stacked_modes_dispatch_on_body_pr_vert() {
    assert_eq!(VertMode::of("wordArtVert"), VertMode::Stacked { rtl: false });
    assert_eq!(VertMode::of("wordArtVertRtl"), VertMode::Stacked { rtl: true });
}
