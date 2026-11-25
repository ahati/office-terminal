//! Text-box parity (`text-box-b`): picture bullets (`<a:buBlip>`,
//! ECMA-376 §21.1.2.4.2).
//!
//! Ports, with the reference's own values, from `/tmp/oooxml` @ `1f5c6e1d`:
//! `packages/pptx/src/bullet-image.test.ts`.
//!
//! The reference drives `renderTextBody` against a mock context that records
//! `drawImage` and `fillText`, with the bitmap cache warmed by `renderSlide`'s
//! prefetch pass. This port compiles the same `TextBody` and inspects the vector
//! IR: a picture marker would be a `VectorCommand::DrawImage` in the marker's
//! gutter. `Bullet::Blip` is not consumed by `text::emit`, so the drawing cases
//! are blocked and only the "draws nothing" guards are observable.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_box_b_bullet_image_tests.rs.txt`; the
//! ledger keeps their rows.

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{AffineTransform, PositionedGlyphRun, RectF32, VectorCommand};
use office_terminal_core::text::bullets::MarkerBitmap;
use office_terminal_core::text::emit::{compile_text_body, compile_text_body_with_markers};
use serde_json::{json, Value};

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;
/// `marL` / `indent` of the reference's hanging-gutter list metric (36 px).
const GUTTER_EMU: i64 = 457_200;
const BOX_W: f32 = 4000.0;
const BOX_H: f32 = 2000.0;
/// The specs' sentinel `ImageBitmap`: intentionally NON-square (16 × 8, ratio
/// 2:1) so a forced-square draw fails (`bullet-image.test.ts:32-38`).
const SENTINEL_W: f32 = 16.0;
const SENTINEL_H: f32 = 8.0;
const SENTINEL_RATIO: f32 = SENTINEL_W / SENTINEL_H;

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

/// The specs' `blipBullet(over)` helper.
fn blip_bullet(size_pct: Option<f64>, size_pts: Option<f64>) -> Value {
    json!({
        "type": "blip",
        "imagePath": "ppt/media/bullet-img.png",
        "mimeType": "image/png",
        "sizePct": size_pct,
        "sizePts": size_pts,
    })
}

fn char_bullet() -> Value {
    json!({
        "type": "char",
        "char": "•",
        "color": null,
        "sizePct": null,
        "fontFamily": "Arial",
    })
}

/// `bodyWithBullet(bullet, runs)`: 91440 / 45720 EMU insets, `wrap: square`.
fn body(bullet: Value, runs: Vec<Value>, vert: &str) -> pptx_model::TextBody {
    let value = json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l",
            "marL": GUTTER_EMU, "marR": 0, "indent": -GUTTER_EMU,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0,
            "bullet": bullet,
            "defFontSize": null, "defColor": null, "defBold": null, "defItalic": null,
            "defFontFamily": null,
            "tabStops": [], "defTabSz": null, "rtl": false, "eaLnBrk": true,
            "runs": runs,
        }],
        "defaultFontSize": FONT_PX,
        "defaultBold": null, "defaultItalic": null,
        "lIns": 91_440, "rIns": 91_440, "tIns": 45_720, "bIns": 45_720,
        "wrap": "square",
        "vert": vert,
        "autoFit": "none",
        "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
    });
    serde_json::from_value(value).expect("picture-bullet body fixture")
}

fn compile(text_body: &pptx_model::TextBody) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body(
        text_body,
        (BOX_W * PT) as i64,
        (BOX_H * PT) as i64,
        &AffineTransform::identity(),
        None,
        &tx,
    )
}

/// The decoded-bitmap seam (`peekCachedBitmap`, `renderer.ts:5812`): the
/// reference warms the cache before the synchronous draw, so every test that
/// expects a marker paints through this probe.
struct Sentinel;

impl MarkerBitmap for Sentinel {
    fn source_size(&self, _rel_id: &str) -> Option<(u32, u32)> {
        Some((SENTINEL_W as u32, SENTINEL_H as u32))
    }
}

/// Compile with the warmed sentinel bitmap available.
fn compile_warmed(text_body: &pptx_model::TextBody) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    compile_text_body_with_markers(
        text_body,
        (BOX_W * PT) as i64,
        (BOX_H * PT) as i64,
        &AffineTransform::identity(),
        None,
        Some(&Sentinel),
        &tx,
    )
}

/// The placed marker bitmaps' destination boxes, in reference px.
fn draws(commands: &[VectorCommand]) -> Vec<RectF32> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawImage { dest_rect, .. } => Some(RectF32::new(
                dest_rect.x / PT,
                dest_rect.y / PT,
                dest_rect.w / PT,
                dest_rect.h / PT,
            )),
            _ => None,
        })
        .collect()
}

fn images(commands: &[VectorCommand]) -> usize {
    commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawImage { .. }))
        .count()
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

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected}, got {actual}"
    );
}

// ── the marker bitmap ──────────────────────────────────────────────────────

