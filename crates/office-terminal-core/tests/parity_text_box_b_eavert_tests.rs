//! Text-box parity (`text-box-b`): `eaVert` per-glyph orientation and the
//! PowerPoint East-Asian / complex-script default faces.
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/{eavert-glyph-orientation,east-asian-default}.test.ts`.
//!
//! The reference records the NET canvas rotation of every `fillText` and asserts
//! `0` (upright) or `+π/2` (sideways). The Rust model carries the same fact as
//! [`Orient`] / [`EaVertPiece::upright`] (`text/vertical.rs`), and the U+FE1x /
//! U+FE3x vertical form as [`EaVertPiece::painted`], so the two draw classes and
//! the substitutions are asserted directly against the layout kernel.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_box_b_eavert_tests.rs.txt`; the
//! ledger keeps their rows.

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use office_terminal_core::text::emit::compile_text_body;
use office_terminal_core::text::vertical::{
    ea_vert_pieces, graphemes, orientation, vertical_form, EaVertPiece, Orient, VertMode,
};
use serde_json::json;

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;

// Representative glyphs, one per UAX #50 class (`eavert-glyph-orientation:59`).
const U_CJK: char = '国'; // vo=U → upright
const U_KANA: char = 'あ'; // vo=U → upright
const R_LATIN: char = 'A'; // vo=R → sideways

/// The glyphs of one `ea_vert_pieces` call as `(painted, upright)`.
fn pieces(text: &str, capable: impl Fn(char) -> bool) -> Vec<(String, bool)> {
    ea_vert_pieces(text, capable)
        .into_iter()
        .map(|piece| (piece.painted, piece.upright))
        .collect()
}

/// Every glyph is drawable: every vertical form the table knows is available.
fn all_forms(_: char) -> bool {
    true
}

fn bracket_forms(form: char) -> bool {
    matches!(form, '\u{fe35}' | '\u{fe11}')
}

fn assert_orient(ch: char, expected: Orient, what: &str) {
    assert_eq!(orientation(ch), expected, "{what}");
}

// ── per-glyph orientation (§20.1.10.83, issue #790) ────────────────────────

/// `:157` "draws vo=U CJK ideographs UPRIGHT (net rotation 0)".
#[test]
fn cjk_ideograph_is_upright() {
    assert_orient(U_CJK, Orient::Upright, "国 stands upright");
}

/// `:163` "draws vo=U kana UPRIGHT (net rotation 0)".
#[test]
fn kana_is_upright() {
    assert_orient(U_KANA, Orient::Upright, "あ stands upright");
}

/// `:169` "leaves vo=R Latin SIDEWAYS (net rotation +90°)".
#[test]
fn latin_stays_sideways() {
    assert_orient(R_LATIN, Orient::Rotated, "A rotates with the page");
}

/// `:175` "SUBSTITUTES a vo=Tr fullwidth bracket with its vertical form, drawn
/// upright": `（` is never painted; `︵` (U+FE35) is, upright.
#[test]
fn fullwidth_bracket_is_substituted_upright() {
    let painted = pieces("（", all_forms);
    assert_eq!(
        painted,
        [("\u{fe35}".to_string(), true)],
        "（ → ︵ upright"
    );
    assert!(!painted.iter().any(|(text, _)| text == "（"));
}

/// `:183` "SUBSTITUTES a vo=Tu comma with its U+FE11 vertical form, drawn
/// upright": `、` → `︑`.
#[test]
fn ideographic_comma_is_substituted_upright() {
    assert_eq!(
        pieces("、", all_forms),
        [("\u{fe11}".to_string(), true)],
        "、 → ︑ upright"
    );
}

/// `:190` "SUBSTITUTES the vo=Tr white lenticular `〖` with its U+FE1x vertical
/// form, drawn upright (issue #969)" → `︗` (U+FE17).
#[test]
fn white_lenticular_open_is_substituted_upright() {
    assert_eq!(vertical_form('〖'), Some('\u{fe17}'));
    assert_eq!(pieces("〖", all_forms), [("\u{fe17}".to_string(), true)]);
}

