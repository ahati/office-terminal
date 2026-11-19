//! SVG text emission parity.
//!
//! The compiler emits one `PositionedGlyphRun` per broken line, with `glyph.y`
//! set to that line's baseline and `glyph.x`/`glyph.advance` carrying the pen
//! positions the rasterizer paints (`src/text/paint.rs::emit_run`). The SVG
//! serializer must therefore place one `<text>` per run at the run's first
//! glyph x and the line baseline y, pin the run advance with `textLength` when
//! the placed glyphs are wider than their advance sum, and keep the outline
//! (`VectorGlyphs`) mode on the same origin/baseline.
//!
//! Reference behaviour ported from `renderer.ts:5783` (baseline = cursorY +
//! metric ascent) and `renderer.ts:5885-5925` (justified lines spread their
//! inter-word gaps).

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{
    AffineTransform, PositionedGlyph, PositionedGlyphRun, SvgTextMode, VectorCommand, VectorSlide,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::text::emit::compile_text_body;
use kitty_pptx_core::{AssetError, DefaultSlideSvgSerializer, SlideSvgSerializer};

const POINTS: f32 = 20.0;
/// Carlito is bundled, so glyph outlines resolve the same everywhere.
const FAMILY: &str = "Carlito";

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn svg_for(slide: &VectorSlide, mode: SvgTextMode) -> String {
    DefaultSlideSvgSerializer::new()
        .serialize_svg(slide, &NoAssets, mode)
        .expect("svg serialization failed")
}

/// One `<text>` element: its `x`/`y` attributes, raw attribute string and body.
struct TextEl {
    x: f32,
    y: f32,
    attrs: String,
    body: String,
}

fn attr_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(r#"{name}=""#);
    let start = line.find(&key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn text_elements(svg: &str) -> Vec<TextEl> {
    let mut out = Vec::new();
    for line in svg.lines() {
        let Some(pos) = line.find("<text ") else {
            continue;
        };
        let Some(open_end) = line[pos..].find('>').map(|i| pos + i) else {
            continue;
        };
        let attrs = &line[pos..open_end];
        let body = line[open_end + 1..]
            .split("</text>")
            .next()
            .unwrap_or("")
            .to_string();
        let x = attr_value(attrs, "x").and_then(|v| v.parse().ok());
        let y = attr_value(attrs, "y").and_then(|v| v.parse().ok());
        if let (Some(x), Some(y)) = (x, y) {
            out.push(TextEl {
                x,
                y,
                attrs: attrs.to_string(),
                body,
            });
        }
    }
    out
}

/// Every coordinate pair of an outline `<path d="...">`, in document order.
fn path_points(d: &str) -> Vec<(f32, f32)> {
    let mut numbers = Vec::new();
    let mut token = String::new();
    for ch in d.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            token.push(ch);
        } else if !token.is_empty() {
            if let Ok(value) = token.parse::<f32>() {
                numbers.push(value);
            }
            token.clear();
        }
    }
    if !token.is_empty() {
        if let Ok(value) = token.parse::<f32>() {
            numbers.push(value);
        }
    }
    numbers
        .chunks(2)
        .filter_map(|pair| match pair {
            [x, y] => Some((*x, *y)),
            _ => None,
        })
        .collect()
}

fn path_d_elements(svg: &str) -> Vec<String> {
    svg.lines()
        .filter_map(|line| attr_value(line, "d").map(|d| d.to_string()))
        .collect()
}

fn run(text: &str, x: f32, baseline_y: f32, advance: f32) -> PositionedGlyphRun {
    let glyphs = text
        .chars()
        .enumerate()
        .map(|(idx, _)| PositionedGlyph {
            glyph_id: 0,
            x: x + idx as f32 * advance,
            y: baseline_y,
            advance,
            warp_u: None,
        })
        .collect();
    PositionedGlyphRun {
        text: text.to_string(),
        font_family: FAMILY.to_string(),
        font_size_pt: POINTS,
        color: [0, 0, 0, 255],
        bold: false,
        italic: false,
        glyphs,
        decorations: Default::default(),
        single_face: false,
        warp: None,
        reflection: None,
    }
}

fn slide_with(runs: Vec<PositionedGlyphRun>) -> VectorSlide {
    let mut slide = VectorSlide::new(9_144_000, 6_858_000);
    slide.commands.push(VectorCommand::DrawText {
        runs,
        transform: AffineTransform::default(),
    });
    slide
}

/// A two-paragraph body compiled through the real layout/paint pass.
fn two_paragraph_body() -> pptx_model::TextBody {
    let paragraph = |text: &str| {
        format!(
            r#"{{
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
                    "fontFamily": "{FAMILY}"
                }}]
            }}"#
        )
    };
    let json = format!(
        r#"{{
            "verticalAnchor": "t",
            "defaultFontSize": {POINTS},
            "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
            "wrap": "square",
            "vert": "horz",
            "autoFit": "none",
            "numCol": 1, "spcCol": 0, "rtlCol": false, "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [{}, {}]
        }}"#,
        paragraph("Alpha line"),
        paragraph("Beta line")
    );
    serde_json::from_str(&json).expect("text body fixture")
}

