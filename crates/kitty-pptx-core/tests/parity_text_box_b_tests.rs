//! Text-box parity (`text-box-b`): list markers.
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/{empty-bullet,buszpts-bullet-size,autonum-bullet-color,
//! rtl-bullet-indent}.test.ts`.
//!
//! Every probe compiles a real `pptx_model::TextBody` through
//! `text::emit::compile_text_body` (the reference's `renderTextBody(ctx, body,
//! 0, 0, BOX_W, BOX_H, SCALE)`) and reads the emitted `DrawText` runs back: the
//! marker `PositionedGlyphRun`'s `font_size_pt`, `color` and `glyphs[0].x` are
//! the reference mock's `font` size, `fillStyle` and `fillText` x.
//! `SCALE = 1/12700` makes "1 pt → 1 px", i.e. one reference px = 12 700 EMU.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;
use serde_json::{json, Value};

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;
/// `MARK_EMU` of `buszpts-bullet-size.test.ts:83` / `rtl-bullet-indent.test.ts:95`:
/// a hanging gutter of 27 px at SCALE.
const MARK_EMU: i64 = 342_900;
const BOX_W: f32 = 600.0;
const BOX_H: f32 = 400.0;

/// One text run, mirroring the specs' `run(text, over)` helper (20 pt Arial).
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

fn break_run() -> Value {
    json!({ "type": "break" })
}

fn math_run() -> Value {
    json!({
        "type": "math",
        "display": false,
        "fontSize": FONT_PX,
        "nodes": [{ "kind": "run", "text": "x", "style": "italic" }],
    })
}

fn char_bullet(ch: &str) -> Value {
    json!({
        "type": "char",
        "char": ch,
        "color": null,
        "sizePct": null,
        "fontFamily": null,
    })
}

fn autonum_bullet(start_at: Option<u32>) -> Value {
    json!({
        "type": "autoNum",
        "numType": "arabicPeriod",
        "startAt": start_at,
        "color": null,
        "sizePct": null,
        "fontFamily": null,
    })
}

/// One paragraph; the specs' `para(runs, bullet, lvl)` plus the hanging-gutter
/// variants (`marL = MARK_EMU`, `indent = -MARK_EMU`).
fn para(
    runs: Vec<Value>,
    bullet: Value,
    alignment: &str,
    mar_l: i64,
    indent: i64,
    rtl: bool,
) -> Value {
    json!({
        "alignment": alignment,
        "marL": mar_l,
        "marR": 0,
        "indent": indent,
        "spaceBefore": null,
        "spaceAfter": null,
        "spaceLine": null,
        "lvl": 0,
        "bullet": bullet,
        "defFontSize": null,
        "defColor": null,
        "defBold": null,
        "defItalic": null,
        "defFontFamily": null,
        "tabStops": [],
        "defTabSz": null,
        "rtl": rtl,
        "eaLnBrk": true,
        "runs": runs,
    })
}

/// The specs' `body(bullet)` fixture: 0 insets, `wrap: 'square'`, `horz`.
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

fn compile(text_body: &pptx_model::TextBody, default_color: Option<&str>) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        (BOX_W * PT) as i64,
        (BOX_H * PT) as i64,
        &AffineTransform::identity(),
        default_color,
        &tx,
    )
}

/// Every emitted run, in draw order.
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

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

/// A single bulleted paragraph in the specs' hanging gutter.
fn bulleted(runs: Vec<Value>, bullet: Value) -> pptx_model::TextBody {
    body(vec![para(runs, bullet, "l", MARK_EMU, -MARK_EMU, false)])
}

/// The marker runs: the text layer labels the auto-number marker with a digit
/// (`markerFill` in `autonum-bullet-color.test.ts:88`).
fn markers(runs: &[PositionedGlyphRun]) -> Vec<PositionedGlyphRun> {
    runs.iter()
        .filter(|run| run.text.chars().any(|ch| ch.is_ascii_digit()))
        .cloned()
        .collect()
}

