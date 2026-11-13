//! Font-slot parity: PowerPoint's application-default East Asian / complex
//! script faces and the `a:ea` / `a:cs` per-cluster slot dispatch.
//!
//! Ported from `packages/pptx/src/east-asian-default.test.ts` (issue #1627,
//! #1653) and the `a:cs` half of `packages/pptx/src/font-requests.test.ts`
//! (reference read-only @ `1f5c6e1d`). Split out of `parity_fonts_tests.rs` to
//! keep both files under the 600-line limit; the coverage ledger
//! (`tests/parity/coverage/fonts.md`) names both targets.
//!
//! The reference asserts on the resolved CSS `font` string of each segment; the
//! port carries one concrete family per run, so every case asserts
//! `PositionedGlyphRun::font_family`. A face the port substitutes
//! (`resolve_font_family`) is asserted by its metric face — `Arial` is the only
//! application default that is substituted (`Liberation Sans`).

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::east_asian_default::{
    complex_script_default_face, east_asian_default_faces, is_east_asian_face,
    is_east_asian_slot_symbol, is_serif_latin_face,
};
use kitty_pptx_core::text::emit::compile_text_body;

const FONT_PX: f64 = 20.0;
const BOX_W: i64 = 3_200_400;
const BOX_H: i64 = 2_743_200;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// One authored run: text plus the three face slots and `rPr@lang`.
struct RunSpec<'a> {
    text: &'a str,
    latin: Option<&'a str>,
    ea: Option<&'a str>,
    cs: Option<&'a str>,
    lang: Option<&'a str>,
}

fn run_spec<'a>(text: &'a str, latin: Option<&'a str>) -> RunSpec<'a> {
    RunSpec {
        text,
        latin,
        ea: None,
        cs: None,
        lang: None,
    }
}

fn run_value(spec: &RunSpec<'_>) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "text": spec.text,
        "bold": serde_json::Value::Null,
        "italic": serde_json::Value::Null,
        "underline": false,
        "strikethrough": false,
        "strikeDouble": false,
        "hyperlinkUsesTextFill": false,
        "fieldType": serde_json::Value::Null,
        "fontSize": FONT_PX,
        "color": "000000",
        "fontFamily": spec.latin,
        "fontFamilyEa": spec.ea,
        "fontFamilyCs": spec.cs,
        "lang": spec.lang,
    })
}

