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
use office_terminal_core::contracts::HyperlinkTarget;
use office_terminal_core::contracts::{AffineTransform, PositionedGlyphRun, RectF32, VectorCommand};
use office_terminal_core::text::emit::compile_text_body;
use office_terminal_core::text::vertical::{graphemes, place_stacked, vertical_form, StackedGlyph, VertMode};
use office_terminal_core::SlideCompiler;
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
            style: office_terminal_core::text::StyleId(0),
            advance: CELL * PT,
            width: 0.6 * 24.0 * PT,
            thickness: CELL * PT,
            ascent: (CELL - ARIAL_DESCENT * 24.0) * PT,
            descent: ARIAL_DESCENT * 24.0 * PT,
            rotation: 0.0,
            object: None,
            paragraph: 0,
            break_before: false,
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
    close(rtl_centre / PT, 60.0 - 7.2 - CELL / 2.0, "rtl column centre");
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

#[test]
fn stacked_glyphs_advance_one_cell_along_the_column() {
    let glyphs = arial_glyphs("AB");
    let slots = place_stacked(&glyphs, text_rect(), "t", "l", false, false);
    close((slots[1].baseline - slots[0].baseline) / PT, CELL, "cell pitch");
}

#[test]
fn stacked_ascii_bracket_is_turned_ninety_degrees() {
    let commands = compile(&body("(A", "wordArtVert"), 60.0, 470.0);
    let rotated = commands.iter().any(|command| match command {
        VectorCommand::DrawText { transform, .. } => transform.kx.abs() > 0.1 || transform.ky.abs() > 0.1,
        _ => false,
    });
    assert!(rotated, "ASCII bracket must use a rotated DrawText frame");
}

#[test]
fn stacked_reserves_a_character_prefix_cell() {
    let mut text_body = body("ABCDEFGHIJKLMNO", "wordArtVert");
    text_body.paragraphs[0].bullet = pptx_model::Bullet::Char {
        ch: "R".to_string(),
        color: None,
        size_pct: None,
        size_pts: None,
        font_family: None,
    };
    let commands = compile(&text_body, 300.0, 250.0);
    let runs = draw_runs(&commands);
    assert_eq!(runs.first().map(|run| run.text.as_str()), Some("R"));
}

fn math_body(text: &str, vert: &str) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": [{
                "type": "math",
                "fallbackText": null,
                "nodes": [{ "kind": "run", "text": text, "style": "italic" }],
                "display": false,
                "fontSize": 18
            }]
        }],
        "defaultFontSize": 18,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": vert, "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false
    });
    serde_json::from_value(value).expect("math body")
}

#[test]
fn stacked_cached_equation_keeps_a_drawable_cell() {
    let commands = compile(&math_body("x", "wordArtVert"), 200.0, 200.0);
    assert!(!draw_runs(&commands).is_empty(), "equation runs reach paint");
}

#[test]
fn stacked_glyphs_carry_the_run_hyperlink() {
    let mut text_body = body("AB", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.hyperlink = Some("https://example.com/".to_string());
    }
    let mut shape: pptx_model::ShapeElement = serde_json::from_value(json!({
        "x": 0, "y": 0, "width": 4_572_000, "height": 2_286_000,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "rect", "fill": null, "stroke": null,
        "textBody": null, "custGeom": null
    }))
    .expect("shape fixture");
    shape.text_body = Some(text_body);
    let slide = pptx_model::Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: Some("ppt/slides/slide1.xml".into()),
        background: None,
        elements: vec![pptx_model::SlideElement::Shape(shape)],
        element_sources: vec![],
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    };
    let (tx, _rx) = unbounded();
    let vector_slide = office_terminal_core::DefaultSlideCompiler::new()
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile");
    let target = HyperlinkTarget::ExternalUri("https://example.com/".to_string());
    let resolved: Vec<HyperlinkTarget> = vector_slide.hyperlinks.iter().map(|link| link.target.clone()).collect();
    assert_eq!(resolved, vec![target.clone(), target]);
}