/// A `buAutoNum` paragraph in the specs' hanging gutter (`para(runs, autoNum)`).
fn numbered(runs: Vec<Value>) -> Value {
    para(runs, autonum_bullet(None), "l", MARK_EMU, -MARK_EMU, false)
}

fn numbered_at(runs: Vec<Value>, start_at: u32) -> Value {
    para(
        runs,
        autonum_bullet(Some(start_at)),
        "l",
        MARK_EMU,
        -MARK_EMU,
        false,
    )
}

/// The reference's `resolveBulletLabel(...)` label sequence: the auto-number
/// markers, in paragraph order, with the painter's trailing gap trimmed.
fn labels(paragraphs: Vec<Value>) -> Vec<String> {
    markers(&draw_runs(&compile(&body(paragraphs), None)))
        .iter()
        .map(|run| run.text.trim_end().to_string())
        .collect()
}

// ── empty-bullet.test.ts — paragraphHasRenderableContent ───────────────────
// `:16` "is true for a non-empty text run".

#[test]
fn renderable_content_true_for_a_non_empty_text_run() {
    let runs = draw_runs(&compile(
        &bulleted(vec![text_run("foo")], char_bullet("•")),
        None,
    ));
    assert_eq!(runs[0].text.trim_end(), "•", "marker first: {runs:?}");
    assert_eq!(runs[1].text, "foo");
}

/// `:21` "is false for no runs at all". The observable is that no marker run is
/// painted for the empty paragraph.
#[test]
#[ignore = "blocked: no text::emit::paragraph_has_renderable_content; an empty paragraph still emits its marker"]
fn renderable_content_false_for_no_runs() {
    let runs = draw_runs(&compile(&bulleted(vec![], char_bullet("•")), None));
    assert!(runs.is_empty(), "no marker drawn: {runs:?}");
}

/// `:26` "is false for a single empty-string text run".
#[test]
#[ignore = "blocked: no text::emit::paragraph_has_renderable_content; an empty paragraph still emits its marker"]
fn renderable_content_false_for_a_single_empty_run() {
    let runs = draw_runs(&compile(
        &bulleted(vec![text_run("")], char_bullet("•")),
        None,
    ));
    assert!(runs.is_empty(), "no marker drawn: {runs:?}");
}

/// `:31` "is false for a paragraph that is only a line break".
#[test]
#[ignore = "blocked: no text::emit::paragraph_has_renderable_content; a <a:br/>-only paragraph still emits its marker"]
fn renderable_content_false_for_a_break_only_paragraph() {
    let runs = draw_runs(&compile(
        &bulleted(vec![break_run()], char_bullet("•")),
        None,
    ));
    assert!(runs.is_empty(), "no marker drawn: {runs:?}");
}

/// `:37` "is true for an equation run even with no text".
#[test]
fn renderable_content_true_for_an_equation_run() {
    let runs = draw_runs(&compile(&bulleted(vec![math_run()], char_bullet("•")), None));
    assert_eq!(runs[0].text.trim_end(), "•", "marker drawn: {runs:?}");
}

/// `:43` "is true when at least one run has text among empties".
#[test]
fn renderable_content_true_for_one_text_run_among_empties() {
    let runs = draw_runs(&compile(
        &bulleted(vec![text_run(""), text_run("x")], char_bullet("•")),
        None,
    ));
    assert_eq!(runs[0].text.trim_end(), "•");
    assert_eq!(runs[1].text, "x");
}

// ── empty-bullet.test.ts — resolveBulletLabel ──────────────────────────────

/// `:78` "suppresses a char bullet on an empty paragraph" (`[]` and `[brk]`
/// both resolve to `''`).
#[test]
#[ignore = "blocked: emit::bullet_label has no empty-paragraph suppression (resolveBulletLabel coupling)"]
fn resolve_bullet_label_suppresses_a_char_bullet_when_empty() {
    for runs in [vec![], vec![break_run()]] {
        let emitted = draw_runs(&compile(&bulleted(runs, char_bullet("•")), None));
        assert!(emitted.is_empty(), "no char marker: {emitted:?}");
    }
}