fn compiled_runs() -> Vec<PositionedGlyphRun> {
    let (tx, _rx) = unbounded();
    let commands = compile_text_body(
        &two_paragraph_body(),
        4_000_000,
        2_000_000,
        &AffineTransform::identity(),
        None,
        &tx,
    );
    for command in commands {
        if let VectorCommand::DrawText { runs, .. } = command {
            return runs;
        }
    }
    panic!("expected a DrawText command");
}

#[test]
fn two_line_body_emits_one_text_per_line_with_distinct_baselines() {
    let runs = compiled_runs();
    assert_eq!(runs.len(), 2, "one run per broken line: {runs:?}");
    let baselines: Vec<f32> = runs
        .iter()
        .map(|r| r.glyphs.first().expect("placed glyph").y)
        .collect();
    assert!(
        (baselines[1] - baselines[0]).abs() > 1.0,
        "the two paragraphs sit on different baselines: {baselines:?}"
    );

    let svg = svg_for(&slide_with(runs.clone()), SvgTextMode::SelectableText);
    let texts = text_elements(&svg);
    assert_eq!(texts.len(), 2, "one <text> per run: {svg}");
    assert_eq!(texts[0].body, "Alpha line");
    assert_eq!(texts[1].body, "Beta line");

    for (el, run) in texts.iter().zip(runs.iter()) {
        let glyph = run.glyphs.first().expect("placed glyph");
        assert!(
            (el.x - glyph.x).abs() < 0.05,
            "run origin x: expected {}, got {}",
            glyph.x,
            el.x
        );
        assert!(
            (el.y - glyph.y).abs() < 0.05,
            "run baseline y: expected {}, got {}",
            glyph.y,
            el.y
        );
    }
    assert!(
        (texts[0].x - texts[1].x).abs() < 0.05,
        "left-aligned lines share x: {} vs {}",
        texts[0].x,
        texts[1].x
    );
    assert!(
        (texts[1].y - texts[0].y).abs() > 1.0,
        "lines must not overlap at one baseline"
    );
}

#[test]
fn runs_preserve_whitespace_and_never_reshape() {
    let r = run("  spaced  ", 500_000.0, 800_000.0, 100_000.0);
    let svg = svg_for(&slide_with(vec![r]), SvgTextMode::SelectableText);
    let texts = text_elements(&svg);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].attrs.contains(r#"xml:space="preserve""#),
        "authored spacing must survive XML whitespace collapsing: {}",
        texts[0].attrs
    );
    assert_eq!(texts[0].body, "  spaced  ");
}