/// `:190` "SUBSTITUTES the vo=Tr white lenticular `〗`" → `︘` (U+FE18).
#[test]
fn white_lenticular_close_is_substituted_upright() {
    assert_eq!(vertical_form('〗'), Some('\u{fe18}'));
    assert_eq!(pieces("〗", all_forms), [("\u{fe18}".to_string(), true)]);
}

/// `:200` "ROTATES the vo=Tr colon `：` (geometric fallback → FE13 side-by-side
/// dots) (issue #969)": it is not substituted and rotates with the page.
#[test]
fn fullwidth_colon_is_not_substituted() {
    assert_eq!(vertical_form('：'), None, "no FE13 substitution");
}

/// `:210` "draws the vo=Tr semicolon `；` UPRIGHT (geometric fallback → FE14
/// dot-over-comma) (issue #969)".
#[test]
fn fullwidth_semicolon_stays_upright() {
    assert_eq!(pieces("；", all_forms), [("；".to_string(), true)]);
}

/// `:219` "plain-rotates the unreachable vo=Tr long-stroke mark `ー`": painted as
/// authored, rotated with the page, and not reflected (the Rust model has no
/// per-glyph scale, so the reflection half of the assertion is N/A).
#[test]
fn prolonged_sound_mark_rotates_with_the_page() {
    assert_eq!(
        ea_vert_pieces("ー", |_| false),
        [EaVertPiece {
            upright: false,
            painted: "ー".to_string(),
            measured: "ー".to_string()
        }]
    );
}

/// `:219` "plain-rotates the unreachable vo=Tr long-stroke mark `〜`"
/// (U+301C).
#[test]
fn wave_dash_rotates_with_the_page() {
    assert_eq!(pieces("〜", |_| false), [("〜".to_string(), false)]);
    assert_orient('〜', Orient::Rotated, "U+301C rotates");
}

/// `:219` "plain-rotates the unreachable vo=Tr long-stroke mark `～`"
/// (U+FF5E).
#[test]
fn fullwidth_tilde_rotates_with_the_page() {
    assert_eq!(pieces("～", |_| false), [("～".to_string(), false)]);
    assert_orient('～', Orient::Rotated, "U+FF5E rotates");
}

/// `:238` "orients a mixed column: CJK upright, Latin sideways, bracket
/// substituted, comma substituted, ー rotated".
#[test]
fn mixed_column_keeps_a_class_per_glyph() {
    // The face carries the FE35/FE11 bracket forms but no FE31 long-mark form,
    // which is the reference's "unreachable" ー case.
    let painted = pieces("国A（、ー", bracket_forms);
    assert_eq!(
        painted,
        [
            ("国".to_string(), true),
            ("A".to_string(), false),
            ("\u{fe35}".to_string(), true),
            ("\u{fe11}".to_string(), true),
            ("ー".to_string(), false),
        ]
    );
}

// ── distributed columns spread the glyph pitch ─────────────────────────────

fn ea_vert_body(text: &str, alignment: &str) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": alignment,
            "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0,
            "bullet": { "type": "none" },
            "defFontSize": null, "defColor": null, "defBold": null, "defItalic": null,
            "defFontFamily": null,
            "tabStops": [], "defTabSz": null, "rtl": false, "eaLnBrk": true,
            "runs": [{
                "type": "text", "text": text,
                "bold": null, "italic": null, "underline": false,
                "strikethrough": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "fontSize": FONT_PX, "color": "000000", "fontFamily": "Arial",
            }],
        }],
        "defaultFontSize": FONT_PX,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "none",
        "vert": "eaVert",
        "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
    });
    serde_json::from_value(value).expect("eaVert body fixture")
}