/// `:86` "suppresses an autoNum marker on an empty paragraph".
#[test]
#[ignore = "blocked: emit::bullet_label has no empty-paragraph suppression (resolveBulletLabel coupling)"]
fn resolve_bullet_label_suppresses_an_autonum_marker_when_empty() {
    let emitted = draw_runs(&compile(&bulleted(vec![], autonum_bullet(None)), None));
    assert!(emitted.is_empty(), "no auto-number marker: {emitted:?}");
}

/// `:130` "still draws a char bullet when the paragraph has text" (`'•'`).
#[test]
fn resolve_bullet_label_keeps_the_char_bullet_with_text() {
    let emitted = draw_runs(&compile(
        &bulleted(vec![text_run("foo")], char_bullet("•")),
        None,
    ));
    assert_eq!(emitted[0].text.trim_end(), "•");
}

/// `:110` "numbers consecutive non-empty paragraphs 1, 2, 3" → `['1.','2.','3.']`.
#[test]
fn resolve_bullet_label_numbers_consecutive_paragraphs() {
    let paragraphs = ["a", "b", "c"]
        .iter()
        .map(|text| {
            numbered(vec![text_run(text)])
        })
        .collect();
    assert_eq!(labels(paragraphs), ["1.", "2.", "3."]);
}

/// `:120` "continues the sequence across an empty line (1. / blank / 2.)": the
/// blank paragraph resolves to `''` and the sequence continues at `'2.'`, not
/// `'3.'`.
#[test]
#[ignore = "blocked: bullet numbering is paragraph-index based, no per-level counter that skips empties"]
fn resolve_bullet_label_continues_across_an_empty_line() {
    let paragraphs = vec![
        numbered(vec![text_run("foo")]),
        numbered(vec![]),
        numbered(vec![text_run("bar")]),
    ];
    assert_eq!(labels(paragraphs), ["1.", "2."]);
}

/// `:131` "continues the sequence across a line-break-only paragraph"
/// (`['1.', '', '2.']`).
#[test]
#[ignore = "blocked: bullet numbering is paragraph-index based, no per-level counter that skips empties"]
fn resolve_bullet_label_continues_across_a_break_only_paragraph() {
    let paragraphs = vec![
        numbered(vec![text_run("foo")]),
        numbered(vec![break_run()]),
        numbered(vec![text_run("bar")]),
    ];
    assert_eq!(labels(paragraphs), ["1.", "2."]);
}

/// `:141` "honours startAt and keeps numbering per level" (`startAt = 5` →
/// `'5.'`, `'6.'`).
#[test]
fn resolve_bullet_label_honours_start_at() {
    let paragraphs = ["a", "b"]
        .iter()
        .map(|text| {
            numbered_at(vec![text_run(text)], 5)
        })
        .collect();
    assert_eq!(labels(paragraphs), ["5.", "6."]);
}

/// `:152` "resets the counter when a non-list paragraph appears between items"
/// (the heading's `bullet: none` restarts at `'1.'`).
#[test]
#[ignore = "blocked: bullet numbering is paragraph-index based, no counter reset on a non-list paragraph"]
fn resolve_bullet_label_resets_after_a_non_list_paragraph() {
    let paragraphs = vec![
        numbered(vec![text_run("a")]),
        para(
            vec![text_run("heading")],
            json!({ "type": "none" }),
            "l",
            MARK_EMU,
            -MARK_EMU,
            false,
        ),
        numbered(vec![text_run("b")]),
    ];
    assert_eq!(labels(paragraphs), ["1.", "1."]);
}

// ── buszpts-bullet-size.test.ts (§21.1.2.4.10) ─────────────────────────────

fn char_bullet_sized(size_pts: Option<f64>, size_pct: Option<f64>) -> Value {
    json!({
        "type": "char",
        "char": "◆",
        "color": null,
        "sizePct": size_pct,
        "sizePts": size_pts,
        "fontFamily": null,
    })
}