#[test]
fn justified_run_pins_its_advance_with_text_length() {
    // A justified line spreads its inter-word gaps (renderer.ts:5917). Where a
    // gap falls inside a run, the placed glyphs are wider than their advance
    // sum, so the extent -- not the raw advances -- is the width SVG must keep.
    let natural = run("alpha beta", 1_000_000.0, 2_000_000.0, 100_000.0);
    let mut spread = run("alpha beta", 1_000_000.0, 3_000_000.0, 100_000.0);
    let gap = 50_000.0;
    for glyph in spread.glyphs.iter_mut().skip(6) {
        glyph.x += gap;
    }
    let expected_extent = 10.0 * 100_000.0 + gap;

    let svg = svg_for(
        &slide_with(vec![natural, spread]),
        SvgTextMode::SelectableText,
    );
    let texts = text_elements(&svg);
    assert_eq!(texts.len(), 2, "{svg}");

    assert!(
        !texts[0].attrs.contains("textLength"),
        "a contiguous run keeps its natural advance: {}",
        texts[0].attrs
    );
    assert!(
        texts[1].attrs.contains(&format!(r#"textLength="{expected_extent:.1}""#)),
        "justified run must pin the placed extent: {}",
        texts[1].attrs
    );
    assert!(
        texts[1]
            .attrs
            .contains(r#"lengthAdjust="spacingAndGlyphs""#),
        "pinned advance must rescale the glyphs too: {}",
        texts[1].attrs
    );
    let pinned = attr_value(&texts[1].attrs, "textLength").expect("textLength value");
    assert_eq!(pinned, format!("{expected_extent:.1}"));
}

#[test]
fn decoration_fills_and_face_are_kept() {
    let mut r = run("Underlined", 300_000.0, 900_000.0, 120_000.0);
    let line = kitty_pptx_core::contracts::DecorationLine {
        color: [10, 20, 30, 128],
        top_emu: -152_400.0,
        thickness_emu: 12_700.0,
        gap_emu: 0.0,
        style: kitty_pptx_core::contracts::DecorationStyle::Solid,
    };
    r.decorations = kitty_pptx_core::contracts::TextDecorations {
        underline: Some(line),
        strike: Some(line),
    };
    r.color = [10, 20, 30, 128];
    r.bold = true;
    r.italic = true;

    let svg = svg_for(&slide_with(vec![r]), SvgTextMode::SelectableText);
    let texts = text_elements(&svg);
    assert_eq!(texts.len(), 1);
    let attrs = &texts[0].attrs;
    assert!(
        attrs.contains(r#"text-decoration="underline line-through""#),
        "{attrs}"
    );
    assert!(attrs.contains(r#"font-family="Carlito""#), "{attrs}");
    assert!(
        attrs.contains(&format!(r#"font-size="{:.1}""#, POINTS * 12700.0)),
        "{attrs}"
    );
    assert!(attrs.contains(r#"fill="rgb(10,20,30)""#), "{attrs}");
    assert!(attrs.contains(r#"fill-opacity="0.50""#), "{attrs}");
    assert!(attrs.contains(r#"font-weight="bold""#), "{attrs}");
    assert!(attrs.contains(r#"font-style="italic""#), "{attrs}");
}

#[test]
fn outline_mode_keeps_the_run_origin_and_baseline() {
    // Two runs on two baselines: the outline must move with the line, exactly
    // like the `<text>` mode, and start at the run's pen x.
    let origin = 1_000_000.0;
    let first_baseline = 2_000_000.0;
    let second_baseline = 2_600_000.0;
    let runs = vec![
        run("H", origin, first_baseline, 600_000.0),
        run("H", origin, second_baseline, 600_000.0),
    ];
    let slide = slide_with(runs);

    let outlines = svg_for(&slide, SvgTextMode::VectorGlyphs);
    let ds = path_d_elements(&outlines);
    assert_eq!(ds.len(), 2, "one outline path per run: {outlines}");
    assert!(
        !outlines.contains("<text "),
        "outlines must not fall back to selectable text: {outlines}"
    );

    let em = POINTS * 12700.0;
    let mut bottoms = Vec::new();
    for (d, baseline) in ds.iter().zip([first_baseline, second_baseline]) {
        let points = path_points(d);
        assert!(!points.is_empty(), "empty outline: {d}");
        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        // The pen x is the run origin; the left side bearing stays inside a
        // quarter em.
        assert!(
            min_x >= origin - 1.0 && min_x - origin < em * 0.25,
            "outline must start at the run origin {origin}, got {min_x}"
        );
        // A capital sits on the baseline: no descender, cap height above it.
        assert!(
            (baseline - max_y).abs() < em * 0.1,
            "outline baseline {baseline}, glyph bottom {max_y}"
        );
        assert!(
            min_y < baseline - em * 0.5,
            "outline must climb above the baseline {baseline}, top {min_y}"
        );
        bottoms.push(max_y);
    }
    assert!(
        (bottoms[1] - bottoms[0] - (second_baseline - first_baseline)).abs() < em * 0.1,
        "second line outline must follow its own baseline: {bottoms:?}"
    );

    // The selectable mode agrees: same origin, same baselines.
    let texts = text_elements(&svg_for(&slide, SvgTextMode::SelectableText));
    assert_eq!(texts.len(), 2);
    for (text, baseline) in texts.iter().zip([first_baseline, second_baseline]) {
        assert!((text.x - origin).abs() < 0.05);
        assert!((text.y - baseline).abs() < 0.05);
    }
}


#[test]
fn vector_glyphs_paint_the_same_decoration_bands() {
    let mut r = run("Decorated", 300_000.0, 900_000.0, 120_000.0);
    let underline = kitty_pptx_core::contracts::DecorationLine {
        color: [255, 0, 0, 255],
        top_emu: 76_200.0,
        thickness_emu: 25_400.0,
        gap_emu: 0.0,
        style: kitty_pptx_core::contracts::DecorationStyle::Solid,
    };
    let strike = kitty_pptx_core::contracts::DecorationLine {
        color: [0, 0, 0, 255],
        top_emu: -76_200.0,
        thickness_emu: 25_400.0,
        gap_emu: 25_400.0,
        style: kitty_pptx_core::contracts::DecorationStyle::Solid,
    };
    r.decorations = kitty_pptx_core::contracts::TextDecorations {
        underline: Some(underline),
        strike: Some(strike),
    };

    let svg = svg_for(&slide_with(vec![r]), SvgTextMode::VectorGlyphs);
    assert!(svg.contains("<path "), "glyph outlines render");
    assert_eq!(
        svg.matches("<rect x=").count(),
        3,
        "underline + double strike"
    );
    assert!(svg.contains(r#"fill="rgb(255,0,0)""#), "underline colour");
    assert!(!svg.contains("text-decoration"), "no CSS decoration in outline mode");
}