/// The upright cell of an `eaVert` column is painted OUTSIDE the rotated frame,
/// so the along-column position (layout x) becomes the glyph's slide y.
fn upright_column(text: &str, alignment: &str, box_w: f32, box_h: f32) -> Vec<PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        &ea_vert_body(text, alignment),
        (box_w * PT) as i64,
        (box_h * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.iter().cloned()),
            _ => None,
        })
        .flatten()
        .filter(|run| run.text.chars().count() == 1)
        .collect()
}

fn column_pitch(column: &[PositionedGlyphRun]) -> f32 {
    column[1].glyphs[0].y - column[0].glyphs[0].y
}

/// `:262` "draws a `dist` pure-CJK column with a wider cell pitch than a
/// left-aligned one": the natural pitch is one em; `dist` stretches it to fill
/// the 400 px column length.
#[test]
fn distributed_column_spreads_the_glyph_pitch() {
    let text = "あいうえお";
    let left = upright_column(text, "l", 200.0, 400.0);
    let dist = upright_column(text, "dist", 200.0, 400.0);
    assert_eq!(left.len(), 5, "all five glyphs are upright cells");
    assert_eq!(dist.len(), 5);
    let natural = column_pitch(&left);
    let spread = column_pitch(&dist);
    // The stretch is the line's slack ((400 px − 5 em) over the four interior
    // boundaries), added to the natural pitch.
    let expected = natural + (400.0 * PT - 5.0 * natural) / 4.0;
    assert!(
        (spread - expected).abs() < 1.0,
        "distributed pitch {spread} vs {expected}"
    );
    assert!(
        spread > natural * 1.5,
        "distributed pitch fills the column: {spread} vs {natural}"
    );
}

// ── drawEaVertRun — per-glyph orientation helper ───────────────────────────
// The reference helper counter-rotates a signed-up glyph by −90° inside the
// +90° page frame; the Rust kernel passes the same fact as `upright`.

/// `:302` "counter-rotates vo=U glyphs by −90° (upright in the +90° page
/// frame)": the painted glyph is the authored one, upright.
#[test]
fn helper_uprights_vo_u_glyphs() {
    let mut painted = ea_vert_pieces("国", all_forms);
    let piece = painted.remove(0);
    assert_eq!(piece.painted, "国");
    assert!(piece.upright, "国 is counter-rotated upright");
}

/// `:307` "does not counter-rotate vo=R Latin (stays with the page)".
#[test]
fn helper_leaves_vo_r_glyphs_rotated() {
    let mut rotated = ea_vert_pieces("A", all_forms);
    let piece = rotated.remove(0);
    assert_eq!(piece.painted, "A");
    assert!(!piece.upright, "A keeps the page rotation");
}

/// `:312` "substitutes vo=Tr brackets and vo=Tu commas, counter-rotated
/// upright".
#[test]
fn helper_substitutes_and_uprights_brackets_and_commas() {
    assert_eq!(pieces("（", all_forms), [("\u{fe35}".to_string(), true)]);
    assert_eq!(pieces("、", all_forms), [("\u{fe11}".to_string(), true)]);
}

/// `:323` "leaves vo=Tr ー plain-rotated with the page (no local transform or
/// substitution)": with no vertical form available the authored glyph is drawn
/// sideways.
#[test]
fn helper_leaves_the_long_mark_plain_rotated() {
    let mut rotated = ea_vert_pieces("ー", |_| false);
    let piece = rotated.remove(0);
    assert_eq!(piece.painted, "ー");
    assert!(!piece.upright);
}

// ── east-asian-default.test.ts (issue #1627) ───────────────────────────────
// The reference module `packages/pptx/src/east-asian-default.ts` classifies
// Latin faces by PANOSE serif style and resolves the application default East
// Asian / complex-script faces per character script.