/// The marker fill is the one whose text is the bullet glyph
/// (`markerPx`, `:119`).
fn marker_size_pt(bullet: Value) -> f32 {
    let runs = draw_runs(&compile(&bulleted(vec![text_run("item")], bullet), None));
    let marker = runs
        .iter()
        .find(|run| run.text.contains('◆'))
        .unwrap_or_else(|| panic!("char marker drawn: {runs:?}"));
    marker.font_size_pt
}

/// `:99` "uses the absolute point size (× scale), NOT the run size": buSzPts =
/// 40 pt with a 20 pt run → the marker paints at 40 px = 40 pt here.
#[test]
#[ignore = "blocked: emit::build_paragraphs ignores Bullet::Char::size_pts (the marker is always the paragraph size)"]
fn bu_sz_pts_sizes_the_marker_absolutely() {
    close(marker_size_pt(char_bullet_sized(Some(40.0), None)), 40.0, "buSzPts 40");
}

/// `:113` "buSzPts wins over a co-present buSzPct": 30 pt absolute beats
/// 200 % × 20 pt = 40 px.
#[test]
#[ignore = "blocked: emit::build_paragraphs ignores Bullet::Char::size_pts / size_pct"]
fn bu_sz_pts_wins_over_bu_sz_pct() {
    close(
        marker_size_pt(char_bullet_sized(Some(30.0), Some(200.0))),
        30.0,
        "buSzPts over buSzPct",
    );
}

/// `:120` "falls back to buSzPct × run size when no buSzPts is present":
/// 200 % of the 20 pt run = 40 px.
#[test]
#[ignore = "blocked: emit::build_paragraphs ignores Bullet::Char::size_pct"]
fn bu_sz_pct_falls_back_to_the_run_size() {
    close(
        marker_size_pt(char_bullet_sized(None, Some(200.0))),
        40.0,
        "buSzPct 200%",
    );
}

/// `:127` "falls back to the run size when neither buSzPts nor buSzPct is
/// present": the marker inherits the 20 pt first-run size.
#[test]
fn marker_inherits_the_run_size_without_bu_sz() {
    close(marker_size_pt(char_bullet_sized(None, None)), 20.0, "run size");
}

// ── autonum-bullet-color.test.ts (buClr, §21.1.2.4.4) ──────────────────────

fn autonum_styled(color: Option<&str>, size_pct: Option<f64>, family: Option<&str>) -> Value {
    json!({
        "type": "autoNum",
        "numType": "arabicPeriod",
        "startAt": null,
        "color": color,
        "sizePct": size_pct,
        "fontFamily": family,
    })
}

/// `:103` "uses the explicit buClr colour, not the inherited first-run colour":
/// buClr = C00000 (red) while the first run is blue → `rgba(192,0,0,1)`.
#[test]
#[ignore = "blocked: emit::build_paragraphs ignores Bullet::AutoNum::color (buClr §21.1.2.4.4)"]
fn autonum_marker_uses_the_explicit_bu_clr() {
    let runs = draw_runs(&compile(
        &bulleted(
            vec![text_run("item")],
            autonum_styled(Some("C00000"), None, None),
        ),
        Some("0000FF"),
    ));
    let marker = markers(&runs);
    assert_eq!(marker[0].color, [192, 0, 0, 255], "buClr red");
}

/// `:113` "falls back to the first-run colour when no buClr is present
/// (buClrTx)": the marker inherits the blue first-run colour.
#[test]
fn autonum_marker_inherits_the_text_colour_without_bu_clr() {
    let runs = draw_runs(&compile(
        &bulleted(vec![text_run("item")], autonum_styled(None, None, None)),
        Some("0000FF"),
    ));
    let marker = markers(&runs);
    assert_eq!(marker[0].color, [0, 0, 255, 255], "inherited blue");
}

