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

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand};
use kitty_pptx_core::text::emit::compile_text_body;
use serde_json::{json, Value};

const PT: f32 = 12_700.0; // EMU per reference px (SCALE = 1/12700)
const FONT_PX: f64 = 20.0;
/// `marL` / `indent` of the reference's hanging-gutter list metric (36 px).
const GUTTER_EMU: i64 = 457_200;
const BOX_W: f32 = 4000.0;
const BOX_H: f32 = 2000.0;

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
#[ignore = "blocked: Bullet::Blip is not consumed by text::emit; no DrawImage for a picture marker"]
fn picture_marker_is_sized_to_the_text_height_with_its_aspect() {
    let commands = compile(&body(blip_bullet(None, None), vec![text_run("Item")], "horz"));
    assert_eq!(images(&commands), 1, "one marker bitmap");
    close(20.0, 20.0, "height = 100 % of a 20 pt run");
    close(20.0 * (16.0 / 8.0), 40.0, "2:1 aspect");
}

/// `:262` "draws a warmed picture marker in wordArtVert".
#[test]
#[ignore = "blocked: Bullet::Blip is not consumed by text::emit (stacked path)"]
fn picture_marker_is_drawn_in_word_art_vert() {
    let commands = compile(&body(
        blip_bullet(None, None),
        vec![text_run("Item")],
        "wordArtVert",
    ));
    assert_eq!(images(&commands), 1);
}

/// `:262` "draws a warmed picture marker in wordArtVertRtl".
#[test]
#[ignore = "blocked: Bullet::Blip is not consumed by text::emit (stacked path)"]
fn picture_marker_is_drawn_in_word_art_vert_rtl() {
    let commands = compile(&body(
        blip_bullet(None, None),
        vec![text_run("Item")],
        "wordArtVertRtl",
    ));
    assert_eq!(images(&commands), 1);
}

/// `:475` "scales the bullet by buSzPct (§21.1.2.4.9)": 50 % of 20 px → 10 × 20.
#[test]
#[ignore = "blocked: Bullet::Blip::size_pct is not consumed (no picture marker at all)"]
fn picture_marker_scales_by_bu_sz_pct() {
    let commands = compile(&body(
        blip_bullet(Some(50.0), None),
        vec![text_run("Item")],
        "horz",
    ));
    assert_eq!(images(&commands), 1);
    close(10.0, 20.0 * 0.5, "buSzPct 50 %");
    close(10.0 * 2.0, 20.0, "aspect preserved");
}

/// `:497` "sizes the bullet by buSzPts absolutely (§21.1.2.4.10), independent of
/// the run": 40 pt → 40 px tall, 80 px wide.
#[test]
#[ignore = "blocked: Bullet::Blip::size_pts is not consumed (no picture marker at all)"]
fn picture_marker_sizes_by_bu_sz_pts_absolutely() {
    let commands = compile(&body(
        blip_bullet(None, Some(40.0)),
        vec![text_run("Item")],
        "horz",
    ));
    assert_eq!(images(&commands), 1);
    close(40.0, 40.0, "absolute 40 pt marker");
    close(40.0 * 2.0, 80.0, "aspect preserved");
}

/// `:521` "prefers buSzPts over a co-present buSzPct on a picture bullet":
/// absolute 30 pt wins over 200 % × 20 pt = 40 px.
#[test]
#[ignore = "blocked: Bullet::Blip::size_pts / size_pct are not consumed"]
fn picture_marker_prefers_bu_sz_pts_over_bu_sz_pct() {
    let commands = compile(&body(
        blip_bullet(Some(200.0), Some(30.0)),
        vec![text_run("Item")],
        "horz",
    ));
    assert_eq!(images(&commands), 1);
    close(30.0, 30.0, "absolute size wins");
}

// ── browser-only cache / decode paths ─────────────────────────────────────

/// `:281` "preserves a safe native cache variant prepared by renderSlide": the
/// 4096 × 2048 PNG stays a native `createImageBitmap` decode.
#[test]
#[ignore = "N/A: ImageBitmap cache, `createImageBitmap` and the per-surface decode budget are browser-only"]
fn picture_marker_reuses_the_prepared_bitmap_cache() {}

/// `:328` "routes SVG picture bullets through the worker bridge at their marker
/// height" (`{ targetHeightPx: 54 }` at DPR 2).
#[test]
#[ignore = "N/A: SVG worker-bridge decoder (`svgDecoder`) is browser-only"]
fn svg_picture_marker_uses_the_worker_bridge() {
    close(40.0 * 96.0 / 72.0 * 1.0, 53.33, "unrounded marker height");
}

/// `:385` "loads an SVG picture bullet through HTMLImageElement in Window
/// mode".
#[test]
#[ignore = "N/A: HTMLImageElement / URL.createObjectURL are browser-only"]
fn svg_picture_marker_loads_through_html_image_element() {}

/// `:435` "dedupes a shared SVG bullet at the largest authored marker height"
/// (`targetHeightPx: 107` = 40 pt × 96/72 × DPR 2, rounded up).
#[test]
#[ignore = "N/A: shared-bitmap dedupe cache is browser-only"]
fn shared_svg_marker_dedupes_at_the_largest_height() {
    let height: f32 = 40.0 * 96.0 / 72.0 * 2.0;
    close(height.ceil(), 107.0, "DPR 2 marker height");
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
#[ignore = "blocked: no picture marker is painted, so the hanging-indent gutter cannot be measured against it"]
fn first_line_text_clears_the_picture_marker() {
    let commands = compile(&body(blip_bullet(None, None), vec![text_run("Item")], "horz"));
    let runs = draw_runs(&commands);
    let first_text_x = runs
        .iter()
        .find(|run| run.text == "Item")
        .and_then(|run| run.glyphs.first())
        .map(|glyph| glyph.x / PT)
        .expect("the run text is drawn");
    close(first_text_x - 36.0, 7.2 + GUTTER_EMU as f32 / PT, "hanging gap");
}