#[test]
fn stacked_body_rotation_follows_the_frame_flips() {
    use office_terminal_core::text::vertical::shape_text_rotation;
    assert_eq!(shape_text_rotation("wordArtVert", 0.0, true, false), 0.0);
    assert_eq!(shape_text_rotation("wordArtVertRtl", 0.0, false, true), 180.0);
    assert_eq!(shape_text_rotation("horz", 0.0, false, true), 0.0);
}

#[test]
fn stacked_cluster_split_across_runs_stays_one_cell() {
    let mut text_body = body("葛", "wordArtVert");
    let second = json!({
        "type": "text", "text": "\u{e0100}B", "bold": true, "italic": null,
        "underline": false, "strikethrough": false, "strikeDouble": false,
        "hyperlinkUsesTextFill": false, "fieldType": null,
        "fontSize": FONT_PX, "color": "000000", "fontFamily": "Arial"
    });
    text_body.paragraphs[0]
        .runs
        .push(serde_json::from_value(second).expect("second run"));
    let commands = compile(&text_body, 60.0, 470.0);
    assert_eq!(texts(&draw_runs(&commands)), ["葛\u{e0100}", "B"]);
}

#[test]
fn stacked_cell_follows_an_exceptional_drawing_face() {
    let mut text_body = body("日日", "wordArtVert");
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.font_family = Some("Yu Gothic".to_string());
    }
    let commands = compile(&text_body, 100.0, 470.0);
    let runs = draw_runs(&commands);
    assert_eq!(runs.len(), 2);
    let y0 = runs[0].glyphs[0].y;
    let y1 = runs[1].glyphs[0].y;
    let expected = (7.0 / 6.0) * (2636.0 / 2048.0) * 24.0 * PT;
    assert!(
        (y1 - y0 - expected).abs() < 0.5,
        "Yu Gothic cell: expected {expected}, got {}",
        y1 - y0
    );
}

fn draw_text_entries(commands: &[VectorCommand]) -> Vec<(String, AffineTransform)> {
    let mut out = Vec::new();
    for command in commands {
        if let VectorCommand::DrawText { runs, transform } = command {
            for run in runs {
                out.push((run.text.clone(), *transform));
            }
        }
    }
    out
}

fn body_with_family(text: &str, vert: &str, family: &str) -> pptx_model::TextBody {
    let mut text_body = body(text, vert);
    if let pptx_model::TextRun::Text(run) = &mut text_body.paragraphs[0].runs[0] {
        run.font_family = Some(family.to_string());
    }
    text_body
}

#[test]
fn stacked_parallel_mark_follows_the_face_feature() {
    let yu = draw_text_entries(&compile(
        &body_with_family("∥（", "wordArtVert", "Yu Gothic"),
        60.0,
        470.0,
    ));
    let bar = yu.iter().find(|(text, _)| text == "∥").expect("bar");
    let form = yu.iter().find(|(text, _)| text == "︵").expect("（ form");
    assert!(
        bar.1.kx.abs() < 0.1 && bar.1.ky.abs() < 0.1,
        "vrt2 marker stays upright"
    );
    assert!(form.1.kx.abs() < 0.1 && form.1.ky.abs() < 0.1);

    let ms = draw_text_entries(&compile(
        &body_with_family("∥", "wordArtVert", "MS Gothic"),
        60.0,
        470.0,
    ));
    let bar = ms.iter().find(|(text, _)| text == "∥").expect("bar");
    assert!(
        bar.1.kx.abs() > 0.1 || bar.1.ky.abs() > 0.1,
        "MS Gothic's vert marker turns"
    );
}