/// `:122` "uses the authored bullet typeface and percentage size": buSzPct 50
/// of a 20 pt run → a `10px` marker in the authored `Calibri` face.
#[test]
#[ignore = "blocked: the marker face is hard-coded to Carlito and size_pct is ignored in emit::build_paragraphs"]
fn autonum_marker_uses_the_bullet_face_and_percentage_size() {
    let runs = draw_runs(&compile(
        &bulleted(
            vec![text_run("item")],
            autonum_styled(None, Some(50.0), Some("Calibri")),
        ),
        Some("0000FF"),
    ));
    let marker = markers(&runs);
    close(marker[0].font_size_pt, 10.0, "buSzPct 50 of 20 pt");
    assert_eq!(marker[0].font_family, "Carlito", "Calibri → Carlito");
}

// ── rtl-bullet-indent.test.ts (issue #930) ─────────────────────────────────

/// The RTL leading edge: box 600 px, no insets, `marR = 0`, so
/// `textX + textMaxW = 600` (`:139`).
const LEADING_EDGE: f32 = BOX_W;

fn rtl_body(bullet: Value, text: &str, rtl: bool) -> pptx_model::TextBody {
    body(vec![para(
        vec![text_run(text)],
        bullet,
        if rtl { "r" } else { "l" },
        MARK_EMU,
        -MARK_EMU,
        rtl,
    )])
}

/// `:136` "char bullet: marker RIGHT edge sits on the leading edge, text
/// reserved to its left": `markerRight ≈ 600` and `textRight ≈ marker.x`.
#[test]
#[ignore = "blocked: RTL list markers are inline line-prefix text with no right-edge anchor (issue #930)"]
fn rtl_char_marker_leads_at_the_right_edge() {
    let runs = draw_runs(&compile(&rtl_body(char_bullet("•"), "ابح", true), None));
    let marker = runs
        .iter()
        .find(|run| run.text.contains('•'))
        .expect("bullet marker drawn");
    let marker_w: f32 = marker.glyphs.iter().map(|glyph| glyph.advance).sum();
    close(
        marker.glyphs[0].x + marker_w,
        LEADING_EDGE * PT,
        "marker right edge",
    );
    let text_right = runs
        .iter()
        .filter(|run| !run.text.contains('•'))
        .filter_map(|run| run.glyphs.last().map(|glyph| glyph.x + glyph.advance))
        .fold(f32::MIN, f32::max);
    close(text_right, marker.glyphs[0].x, "text right edge");
    assert!(text_right < LEADING_EDGE * PT, "text stays inside the box");
}

/// `:157` "autoNum marker: also leads at the right edge (not pushed into the
/// margin)".
#[test]
#[ignore = "blocked: RTL list markers are inline line-prefix text with no right-edge anchor (issue #930)"]
fn rtl_autonum_marker_leads_at_the_right_edge() {
    let runs = draw_runs(&compile(&rtl_body(autonum_bullet(None), "ابح", true), None));
    let marker = markers(&runs);
    let marker_w: f32 = marker[0].glyphs.iter().map(|glyph| glyph.advance).sum();
    close(
        marker[0].glyphs[0].x + marker_w,
        LEADING_EDGE * PT,
        "autoNum marker right edge",
    );
}

/// `:167` "LTR control: a char bullet still hangs at the left gutter
/// (byte-identical)": `bulletX = textX + indent = marL − |indent| = 0`.
#[test]
fn ltr_char_marker_still_hangs_at_the_left_gutter() {
    let commands = compile(&rtl_body(char_bullet("•"), "abc", false), None);
    let runs = draw_runs(&commands);
    let marker = runs
        .iter()
        .find(|run| run.text.contains('•'))
        .expect("bullet marker drawn");
    close(marker.glyphs[0].x, 0.0, "marker x");
    // The paragraph is not mirrored: the element transform stays the identity
    // (`marker.direction !== 'rtl'`).
    let transform = commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawText { transform, .. } => Some(*transform),
            _ => None,
        })
        .expect("DrawText command");
    assert_eq!(transform, AffineTransform::identity());
}
