//! Misc parity batch — ports of the reference PPTX specs that are not tied to
//! one rendering sub-system (`dims`, `parse-error placeholders`, `scene3d`
//! transform order, `WordArt` resource bounds, SmartArt fallback contrast,
//! parsed-text-colour provenance).
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/*.test.ts`.
//! Every case below carries the reference's own inputs and expected numbers.
//!
//! BLOCKED cases (no production API yet — see
//! `tests/parity/coverage/misc.md` for the per-file tally and the missing
//! symbols):
//!   * `backgroundLuminance`        → no `background_luminance(&Fill) -> Option<f32>`
//!   * `smartArtFallbackTextColor`  → no contrast-aware default in `text::emit`
//!   * `isSmartArtFallbackShape`    → no fallback-shape predicate in `compiler`
//!   * `PptxTextRunInfo` identity   → `PositionedGlyphRun` carries no
//!     `shape_id` / `element_index` / `origin`
//!   * `hlinkClr="tx"`              → text emit never colours hyperlink runs
//!   * slide-theme / caller `defaultTextColor` fallback → `compile_text_body`
//!     only takes `default_shape_text_color`
//!
//! The parse-error placeholder cases moved to `parity_parse_error_tests` when
//! `Slide::parse_error` emission landed (ref renderer.ts:8188-8274).

use std::collections::BTreeMap;
use std::sync::Arc;

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::dim::{apply_dim_overlay, DimOptions};
use kitty_pptx_core::compiler::style::parse_hex_color;
use kitty_pptx_core::compiler::text::compile_text_body;
use kitty_pptx_core::compiler::transform::compute_shape_transform;
use kitty_pptx_core::compiler::text_warp::build_warp_envelope;
use kitty_pptx_core::compiler::{DefaultSlideCompiler, SlideCompiler};
use kitty_pptx_core::contracts::{
    AffineTransform, FillRule, PathSegment, PositionedGlyphRun, VectorBackground, VectorCommand,
    VectorPath, VectorPaint, VectorSlide,
};
use pptx_model::{
    Bullet, Fill, Paragraph, ShapeElement, Slide, SlideElement, SlideElementOrigin,
    SlideElementSource, SlideTheme, TextBody, TextRun, TextRunData, TextWarp,
};

const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 6_858_000;

// ── model builders (mirror the reference test literals) ────────────────────

fn text_run(text: &str, color: Option<&str>) -> TextRunData {
    TextRunData {
        text: text.to_string(),
        bold: None,
        italic: None,
        underline: false,
        underline_style: None,
        underline_color: None,
        underline_fill: None,
        underline_line: None,
        underline_line_no_fill: false,
        strikethrough: false,
        strike_double: false,
        font_size: Some(18.0),
        color: color.map(str::to_string),
        pattern_fill: None,
        glyph_fill: None,
        no_fill: false,
        font_family: None,
        font_family_ea: None,
        font_family_cs: None,
        font_family_sym: None,
        lang: None,
        alt_lang: None,
        baseline: None,
        caps: None,
        letter_spacing: None,
        field_type: None,
        hyperlink: None,
        hyperlink_uses_text_fill: false,
        hyperlink_action: None,
        hyperlink_mouse_over: None,
        hyperlink_mouse_over_action: None,
        shadow: None,
        reflection: None,
        outline: None,
        highlight: None,
        character_attributes: BTreeMap::new(),
        character_child_attributes: BTreeMap::new(),
    }
}

/// ref smartart-fallback-contrast.test.ts `paragraph(text)`.
fn paragraph(text: &str) -> Paragraph {
    Paragraph {
        alignment: "l".to_string(),
        mar_l: 0,
        mar_r: 0,
        indent: 0,
        space_before: None,
        space_after: None,
        space_before_pct: None,
        space_after_pct: None,
        space_line: None,
        lvl: 0,
        bullet: Bullet::None,
        def_font_size: None,
        def_color: None,
        def_bold: None,
        def_italic: None,
        def_font_family: None,
        tab_stops: Vec::new(),
        def_tab_sz: None,
        rtl: false,
        ea_ln_brk: true,
        font_algn: None,
        runs: vec![TextRun::Text(text_run(text, None))],
        math_fallbacks: Vec::new(),
        end_run_properties: None,
    }
}

/// ref smartart-fallback-contrast.test.ts `textBody(text)`.
fn text_body(text: &str) -> TextBody {
    TextBody {
        vertical_anchor: "t".to_string(),
        paragraphs: vec![paragraph(text)],
        default_font_size: Some(18.0),
        default_bold: None,
        default_italic: None,
        l_ins: 91440,
        r_ins: 91440,
        t_ins: 45720,
        b_ins: 45720,
        wrap: "square".to_string(),
        vert: "horz".to_string(),
        auto_fit: "none".to_string(),
        font_scale: None,
        ln_spc_reduction: None,
        num_col: 1,
        spc_col: 0,
        rtl_col: false,
        spc_first_last_para: false,
        anchor_ctr: false,
        compat_ln_spc: None,
        text_warp: None,
    }
}

/// ref smartart-fallback-contrast.test.ts `fallbackShape(text)` — the shape the
/// Rust SmartArt fallback synthesizes (`ppt/slides` "SmartArt", no cNvPr id)
/// unless the caller overrides `name` / `id`.
fn fallback_shape(text: &str) -> ShapeElement {
    ShapeElement {
        math_fallback: None,
        x: 914_400,
        y: 914_400,
        width: 4_572_000,
        height: 2_286_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        geometry: "rect".to_string(),
        fill: None,
        stroke: None,
        text_body: Some(text_body(text)),
        default_text_color: None,
        cust_geom: None,
        cust_geom_paint: None,
        adj: None,
        adj2: None,
        adj3: None,
        adj4: None,
        adj5: None,
        adj6: None,
        adj7: None,
        adj8: None,
        shadow: None,
        inner_shadow: None,
        glow: None,
        soft_edge: None,
        reflection: None,
        id: None,
        name: Some("SmartArt".to_string()),
        hyperlink: None,
        hyperlink_action: None,
        placeholder_type: None,
        placeholder_idx: None,
        text_rect: None,
        scene3d: None,
        sp3d: None,
    }
}

fn solid(hex: &str) -> Fill {
    Fill::Solid {
        color: hex.to_string(),
    }
}

fn slide_with(background: Option<Fill>, elements: Vec<ShapeElement>) -> Slide {
    let sources = elements
        .iter()
        .map(|_| SlideElementSource {
            origin: SlideElementOrigin::Slide,
        })
        .collect();
    Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: Some("ppt/slides/slide1.xml".to_string()),
        background,
        elements: elements.into_iter().map(SlideElement::Shape).collect(),
        element_sources: sources,
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

fn compile(slide: &Slide) -> VectorSlide {
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile(slide, SLIDE_W, SLIDE_H, &tx)
        .expect("slide compiles")
}

fn runs(slide: &VectorSlide) -> Vec<PositionedGlyphRun> {
    slide
        .commands
        .iter()
        .flat_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => runs.clone(),
            _ => Vec::new(),
        })
        .collect()
}

fn colour_of(slide: &VectorSlide, needle: &str) -> [u8; 4] {
    runs(slide)
        .into_iter()
        .find(|run| run.text.contains(needle))
        .unwrap_or_else(|| panic!("no run containing {needle:?}"))
        .color
}

fn background(slide: &VectorSlide) -> Option<VectorBackground> {
    slide.background.clone()
}

// ── scene3d transform order ────────────────────────────────────────────────
// ref scene3d-transform-order.test.ts: `splitScene3dShapeTransform(25, true,
// false)` → `{outerRotation: 25, localFlipH: true, localFlipV: false}`, i.e.
// the authored flip belongs to the local face and the 2-D rotation stays
// outside it. `compute_shape_transform` composes exactly that:
// M = R(25°) · diag(−1, 1) (ref renderer.ts:3576-3586, 3969-3986).

#[test]
fn scene3d_keeps_rotation_outside_the_local_flip() {
    let t = compute_shape_transform(0, 0, 1_000, 1_000, 25.0, true, false);
    let rad = 25f32.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());

    // Outer rotation applied to a horizontally mirrored local face.
    assert!((t.sx - (-cos)).abs() < 1e-5, "sx = {}", t.sx);
    assert!((t.sy - cos).abs() < 1e-5, "sy = {}", t.sy);
    assert!((t.kx - (-sin)).abs() < 1e-5, "kx = {}", t.kx);
    assert!((t.ky - (-sin)).abs() < 1e-5, "ky = {}", t.ky);

    // `kx = −sin` is what distinguishes R·F from F·R (flip outside would give
    // `kx = +sin`), so the reference's ordering is pinned by this number.
    assert!((t.kx - sin).abs() > 1e-3);

    // One mirror only (flipV is false): det = sx·sy − kx·ky = −1.
    let det = t.sx * t.sy - t.kx * t.ky;
    assert!((det + 1.0).abs() < 1e-5, "det = {det}");

    // Center of the 1000×1000 box is the fixed point (rotation about centre).
    let cx = t.sx * 500.0 + t.kx * 500.0 + t.tx;
    let cy = t.ky * 500.0 + t.sy * 500.0 + t.ty;
    assert!((cx - 500.0).abs() < 1e-2, "cx = {cx}");
    assert!((cy - 500.0).abs() < 1e-2, "cy = {cy}");
}

// ── WordArt resource bounds ────────────────────────────────────────────────
// ref wordart-resource-bounds.test.ts: 100 empty manual lines must not build a
// per-line warp envelope — "one base envelope and, at most, one expanded
// envelope for the painted line" (≤ 2 `buildWarpEnvelope` calls). The port
// compiles the same body (620×150 px = 7 874 000 × 1 905 000 EMU) and counts
// *distinct* envelope allocations reachable from the emitted runs.

fn warped_body(empty_lines: usize, painted: &[&str]) -> TextBody {
    let mut runs = Vec::new();
    for (i, text) in painted.iter().enumerate() {
        runs.push(TextRun::Text(text_run(text, None)));
        if i + 1 < painted.len() {
            runs.push(TextRun::Break {
                font_size: None,
                font_family: None,
                bold: None,
                italic: None,
                character_attributes: BTreeMap::new(),
                character_child_attributes: BTreeMap::new(),
            });
        }
    }
    for _ in 0..empty_lines {
        runs.push(TextRun::Break {
            font_size: None,
            font_family: None,
            bold: None,
            italic: None,
            character_attributes: BTreeMap::new(),
            character_child_attributes: BTreeMap::new(),
        });
    }
    let mut para = paragraph("A");
    para.alignment = "ctr".to_string();
    para.runs = runs;

    let mut body = text_body("A");
    body.vertical_anchor = "ctr".to_string();
    body.paragraphs = vec![para];
    body.text_warp = Some(TextWarp {
        preset: "textArchUp".to_string(),
        adj: Vec::new(),
    });
    body
}

fn envelope_allocations(commands: &[VectorCommand]) -> (usize, usize) {
    let mut ptrs: Vec<usize> = Vec::new();
    let mut warp_ptrs: Vec<usize> = Vec::new();
    for cmd in commands {
        if let VectorCommand::DrawText { runs, .. } = cmd {
            for run in runs {
                if let Some(warp) = &run.warp {
                    ptrs.push(Arc::as_ptr(&warp.envelope) as usize);
                    warp_ptrs.push(Arc::as_ptr(warp) as usize);
                }
            }
        }
    }
    ptrs.sort_unstable();
    ptrs.dedup();
    warp_ptrs.sort_unstable();
    warp_ptrs.dedup();
    (ptrs.len(), warp_ptrs.len())
}

#[test]
fn wordart_empty_manual_lines_do_not_build_per_line_envelopes() {
    let (tx, _rx) = unbounded();
    let transform = AffineTransform {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
    // Body exactly as the reference builds it: `run('A')` + 100 breaks.
    let body = warped_body(100, &["A"]);
    let commands = compile_text_body(
        &body,
        620 * 12_700,
        150 * 12_700,
        &transform,
        None,
        &tx,
    );

    let (envelopes, when_needed) = envelope_allocations(&commands);
    // At most the reference's two builds (base + expanded); here: exactly one.
    assert!(envelopes <= 2, "envelope allocations = {envelopes}");
    assert_eq!(envelopes, 1, "one envelope shared by every painted line");
    assert!(
        when_needed <= 2,
        "per-line envelope wrappers = {when_needed}"
    );

    // The 100 empty manual lines paint nothing: one glyph run in total.
    let texts: Vec<String> = commands
        .iter()
        .flat_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => runs.iter().map(|r| r.text.clone()).collect(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(texts, vec!["A".to_string()]);
    assert_eq!(commands.len(), 1);

    // Sanity: the preset really produces a single-edge envelope (620 px wide).
    let envelope = build_warp_envelope("textArchUp", &[], 620.0 * 12_700.0, 150.0 * 12_700.0)
        .expect("textArchUp envelope");
    assert!(envelope.single_edge);
}

#[test]
fn wordart_painted_lines_share_the_same_envelope() {
    let (tx, _rx) = unbounded();
    let transform = AffineTransform {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
    let body = warped_body(97, &["Line1", "Line2", "Line3"]);
    let commands = compile_text_body(&body, 620 * 12_700, 150 * 12_700, &transform, None, &tx);

    let (envelopes, _) = envelope_allocations(&commands);
    assert_eq!(envelopes, 1, "three painted lines reuse one envelope");

    let texts: Vec<String> = commands
        .iter()
        .flat_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => runs.iter().map(|r| r.text.clone()).collect(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(texts, vec!["Line1", "Line2", "Line3"]);
}

// ── dim overlay (ref dim-overlay.test.ts:1-40) ────────────────────────────
// The reference saves, sets `globalAlpha = opacity` and `fillStyle = color`,
// fills the whole slide and restores (ref renderer.ts:8110-8122), after the
// complete slide paint (ref renderer.ts:9091). The display-list port folds the
// alpha product into one trailing full-slide `DrawPath`.

#[test]
fn dim_overlay_paints_one_full_slide_scrim_last() {
    let mut slide = compile(&slide_with(None, vec![fallback_shape("Plain box")]));
    let before = slide.commands.len();

    apply_dim_overlay(
        &mut slide,
        &DimOptions {
            color: parse_hex_color("#ffffff"),
            opacity: 0.6,
        },
    );

    assert_eq!(slide.commands.len(), before + 1, "exactly one command appended");
    match slide.commands.last().expect("scrim command") {
        VectorCommand::DrawPath { path, fill, .. } => {
            let (w, h) = (SLIDE_W as f32, SLIDE_H as f32);
            let full_slide = VectorPath::new(
                vec![
                    PathSegment::MoveTo { x: 0.0, y: 0.0 },
                    PathSegment::LineTo { x: w, y: 0.0 },
                    PathSegment::LineTo { x: w, y: h },
                    PathSegment::LineTo { x: 0.0, y: h },
                    PathSegment::Close,
                ],
                FillRule::NonZero,
            );
            assert_eq!(path, &full_slide, "scrim covers the whole slide box");
            // 0.6 × 255 = 153, alpha-composed with the opaque fill.
            assert_eq!(fill, &Some(VectorPaint::Solid([255, 255, 255, 153])));
        }
        other => panic!("last command must be the scrim, got {other:?}"),
    }
}

// ── SmartArt fallback contrast (#805) / parsed text colour (#1989) ─────────
// ref smartart-fallback-contrast.test.ts `renderAndCollect`: `defaultTextColor:
// '383838'`. The reference option maps onto the model's inherited default
// (`ShapeElement::default_text_color`) here, because `compile_text_body` takes
// that single default. Everything the reference asserts about *unchanged*
// behaviour ports directly; the contrast switch itself is BLOCKED.

const DARK_BG: &str = "1F3864";
const LIGHT_BG: &str = "FFFFFF";
const THEME_DEFAULT: &str = "383838";

#[test]
fn smartart_fallback_keeps_the_theme_default_on_a_light_background() {
    let mut shape = fallback_shape("Task one");
    shape.default_text_color = Some(THEME_DEFAULT.to_string());
    let slide = compile(&slide_with(Some(solid(LIGHT_BG)), vec![shape]));

    assert_eq!(colour_of(&slide, "Task one"), [0x38, 0x38, 0x38, 255]);
    assert_eq!(
        background(&slide),
        Some(VectorBackground::Solid([255, 255, 255, 255]))
    );
}

#[test]
fn dark_background_does_not_touch_ordinary_shapes() {
    let mut shape = fallback_shape("Plain box");
    shape.name = Some("Rectangle 5".to_string());
    shape.id = Some("4".to_string());
    shape.default_text_color = Some(THEME_DEFAULT.to_string());
    let slide = compile(&slide_with(Some(solid(DARK_BG)), vec![shape]));

    // No contrast switch: the inherited default survives on a dark slide.
    assert_eq!(colour_of(&slide, "Plain box"), [0x38, 0x38, 0x38, 255]);
    assert_eq!(
        background(&slide),
        Some(VectorBackground::Solid([0x1F, 0x38, 0x64, 255]))
    );
}

#[test]
fn parsed_slide_without_an_authored_colour_paints_hard_black() {
    // Issue #1989: the parser resolved every inherited colour, so a run still
    // without one paints 000000 — neither the caller default nor the slide
    // theme (`parsedTextChain` is unconditional here: the crate only renders
    // parser-produced models).
    let mut plain = fallback_shape("Plain box");
    plain.name = Some("Rectangle 5".to_string());
    plain.id = Some("4".to_string());

    let outcomes = [
        slide_with(Some(solid(DARK_BG)), vec![plain.clone()]),
        Slide {
            theme: Some(SlideTheme {
                default_text_color: Some("1F3F5F".to_string()),
                major_font: None,
                minor_font: None,
                hlink_color: None,
            }),
            ..slide_with(Some(solid(DARK_BG)), vec![plain])
        },
    ];
    for slide in &outcomes {
        let compiled = compile(slide);
        assert_eq!(colour_of(&compiled, "Plain box"), [0, 0, 0, 255]);
    }
}

#[test]
fn fallback_shape_keeps_an_explicit_run_colour() {
    let mut shape = fallback_shape("Orange label");
    let run = shape
        .text_body
        .as_mut()
        .and_then(|body| body.paragraphs.first_mut())
        .and_then(|para| para.runs.first_mut())
        .expect("text run");
    if let TextRun::Text(data) = run {
        data.color = Some("ED7D31".to_string());
    }
    let slide = compile(&slide_with(Some(solid(DARK_BG)), vec![shape]));

    let colour = colour_of(&slide, "Orange label");
    assert_eq!(colour, [0xED, 0x7D, 0x31, 255]);
    assert_ne!(colour, [255, 255, 255, 255]);
}

#[test]
fn parsed_text_colour_chain_ignores_the_theme_default() {
    // ref model-source-text-colour.test.ts variant 'parser': the built-in
    // parser's chain paints the hard default. The reference fixture's
    // `defaultTextColor` is FFFFFF — white default text that must NOT be used
    // for a parsed chain.
    let mut shape = fallback_shape("Provenance");
    shape.name = Some("TextBox 1".to_string());
    shape.id = Some("1".to_string());
    let slide = Slide {
        theme: Some(SlideTheme {
            default_text_color: Some("FFFFFF".to_string()),
            major_font: None,
            minor_font: None,
            hlink_color: None,
        }),
        ..slide_with(Some(solid(DARK_BG)), vec![shape])
    };

    assert_eq!(colour_of(&compile(&slide), "Provenance"), [0, 0, 0, 255]);
}

#[test]
fn dark_smartart_fallback_uses_the_contrast_default() {
    // The parser's synthetic shape carries no id and no authored default
    // colour; on a dark slide its null-colour runs become white.
    let shape = fallback_shape("Task one");
    let slide = compile(&slide_with(Some(solid(DARK_BG)), vec![shape]));
    assert_eq!(colour_of(&slide, "Task one"), [255, 255, 255, 255]);
}