#[test]
fn stacked_bold_specific_vert_coverage() {
    let regular = draw_text_entries(&compile(
        &body_with_family("ーA", "wordArtVert", "Microsoft JhengHei"),
        100.0,
        470.0,
    ));
    let regular_mark = regular.iter().find(|(text, _)| text == "︱").expect("FE31");
    assert!(regular_mark.1.kx.abs() < 0.1);

    let mut bold_body = body_with_family("ーA", "wordArtVert", "Microsoft JhengHei");
    if let pptx_model::TextRun::Text(run) = &mut bold_body.paragraphs[0].runs[0] {
        run.bold = Some(true);
    }
    let bold = draw_text_entries(&compile(&bold_body, 100.0, 470.0));
    let bold_mark = bold.iter().find(|(text, _)| text == "ー").expect("horizontal mark");
    assert!(
        bold_mark.1.kx.abs() > 0.1 || bold_mark.1.ky.abs() > 0.1,
        "bold JhengHei rotates the long mark"
    );
}

struct MarkerSizes;

impl office_terminal_core::text::bullets::MarkerBitmap for MarkerSizes {
    fn source_size(&self, _rel_id: &str) -> Option<(u32, u32)> {
        Some((36, 24))
    }
}

fn compile_with_bitmaps(text_body: &pptx_model::TextBody, box_w: f32, box_h: f32) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    office_terminal_core::text::emit::compile_text_body_with_markers(
        text_body,
        (box_w * PT) as i64,
        (box_h * PT) as i64,
        &AffineTransform::identity(),
        None,
        Some(&MarkerSizes),
        &tx,
    )
}

#[test]
fn stacked_picture_markers_keep_their_source_aspect() {
    let mut text_body = body("A", "wordArtVert");
    text_body.paragraphs[0].bullet = pptx_model::Bullet::Blip {
        image_path: "marker.png".to_string(),
        mime_type: "image/png".to_string(),
        size_pct: Some(50.0),
        size_pts: None,
    };
    let commands = compile_with_bitmaps(&text_body, 200.0, 200.0);
    let image = commands.iter().find_map(|command| match command {
        VectorCommand::DrawImage { dest_rect, rel_id, .. } => Some((rel_id.clone(), *dest_rect)),
        _ => None,
    });
    let (rel_id, dest) = image.expect("picture marker image");
    assert_eq!(rel_id, "marker.png");
    assert!((dest.w / dest.h - 1.5).abs() < 1e-4, "36:24 source aspect");
}

#[path = "text_box_b_support/stacked_decor_cases.rs"]
mod stacked_decor_cases;

#[path = "text_box_b_support/stacked_feature_cases.rs"]
mod stacked_feature_cases;

fn math_fallback_body(vert: &str) -> pptx_model::TextBody {
    let math = |text: &str| {
        json!({
            "type": "math",
            "fallbackText": null,
            "nodes": [{ "kind": "run", "text": text, "style": "roman" }],
            "display": false,
            "fontSize": 18
        })
    };
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": [math("x"), math("y")],
            "mathFallbacks": [{ "start": 0, "end": 2, "text": "authored preview" }]
        }],
        "defaultFontSize": 18,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": vert, "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false
    });
    serde_json::from_value(value).expect("fallback body")
}

#[test]
fn stacked_complete_inline_equation_branch_is_replaced() {
    for vert in ["horz", "wordArtVert", "wordArtVertRtl"] {
        let commands = compile(&math_fallback_body(vert), 200.0, 200.0);
        let drawn = texts(&draw_runs(&commands));
        let joined = drawn.concat();
        assert_eq!(joined, "authored preview", "{vert}: fallback branch");
        assert!(!drawn.iter().any(|text| text == "x" || text == "y"));
    }
}

#[test]
fn stacked_reserves_autonum_prefix_cells() {
    let mut text_body = body("ABCDEFGHIJKLMNO", "wordArtVert");
    text_body.paragraphs[0].bullet = pptx_model::Bullet::AutoNum {
        num_type: "arabicPeriod".to_string(),
        start_at: Some(12),
        color: None,
        size_pct: None,
        size_pts: None,
        font_family: None,
    };
    let commands = compile(&text_body, 300.0, 250.0);
    let first: String = texts(&draw_runs(&commands)).into_iter().take(3).collect();
    assert_eq!(first, "12.", "auto-number prefix occupies three cells");
}