/// One paragraph holding `runs`, over exactly the fields
/// `text::emit::build_paragraphs` reads.
fn body_value(runs: &[RunSpec<'_>]) -> serde_json::Value {
    let runs: Vec<serde_json::Value> = runs.iter().map(run_value).collect();
    serde_json::json!({
        "verticalAnchor": "t",
        "defaultFontSize": FONT_PX,
        "defaultBold": serde_json::Value::Null,
        "defaultItalic": serde_json::Value::Null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square",
        "vert": "horz",
        "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
        "anchorCtr": false,
        "paragraphs": [{
            "alignment": "l",
            "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": serde_json::Value::Null,
            "spaceAfter": serde_json::Value::Null,
            "spaceLine": serde_json::Value::Null,
            "lvl": 0,
            "bullet": {"type": "none"},
            "defFontSize": FONT_PX,
            "defColor": serde_json::Value::Null,
            "defBold": serde_json::Value::Null,
            "defItalic": serde_json::Value::Null,
            "defFontFamily": serde_json::Value::Null,
            "tabStops": [],
            "defTabSz": serde_json::Value::Null,
            "rtl": false,
            "eaLnBrk": true,
            "runs": runs,
        }],
    })
}

fn body(runs: &[RunSpec<'_>]) -> pptx_model::TextBody {
    serde_json::from_value(body_value(runs)).expect("text body fixture")
}

fn runs_of(commands: &[VectorCommand]) -> Vec<PositionedGlyphRun> {
    let mut out = Vec::new();
    for command in commands {
        if let VectorCommand::DrawText { runs, .. } = command {
            out.extend(runs.iter().cloned());
        }
    }
    out
}

fn compile_runs(text_body: &pptx_model::TextBody) -> Vec<PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    runs_of(&compile_text_body(
        text_body,
        BOX_W,
        BOX_H,
        &AffineTransform::identity(),
        None,
        &tx,
    ))
}

/// `(text, family)` of every painted run, in paint order.
fn faces(text_body: &pptx_model::TextBody) -> Vec<(String, String)> {
    compile_runs(text_body)
        .into_iter()
        .map(|run| (run.text, run.font_family))
        .collect()
}

/// The family of the one run whose text contains `needle`.
fn face_of(runs: &[(String, String)], needle: &str) -> String {
    runs.iter()
        .find(|(text, _)| text.contains(needle))
        .map(|(_, family)| family.clone())
        .unwrap_or_else(|| panic!("no run painted {needle:?}: {runs:?}"))
}

// ---------------------------------------------------------------------------
// east-asian-default.test.ts — PowerPoint application default faces
// ---------------------------------------------------------------------------

/// `:19` classifies Latin faces by their installed PANOSE serif style, and
/// knows which faces declare an OS/2 Far-East code page.
#[test]
fn latin_faces_classify_by_panose_serif_style() {
    for face in ["Perpetua", "Georgia", "Garamond"] {
        assert!(is_serif_latin_face(face), "{face} is serif");
    }
    for face in ["Corbel", "Arial", "Tahoma", "Gill Sans MT", "No Such Face"] {
        assert!(!is_serif_latin_face(face), "{face} is sans");
    }
    assert!(is_east_asian_face("Yu Gothic"));
    assert!(!is_east_asian_face("Tahoma"));
}

/// `:29` uses the first tier face that covers the whole East Asian text.
#[test]
fn east_asian_default_face_covers_the_whole_text() {
    let cases = [
        ("Perpetua", "日本語かな", "MS Mincho"),
        ("Perpetua", "繁體中文", "MS Mincho"),
        ("Perpetua", "简体中文", "PMingLiU"),
        ("Perpetua", "한국어", "Batang"),
        ("Corbel", "日本語かな", "MS Gothic"),
        ("Corbel", "简体中文", "Microsoft JhengHei"),
        ("Corbel", "한국어", "Malgun Gothic"),
        ("No Such Face", "日本語かな", "MS Gothic"),
    ];
    for (latin, text, expected) in cases {
        assert_eq!(
            east_asian_default_faces(Some(latin), text).first().map(String::as_str),
            Some(expected),
            "{latin} / {text}"
        );
    }
}

/// `:41` draws East Asian text in a Latin face that is itself East Asian.
#[test]
fn east_asian_latin_face_keeps_its_own_stack() {
    assert_eq!(
        east_asian_default_faces(Some("Yu Gothic"), "日本語かな"),
        ["Yu Gothic", "Microsoft JhengHei", "Malgun Gothic"]
    );
    assert_eq!(
        east_asian_default_faces(Some("SimSun"), "한국어"),
        ["SimSun", "PMingLiU", "Batang"]
    );
}

/// `:48` picks the complex-script default per character script.
#[test]
fn complex_script_default_face_follows_the_script() {
    assert_eq!(complex_script_default_face('ע'), "Arial");
    assert_eq!(complex_script_default_face('ع'), "Arial");
    assert_eq!(complex_script_default_face('ภ'), "Angsana New");
    assert_eq!(complex_script_default_face('ह'), "Mangal");
    assert_eq!(complex_script_default_face('த'), "Latha");
}

/// `:80` draws an empty East Asian slot in the application default, not the
/// Latin face: `Hxg 日本語` splits into a Corbel run and an
/// `MS Gothic, Microsoft JhengHei, Malgun Gothic` run.
#[test]
fn empty_east_asian_slot_uses_the_application_default() {
    let runs = faces(&body(&[run_spec("Hxg 日本語", Some("Corbel"))]));
    assert_eq!(face_of(&runs, "Hxg"), "Corbel");
    assert_eq!(face_of(&runs, "日本語"), "MS Gothic");
}

/// `:88` keeps a resolved East Asian face and splits complex scripts by their
/// script default (`Yu Mincho` for 日本, Arabic in `Arial`, Thai in
/// `Angsana New`).
#[test]
fn resolved_east_asian_face_is_kept_and_complex_scripts_split() {
    let mut spec = run_spec("日本 עב ไท", Some("Corbel"));
    spec.ea = Some("Yu Mincho");
    let runs = faces(&body(&[spec]));
    assert_eq!(face_of(&runs, "日本"), "Yu Mincho");
    // `Arial` is the port's substituted face (`resolve_font_family`).
    assert_eq!(face_of(&runs, "עב"), "Liberation Sans");
    assert_eq!(face_of(&runs, "ไท"), "Angsana New");
}

/// `:97` draws symbols and CJK-language curly quotes in a resolved East Asian
/// face (#1653), and keeps the Latin-slot characters in the Latin face.
#[test]
fn east_asian_face_claims_symbols_and_curly_quotes() {
    let face = |text: &str, lang: &str, ea: Option<&str>| {
        let mut spec = run_spec(text, Some("Corbel"));
        spec.ea = ea;
        spec.lang = Some(lang);
        face_of(&faces(&body(&[spec])), text)
    };
    for symbol in [
        "§", "¨", "°", "±", "´", "×", "÷", "⁇", "①", "⓿", "■", "◯", "★", "☀", "✓", "➔", "︵",
    ] {
        assert_eq!(face(symbol, "en-US", Some("Yu Gothic")), "Yu Gothic", "{symbol}");
    }
    // × ÷ ⁇ ⁈ stay Latin in the measured complex-script languages; § does not.
    for lang in ["ar-SA", "he-IL", "hi-IN", "th-TH"] {
        assert_eq!(face("×", lang, Some("Yu Gothic")), "Corbel", "{lang}");
        assert_eq!(face("§", lang, Some("Yu Gothic")), "Yu Gothic", "{lang}");
    }
    assert_eq!(face("“", "ja-JP", Some("Yu Gothic")), "Yu Gothic");
    assert_eq!(face("„", "zh-CN", Some("Yu Gothic")), "Yu Gothic");
    // Latin under en-US, U+201F in every language, and Latin-slot symbols.
    assert_eq!(face("“", "en-US", Some("Yu Gothic")), "Corbel");
    assert_eq!(face("‟", "ja-JP", Some("Yu Gothic")), "Corbel");
    for symbol in ["¶", "…", "→", "─", "▟", "€", "α", "«"] {
        assert_eq!(face(symbol, "ja-JP", Some("Yu Gothic")), "Corbel", "{symbol}");
    }
    // An empty East Asian slot leaves them in the Latin face.
    assert_eq!(face("°", "ja-JP", None), "Corbel");
}

/// `:112` the symbol predicate behind the slot decision, asserted directly: a
/// resolved East Asian slot claims the measured symbol blocks, `× ÷ ⁇ ⁈` only
/// outside the four complex-script languages, and curly quotes only in
/// `ja`/`ko`/`zh` runs.
#[test]
fn east_asian_slot_symbol_predicate_matches_the_measured_blocks() {
    for cp in [0xa7, 0xa8, 0xb0, 0xb1, 0xb4, 0x2460, 0x25a0, 0x2600, 0x2713, 0xfe30] {
        assert!(is_east_asian_slot_symbol(cp, Some("en-US")), "U+{cp:04X}");
    }
    for lang in ["ar-SA", "he-IL", "hi-IN", "th-TH"] {
        assert!(!is_east_asian_slot_symbol(0xd7, Some(lang)), "{lang}");
        assert!(is_east_asian_slot_symbol(0xa7, Some(lang)), "{lang}");
    }
    assert!(is_east_asian_slot_symbol(0x201c, Some("ja-JP")));
    assert!(!is_east_asian_slot_symbol(0x201c, Some("en-US")));
    assert!(!is_east_asian_slot_symbol(0x201f, Some("ja-JP")));
    for cp in [0xb6, 0x2026, 0x2192, 0x2500, 0x259f, 0x20ac, 0x3b1, 0xab] {
        assert!(!is_east_asian_slot_symbol(cp, Some("ja-JP")), "U+{cp:04X}");
    }
}

/// `:120` resolves an East Asian-slot symbol stack from its own cluster
/// (#1653): `°م` as one run chooses the same faces as `°` and `م` as two runs.
#[test]
fn east_asian_slot_symbol_stack_resolves_per_cluster() {
    let mut joined = run_spec("°م", Some("Corbel"));
    joined.ea = Some("Sakkal Majalla");
    joined.lang = Some("en-US");
    let mut degree = run_spec("°", Some("Corbel"));
    degree.ea = Some("Sakkal Majalla");
    degree.lang = Some("en-US");
    let mut meem = run_spec("م", Some("Corbel"));
    meem.ea = Some("Sakkal Majalla");
    meem.lang = Some("en-US");
    assert_eq!(
        faces(&body(&[joined])),
        faces(&body(&[degree, meem])),
        "one run and two runs must resolve the same faces"
    );
}

// ---------------------------------------------------------------------------
// font-requests.test.ts — authored CS faces reach the display list
// ---------------------------------------------------------------------------

/// ref font-requests.test.ts `retains authored CS faces in shape preload names`
/// (`it.each(['shape', 'table'])`). The reference asserts the preload name set;
/// the port has no preload surface (no `PptxFontPreloadAccumulator`), so the
/// same contract is asserted where the name is actually used: the painted run
/// reports the authored `a:cs` face, and it does so on both element paths.
#[test]
fn authored_cs_face_reaches_the_painted_run() {
    let mut spec = run_spec("שלום", Some("Arial"));
    spec.cs = Some("Calibri");
    spec.lang = Some("he-IL");
    let cell_body = body_value(&[spec]);
    let text_body: pptx_model::TextBody =
        serde_json::from_value(cell_body.clone()).expect("text body fixture");
    let shape_faces = faces(&text_body);
    assert_eq!(
        face_of(&shape_faces, "שלום"),
        "Carlito",
        "the authored `a:cs` face owns the Hebrew text"
    );

    // Table path: the same cell body compiled through `compile_table_element`.
    let value = serde_json::json!({
        "x": 0, "y": 0,
        "width": 200 * 12_700, "height": 40 * 12_700,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "cols": [200 * 12_700],
        "rows": [{
            "height": 40 * 12_700,
            "cells": [{
                "textBody": cell_body,
                "fill": serde_json::Value::Null,
                "horzOverflow": serde_json::Value::Null,
                "borderL": serde_json::Value::Null,
                "borderR": serde_json::Value::Null,
                "borderT": serde_json::Value::Null,
                "borderB": serde_json::Value::Null,
                "gridSpan": 1, "rowSpan": 1,
                "hMerge": false, "vMerge": false,
            }],
        }],
        "rtl": false,
    });
    let table: pptx_model::TableElement = serde_json::from_value(value).expect("table fixture");
    let (tx, _rx) = unbounded();
    let commands = kitty_pptx_core::compiler::tables::compile_table_element(&table, &tx);
    let table_faces: Vec<(String, String)> = runs_of(&commands)
        .into_iter()
        .map(|run| (run.text, run.font_family))
        .collect();
    assert_eq!(
        face_of(&table_faces, "שלום"),
        "Carlito",
        "a table cell resolves the same `a:cs` slot"
    );
}
