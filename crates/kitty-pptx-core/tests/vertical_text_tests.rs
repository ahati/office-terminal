//! `eaVert` per-glyph orientation (UAX #50) parity.
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/vertical-text.ts`
//! `drawEaVertRunWithCapability` (96-170) and `renderer.ts:5034-5083`, the
//! `eaVert` re-entry with `eaVertUpright = true`; the substitution maps live in
//! `packages/core/src/text/vertical-orientation.ts`.
//!
//! The pure probes pin the geometry and the substitution table exactly (fixed
//! advances, no font needed); the end-to-end probe compiles a real `TextBody`
//! and only asserts what is font independent — one upright cell per East-Asian
//! cluster, the cells marching down the column, and the upright command carrying
//! the element transform rather than the +90° frame.

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::text::vertical::{
    ea_vert_frame, ea_vert_pieces, em_centre_above, layout_frame, orientation, vertical_form,
    EaVertFrame, Orient, VertMode,
};

const POINTS: f32 = 20.0;

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

/// The `eaVert` frame of a 200 x 100 box with an identity element transform.
fn frame() -> EaVertFrame {
    let base = AffineTransform::identity();
    ea_vert_frame(VertMode::of("eaVert"), 200, 100, &base).expect("eaVert has an uprighting frame")
}

/// ref `vertical-text.ts:143-151`: an East-Asian glyph is drawn with the -90°
/// counter-rotation about its cell centre, so in the slide frame its baseline
/// stays horizontal and the cells march DOWN the column on one axis.
#[test]
fn han_column_stands_upright_down_the_column() {
    let pieces = ea_vert_pieces("日本", |_| false);
    assert_eq!(pieces.len(), 2, "one upright cell per ideograph");
    assert!(pieces.iter().all(|piece| piece.upright));

    // Constant cell advance keeps the column axis fixed; the reference's
    // `x + ax + adv/2` cell centre is what `upright_origin` takes.
    let advance = 10.0f32;
    let baseline = 20.0f32;
    let centre_above = 3.0f32;
    let origins: Vec<(f32, f32)> = (0..pieces.len())
        .map(|index| {
            let pen = index as f32 * advance;
            frame().upright_origin(pen + advance * 0.5, baseline, advance, centre_above)
        })
        .collect();

    // The +90° frame sends layout +x (the column) to slide +y, so the glyph
    // origins only move down; the column axis (layout y) is constant.
    close(origins[0].0, origins[1].0, "column axis x");
    assert!(
        origins[1].1 > origins[0].1,
        "second cell below the first: {origins:?}"
    );
    // Baseline origin = cell centre (mapped through the frame) + the em-box
    // centre offset that `textBaseline='middle'` would have applied.
    close(origins[0].1, advance * 0.5 + centre_above, "first cell y");
    close(origins[1].1, advance * 1.5 + centre_above, "second cell y");
    // A 200 x 100 box mirrors layout y into slide x (`200 - v`).
    close(
        origins[0].0,
        200.0 - (baseline - centre_above) - advance * 0.5,
        "x",
    );
    // The em-box centre really lands on the frame-mapped cell centre.
    close(
        origins[0].0 + advance * 0.5,
        200.0 - (baseline - centre_above),
        "em centre x",
    );
    close(origins[0].1 - centre_above, advance * 0.5, "em centre y");
}

/// ref `vertical-text.ts:160-169`: `vo=R` glyphs stay sideways with the frame,
/// so a Latin word is one rotated group (no per-glyph uprighting).
#[test]
fn latin_word_stays_one_rotated_piece() {
    let pieces = ea_vert_pieces("Hello", |_| false);
    assert_eq!(pieces.len(), 1);
    assert!(!pieces[0].upright);
    assert_eq!(pieces[0].painted, "Hello");

    for ch in ['H', 'e', '1', '.', '(', '→'] {
        assert_eq!(orientation(ch), Orient::Rotated, "{ch} rotates");
    }
    for ch in ['日', 'あ', 'ア', '한', '（', '。'] {
        assert_eq!(orientation(ch), Orient::Upright, "{ch} stands upright");
    }
    // ー / 〜 / ～ are the Tr long marks: no vertical form, rotate fallback.
    for ch in ['ー', '〜', '～'] {
        assert_eq!(orientation(ch), Orient::Rotated, "{ch} rotates");
    }
}

/// ref `vertical-orientation.ts`: `vo=Tu` corner-hanging punctuation and `vo=Tr`
/// brackets swap to their U+FE1x / U+FE3x vertical presentation form when the
/// face has it, and keep the authored glyph when it does not.
#[test]
fn punctuation_substitutes_vertical_forms_when_present() {
    let pairs = [
        ('（', '\u{fe35}'),
        ('）', '\u{fe36}'),
        ('「', '\u{fe41}'),
        ('」', '\u{fe42}'),
        ('、', '\u{fe11}'),
        ('。', '\u{fe12}'),
        ('ー', '\u{fe31}'),
        ('〜', '\u{fe32}'),
    ];
    for (source, form) in pairs {
        assert_eq!(vertical_form(source), Some(form), "{source}");

        let present = ea_vert_pieces(&source.to_string(), |_| true);
        assert_eq!(present.len(), 1, "{source}");
        assert_eq!(present[0].painted, form.to_string(), "{source} in face");
        // Substitution is glyph-only: the cell keeps the authored advance.
        assert_eq!(present[0].measured, source.to_string());

        let absent = ea_vert_pieces(&source.to_string(), |_| false);
        assert_eq!(absent.len(), 1, "{source}");
        assert_eq!(absent[0].painted, source.to_string(), "{source} fallback");
    }
    // A substituted vertical form is already vertical, so it stands up; with no
    // form in the face the vo=Tr long marks keep their rotate fallback.
    for source in ["（", "、", "ー"] {
        assert!(ea_vert_pieces(source, |_| true)[0].upright, "{source}");
    }
    assert!(!ea_vert_pieces("ー", |_| false)[0].upright);
}

/// Mixed text splits into maximal same-orientation segments: upright cells on
/// their own, the sideways clusters grouped.
#[test]
fn mixed_han_and_latin_splits_into_upright_and_rotated_segments() {
    let pieces = ea_vert_pieces("日A本bc、", |_| false);
    let shape: Vec<(bool, &str)> = pieces
        .iter()
        .map(|piece| (piece.upright, piece.painted.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (true, "日"),
            (false, "A"),
            (true, "本"),
            (false, "bc"),
            (true, "、"),
        ]
    );
}

/// `eaVert` keeps `vert`'s swapped layout box: the column runs along the box
/// height, which is what the line box arithmetic measures against today.
#[test]
fn line_box_uses_the_swapped_column() {
    let (layout_w, layout_h, frame) = layout_frame(VertMode::of("eaVert"), 200, 100);
    let reference = layout_frame(VertMode::of("vert"), 200, 100);
    close(layout_w, 100.0, "layout width = box height");
    close(layout_h, 200.0, "layout height = box width");
    assert_eq!(frame, reference.2, "same +90° frame as `vert`");

    // Only `eaVert` uprights glyphs; `vert`/`vert270`/stacked stay unchanged.
    let base = AffineTransform::identity();
    assert!(ea_vert_frame(VertMode::of("eaVert"), 200, 100, &base).is_some());
    for vert in ["horz", "vert", "vert270", "wordArtVert"] {
        assert!(
            ea_vert_frame(VertMode::of(vert), 200, 100, &base).is_none(),
            "{vert} has no per-glyph uprighting"
        );
    }

    // The em-box centre above the baseline is `ascent - box/2` of the 1.2-em
    // line box (`natural_line_box`).
    close(em_centre_above(1000.0, 0.8), 360.0, "em centre");
    close(em_centre_above(1000.0, 0.5), 0.0, "symmetric face");
}

fn body(vert: &str, text: &str) -> pptx_model::TextBody {
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {POINTS},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "none",
            "vert": "{vert}",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{{
                "alignment": "l",
                "marL": 0, "marR": 0, "indent": 0,
                "spaceBefore": null,
                "lvl": 0,
                "bullet": {{ "type": "none" }},
                "defFontSize": {POINTS},
                "tabStops": [],
                "defTabSz": null,
                "rtl": false,
                "eaLnBrk": true,
                "runs": [{{
                    "type": "text",
                    "text": "{text}",
                    "bold": false, "italic": false,
                    "underline": false, "strikethrough": false,
                    "strikeDouble": false,
                    "hyperlinkUsesTextFill": false,
                    "fieldType": null,
                    "fontSize": {POINTS},
                    "color": "000000",
                    "fontFamily": "Calibri"
                }}]
            }}]
        }}"#
    );
    serde_json::from_str(&json).expect("text body fixture")
}

fn compile(vert: &str, text: &str) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        &body(vert, text),
        2_000_000,
        1_000_000,
        &AffineTransform::identity(),
        None,
        &tx,
    )
}

fn draw_texts(commands: &[VectorCommand]) -> Vec<(&[PositionedGlyphRun], AffineTransform)> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, transform } => Some((runs.as_slice(), *transform)),
            _ => None,
        })
        .collect()
}

/// End to end: `eaVert` upright cells leave the frame, the Latin group keeps it.
#[test]
fn ea_vert_command_splits_upright_and_rotated_text() {
    let identity = AffineTransform::identity();
    let (_, _, frame) = layout_frame(VertMode::of("eaVert"), 2_000_000, 1_000_000);
    let frame = frame.expect("eaVert frame");

    // A Han column is one upright DrawText carrying one run per cell
    // (`vertical-text.ts:143` fills one code point at a time).
    let han = compile("eaVert", "日日");
    let texts = draw_texts(&han);
    assert_eq!(texts.len(), 1, "upright cells only: {texts:?}");
    assert_eq!(texts[0].1, identity, "upright cells skip the +90° frame");
    let cells = texts[0].0;
    assert_eq!(cells.len(), 2);
    assert!(cells.iter().all(|run| run.text == "日"));
    let (first, second) = (cells[0].glyphs[0], cells[1].glyphs[0]);
    assert!(
        second.y > first.y,
        "cells advance down the column: {first:?} {second:?}"
    );
    // The column axis is fixed across the swapped box (no per-cell drift).
    assert!(
        (second.x - first.x).abs() <= (second.advance - first.advance).abs() * 0.5 + 0.01,
        "cells stay on the column axis: {first:?} {second:?}"
    );

    // A Latin word keeps the rotated frame, so mixed text emits both commands.
    let mixed = compile("eaVert", "日A");
    let texts = draw_texts(&mixed);
    assert_eq!(texts.len(), 2, "upright + rotated: {texts:?}");
    assert!(
        texts
            .iter()
            .any(|(runs, transform)| *transform == identity && runs[0].text == "日"),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|(runs, transform)| {
            *transform == frame && runs.iter().any(|run| run.text.contains('A'))
        }),
        "{texts:?}"
    );
}

/// `vert` still rotates every glyph: no upright command is emitted.
#[test]
fn vert_mode_keeps_one_rotated_command() {
    let commands = compile("vert", "日日");
    let texts = draw_texts(&commands);
    assert_eq!(texts.len(), 1, "one frame run: {texts:?}");
    let (_, _, rotated) = layout_frame(VertMode::of("vert"), 2_000_000, 1_000_000);
    assert_eq!(texts[0].1, rotated.expect("vert frame"));
}