/// `:226` "draws the warmed bullet bitmap sized to the text height with the
/// bitmap aspect ratio, at the bullet gutter": the 16 × 8 sentinel paints at
/// `h = 20` (100 % of the 20 pt run) and `w = 40` (2:1, not forced square).
#[test]
fn picture_marker_is_sized_to_the_text_height_with_its_aspect() {
    let commands = compile_warmed(&body(blip_bullet(None, None), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 1, "one marker bitmap");
    let dest = draws(&commands)[0];
    // Height = 20 pt run × scale(1/12700) × 12700 = 20 px (buSzPct default 100 %).
    close(dest.h, 20.0, "height = 100 % of a 20 pt run");
    // Width preserves the bitmap aspect ratio (16:8 = 2:1), not forced square.
    close(dest.w, 20.0 * SENTINEL_RATIO, "2:1 aspect");
    close(dest.w, dest.h * SENTINEL_RATIO, "aspect preserved");
}

/// `:262` "draws a warmed picture marker in wordArtVert".
#[test]
fn picture_marker_is_drawn_in_word_art_vert() {
    let commands = compile_warmed(&body(blip_bullet(None, None), vec![text_run("Item")], "wordArtVert"));
    assert_eq!(images(&commands), 1);
    let dest = draws(&commands)[0];
    close(dest.h, 20.0, "stacked marker height");
    close(dest.w, 20.0 * SENTINEL_RATIO, "stacked marker width");
}

/// `:262` "draws a warmed picture marker in wordArtVertRtl".
#[test]
fn picture_marker_is_drawn_in_word_art_vert_rtl() {
    let commands = compile_warmed(&body(blip_bullet(None, None), vec![text_run("Item")], "wordArtVertRtl"));
    assert_eq!(images(&commands), 1);
    let dest = draws(&commands)[0];
    close(dest.h, 20.0, "stacked marker height");
    close(dest.w, 20.0 * SENTINEL_RATIO, "stacked marker width");
}

/// `:475` "scales the bullet by buSzPct (§21.1.2.4.9)": 50 % of 20 px → 10 × 20.
#[test]
fn picture_marker_scales_by_bu_sz_pct() {
    let commands = compile_warmed(&body(blip_bullet(Some(50.0), None), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 1);
    let dest = draws(&commands)[0];
    // Height = 20 px text × 50 % = 10 px; width keeps the 2:1 aspect.
    close(dest.h, 20.0 * 0.5, "buSzPct 50 %");
    close(dest.w, 10.0 * SENTINEL_RATIO, "aspect preserved");
}

/// `:497` "sizes the bullet by buSzPts absolutely (§21.1.2.4.10), independent of
/// the run": 40 pt → 40 px tall, 80 px wide.
#[test]
fn picture_marker_sizes_by_bu_sz_pts_absolutely() {
    let commands = compile_warmed(&body(blip_bullet(None, Some(40.0)), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 1);
    let dest = draws(&commands)[0];
    close(dest.h, 40.0, "absolute 40 pt marker");
    close(dest.w, 40.0 * SENTINEL_RATIO, "aspect preserved");
}

/// `:521` "prefers buSzPts over a co-present buSzPct on a picture bullet":
/// absolute 30 pt wins over 200 % × 20 pt = 40 px.
#[test]
fn picture_marker_prefers_bu_sz_pts_over_bu_sz_pct() {
    let commands = compile_warmed(&body(
        blip_bullet(Some(200.0), Some(30.0)),
        vec![text_run("Item")],
        "horz",
    ));
    assert_eq!(images(&commands), 1);
    let dest = draws(&commands)[0];
    // Absolute 30 pt → 30 px wins over 200 % × 20 pt (= 40 px).
    close(dest.h, 30.0, "absolute size wins");
    close(dest.w, 30.0 * SENTINEL_RATIO, "aspect preserved");
}

// ── guards that are observable without the paint path ─────────────────────

/// `:545` "draws nothing (no throw) when the bullet image is not yet decoded":
/// an unwarmed bitmap leaves no image command and does not panic.
#[test]
fn undecoded_picture_marker_draws_nothing() {
    let commands = compile(&body(blip_bullet(None, None), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 0, "no bitmap is placed");
}

/// `:566` "does not draw a picture bullet on an empty paragraph".
#[test]
fn empty_paragraph_draws_no_picture_marker() {
    let commands = compile(&body(blip_bullet(None, None), Vec::new(), "horz"));
    assert_eq!(images(&commands), 0, "no bitmap is placed");
    assert!(
        draw_runs(&commands).is_empty(),
        "no text either: {:?}",
        draw_runs(&commands)
    );
}

/// `:584` "does not call drawImage for a char bullet (regression guard)": the
/// marker stays on the `fillText` path.
#[test]
fn char_marker_never_becomes_an_image() {
    let commands = compile(&body(char_bullet(), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 0, "no image for a char marker");
    let runs = draw_runs(&commands);
    assert_eq!(runs[0].text.trim_end(), "•", "marker run: {runs:?}");
}

/// `:602` "first-line text does not overlap the picture bullet (hanging
/// indent)": `firstTextX − bulletX ≈ 457200 · SCALE = 36`.
#[test]
fn first_line_text_clears_the_picture_marker() {
    let commands = compile_warmed(&body(blip_bullet(None, None), vec![text_run("Item")], "horz"));
    let dest = draws(&commands)[0];
    let runs = draw_runs(&commands);
    // A picture bullet occupies the gutter exactly like a char marker, so the
    // first line's hanging indent is suppressed: the marker seats at
    // `textX + indent` = lIns (7.2 px) and the text starts at `textX` = lIns +
    // marL (43.2 px) — `firstTextX - bulletX ≈ |indent| = 36 px`.
    close(dest.x, 7.2, "marker x = lIns + marL + indent");
    let first_text_x = runs
        .iter()
        .find(|run| run.text == "Item")
        .and_then(|run| run.glyphs.first())
        .map(|glyph| glyph.x / PT)
        .expect("the run text is drawn");
    close(first_text_x, 7.2 + GUTTER_EMU as f32 / PT, "text x = lIns + marL");
    close(first_text_x - dest.x, GUTTER_EMU as f32 / PT, "hanging gap");
}