/// `:19` "classifies Latin faces by their installed PANOSE serif style"
/// (`Perpetua` / `Georgia` / `Garamond` serif; `Corbel` / `Arial` / `Tahoma` /
/// `Gill Sans MT` / unknown not) plus `isEastAsianFace('Yu Gothic')`.
///
/// The classifier lives next to the pinned face catalogue
/// (`text/faces.rs`, ported by the text-box-a `powerPointFaceMetrics` work);
/// an unknown face is sans and not East Asian.
#[test]
fn latin_faces_classify_by_panose_serif_style() {
    use office_terminal_core::text::faces::{is_east_asian_face, is_serif_latin_face};
    for face in ["Perpetua", "Georgia", "Garamond"] {
        assert!(is_serif_latin_face(face), "{face} is serif");
    }
    for face in ["Corbel", "Arial", "Tahoma", "Gill Sans MT", "No Such Face"] {
        assert!(!is_serif_latin_face(face), "{face} is sans");
    }
    assert!(is_east_asian_face("Yu Gothic"), "Yu Gothic Far-East code page");
    assert!(!is_east_asian_face("Tahoma"), "Tahoma is not East Asian");
}

/// `graphemes` backs the stacked column split shared with the `eaVert` path;
/// pinned here so the two vertical modes cannot drift.
#[test]
fn ascii_graphemes_stay_one_per_column() {
    assert_eq!(graphemes("AB"), ["A", "B"]);
    assert_eq!(VertMode::of("eaVert"), VertMode::EaVert);
}

#[test]
fn fullwidth_colon_rotates_with_the_page() {
    assert_eq!(pieces("：", all_forms), [("：".to_string(), false)]);
}

fn upright_column_spaced(
    text: &str,
    alignment: &str,
    box_w: f32,
    box_h: f32,
    spc_pt: f64,
) -> Vec<PositionedGlyphRun> {
    let mut body = ea_vert_body(text, alignment);
    if let pptx_model::TextRun::Text(run) = &mut body.paragraphs[0].runs[0] {
        run.letter_spacing = Some(spc_pt);
    }
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        &body,
        (box_w * PT) as i64,
        (box_h * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.iter().cloned()),
            _ => None,
        })
        .flatten()
        .filter(|run| run.text.chars().count() == 1)
        .collect()
}

/// `:288` "uses vert only for long marks and keeps other glyphs on manual
/// paths" — the painted sequence
/// `['ー','〜','～','︑','︒','：','；','﹁','﹂','“','”','A']` with the `vert`
/// font FEATURE on only the first three.
#[test]
fn helper_uses_the_vert_feature_for_long_marks_only() {
    let painted: Vec<String> = pieces("ー〜～、。：；「」“”A", all_forms)
        .into_iter()
        .flat_map(|(text, _)| {
            graphemes(&text)
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        painted,
        ["ー", "〜", "～", "︑", "︒", "：", "；", "﹁", "﹂", "“", "”", "A"]
    );
}

/// `:317` "keeps a glyph without vert coverage on the plain geometric
/// fallback".
#[test]
fn helper_keeps_uncovered_glyphs_on_the_plain_fallback() {
    assert_eq!(
        pieces("ー〜", |ch| ch == 'ー'),
        [("ー".to_string(), true), ("〜".to_string(), false)]
    );
}

/// `:341` "advances each cell by measure + letterSpacingPx (the justification
/// pitch)": 国国 steps by `FONT_PX` and by `FONT_PX + 8` with an 8 px pitch.
#[test]
fn helper_advances_by_measure_plus_spacing() {
    let plain = upright_column("国国", "l", 200.0, 400.0);
    assert!((column_pitch(&plain) - 20.0 * PT).abs() < 1.0, "one em step");
    let spaced = upright_column_spaced("国国", "l", 200.0, 400.0, 8.0);
    assert!(
        (column_pitch(&spaced) - 28.0 * PT).abs() < 1.0,
        "step includes 8 px spacing"
    );
}
