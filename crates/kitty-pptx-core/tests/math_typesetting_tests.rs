//! PRD 09 — live OMML equation typesetting engine integration tests.
//!
//! Slides are deserialized from `pptx-model` JSON (the parser output shape) and
//! compiled through the public `DefaultSlideCompiler`, mirroring the existing
//! text/table test style.

use crossbeam_channel::Receiver;
use kitty_pptx_core::*;
use ooxml_common::math::MathNode;
use pptx_model::*;

/// A single 2,000,000 × 1,000,000 EMU rectangle with an empty text body, no
/// insets and a single paragraph. The paragraph's run list is replaced by the
/// math run under test.
const SLIDE_JSON: &str = r#"{
  "index": 0,
  "slideNumber": 1,
  "elements": [
    {
      "type": "shape",
      "x": 0,
      "y": 0,
      "width": 2000000,
      "height": 1000000,
      "rotation": 0.0,
      "flipH": false,
      "flipV": false,
      "geometry": "rect",
      "textBody": {
        "verticalAnchor": "t",
        "lIns": 0,
        "rIns": 0,
        "tIns": 0,
        "bIns": 0,
        "wrap": "square",
        "vert": "horz",
        "autoFit": "none",
        "fontScale": 1.0,
        "numCol": 1,
        "spcCol": 0,
        "rtlCol": false,
        "spcFirstLastPara": false,
        "anchorCtr": false,
        "paragraphs": [
          {
            "alignment": "l",
            "marL": 0,
            "marR": 0,
            "indent": 0,
            "lvl": 0,
            "bullet": { "type": "none" },
            "defFontSize": 24.0,
            "defColor": "000000",
            "defFontFamily": "Calibri",
            "tabStops": [],
            "rtl": false,
            "eaLnBrk": true,
            "runs": []
          }
        ]
      }
    }
  ],
  "elementSources": [],
  "comments": []
}"#;

fn run(text: &str) -> MathNode {
    MathNode::Run {
        text: text.to_string(),
        style: "italic".to_string(),
        script: None,
    }
}

fn fraction() -> MathNode {
    MathNode::Fraction {
        num: vec![run("1")],
        den: vec![run("x")],
        bar: None,
    }
}

/// Compile a slide whose only paragraph content is the supplied math run.
fn compile_math(
    nodes: Vec<MathNode>,
    display: bool,
    fallback: Option<&str>,
) -> (VectorSlide, Receiver<WarningEvent>) {
    let mut slide: Slide = serde_json::from_str(SLIDE_JSON).expect("slide fixture");
    if let SlideElement::Shape(shape) = &mut slide.elements[0] {
        let body = shape.text_body.as_mut().expect("text body");
        body.paragraphs[0].runs = vec![TextRun::Math {
            fallback_text: fallback.map(str::to_string),
            nodes,
            display,
            font_size: Some(24.0),
            color: Some("112233".to_string()),
        }];
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let vs = DefaultSlideCompiler::new()
        .compile(&slide, 2000000, 1000000, &tx)
        .expect("compile");
    (vs, rx)
}

fn math_paths(vs: &VectorSlide, filled: bool) -> Vec<&contracts::VectorPath> {
    vs.commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path, fill, stroke, ..
            } => {
                let hit = if filled {
                    fill.is_some()
                } else {
                    stroke.is_some()
                };
                hit.then_some(path)
            }
            _ => None,
        })
        .collect()
}

fn text_runs(vs: &VectorSlide) -> Vec<PositionedGlyphRun> {
    vs.commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn fraction_emits_a_filled_rule_and_two_glyph_groups() {
    let (vs, _rx) = compile_math(vec![fraction()], false, None);

    // Exactly one filled path: the fraction bar (the rectangle itself is unfilled).
    let filled = math_paths(&vs, true);
    assert_eq!(filled.len(), 1, "one fraction rule");
    let rule_w = filled[0]
        .segments
        .iter()
        .fold(0.0f32, |acc, seg| match seg {
            contracts::PathSegment::MoveTo { x, .. }
            | contracts::PathSegment::LineTo { x, .. }
            | contracts::PathSegment::QuadTo { x, .. }
            | contracts::PathSegment::CubicTo { x, .. } => acc.max(*x),
            contracts::PathSegment::Close => acc,
        });
    assert!(rule_w > 0.0, "rule has a positive width");

    let runs = text_runs(&vs);
    let texts: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
    assert!(texts.contains(&"1"), "numerator glyph group: {texts:?}");
    assert!(texts.contains(&"x"), "denominator glyph group: {texts:?}");
    assert_eq!(runs.len(), 2, "exactly two glyph groups");
    assert_eq!(runs[0].color, [0x11, 0x22, 0x33, 255], "equation colour");
}

#[test]
fn sub_and_superscript_glyphs_scale_to_seventy_percent() {
    let node = MathNode::SubSup {
        base: vec![run("X")],
        sub: vec![run("i")],
        sup: vec![run("2")],
    };
    let (vs, _rx) = compile_math(vec![node], false, None);
    let runs = text_runs(&vs);
    let base = runs.iter().find(|r| r.text == "X").expect("base run");
    let sub = runs.iter().find(|r| r.text == "i").expect("sub run");
    let sup = runs.iter().find(|r| r.text == "2").expect("sup run");
    assert!((base.font_size_pt - 24.0).abs() < 0.01);
    assert!((sub.font_size_pt - 24.0 * 0.70).abs() < 0.01);
    assert!((sup.font_size_pt - 24.0 * 0.70).abs() < 0.01);
    // Scripts sit vertically offset from the base baseline.
    let base_y = base.glyphs[0].y;
    assert!(sup.glyphs[0].y < base_y, "superscript above baseline");
    assert!(sub.glyphs[0].y > base_y, "subscript below baseline");
}

#[test]
fn nested_fraction_depth_guard_degrades_without_panicking() {
    let mut node = run("x");
    for _ in 0..120 {
        node = MathNode::Fraction {
            num: vec![node],
            den: Vec::new(),
            bar: None,
        };
    }
    let (vs, rx) = compile_math(vec![node], false, None);
    assert!(!vs.commands.is_empty(), "partial equation still renders");
    let warning = rx.try_iter().find_map(|event| match event {
        WarningEvent::ResourceLimitHit { limit } => Some(limit),
        _ => None,
    });
    assert_eq!(warning, Some("math_depth"));
}

#[test]
fn delimiter_emits_stroked_bracket_paths() {
    let node = MathNode::Delimiter {
        beg_char: "(".to_string(),
        end_char: ")".to_string(),
        items: vec![vec![run("x")]],
    };
    let (vs, _rx) = compile_math(vec![node], false, None);
    let stroked = math_paths(&vs, false);
    assert!(
        stroked.len() >= 2,
        "left and right delimiters must be stroked paths"
    );
}

#[test]
fn display_math_is_centred_and_taller_than_inline() {
    let (inline, _rx) = compile_math(vec![fraction()], false, None);
    let (display, _rx2) = compile_math(vec![fraction()], true, None);

    let inline_runs = text_runs(&inline);
    let display_runs = text_runs(&display);
    assert!(!inline_runs.is_empty() && !display_runs.is_empty());

    let inline_x = inline_runs
        .iter()
        .map(|r| r.glyphs[0].x)
        .fold(f32::INFINITY, f32::min);
    let display_x = display_runs
        .iter()
        .map(|r| r.glyphs[0].x)
        .fold(f32::INFINITY, f32::min);
    assert!(
        display_x > inline_x,
        "display math must be centred: inline={inline_x}, display={display_x}"
    );

    let inline_y = inline_runs[0].glyphs[0].y;
    let display_y = display_runs[0].glyphs[0].y;
    assert!(
        display_y > inline_y,
        "display math occupies a taller block: inline={inline_y}, display={display_y}"
    );
}

#[test]
fn display_and_inline_produce_different_block_heights() {
    let (inline, _rx) = compile_math(vec![fraction()], false, None);
    let (display, _rx2) = compile_math(vec![fraction()], true, None);

    let inline_bottom = text_runs(&inline)
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.y))
        .fold(f32::NEG_INFINITY, f32::max);
    let display_bottom = text_runs(&display)
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.y))
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(display_bottom > inline_bottom);
}

#[test]
fn degenerate_nodes_do_not_panic() {
    let nodes = vec![
        MathNode::Fraction {
            num: Vec::new(),
            den: Vec::new(),
            bar: None,
        },
        MathNode::Delimiter {
            beg_char: String::new(),
            end_char: String::new(),
            items: Vec::new(),
        },
        MathNode::Radical {
            index: Vec::new(),
            radicand: Vec::new(),
        },
        MathNode::Array {
            rows: Vec::new(),
            align: "center".to_string(),
        },
        MathNode::Nary {
            op: String::new(),
            lim_loc: String::new(),
            sub: Vec::new(),
            sup: Vec::new(),
            body: Vec::new(),
        },
        MathNode::Accent {
            chr: String::new(),
            base: Vec::new(),
        },
        MathNode::SPre {
            sub: Vec::new(),
            sup: Vec::new(),
            base: Vec::new(),
        },
    ];
    let (vs, _rx) = compile_math(nodes, false, None);
    // The shape command is always present; nothing may panic and no NaN leaks.
    assert!(vs.commands.iter().all(|command| match command {
        VectorCommand::DrawText { runs, .. } => runs
            .iter()
            .all(|r| r.glyphs.iter().all(|g| g.x.is_finite() && g.y.is_finite())),
        _ => true,
    }));
}

#[test]
fn empty_math_node_list_yields_no_math_commands() {
    let (vs, _rx) = compile_math(Vec::new(), false, None);
    assert!(text_runs(&vs).is_empty());
    assert!(math_paths(&vs, true).is_empty());
}

#[test]
fn nary_display_limits_are_stacked_above_and_below() {
    let node = MathNode::Nary {
        op: "\u{2211}".to_string(),
        lim_loc: "undOvr".to_string(),
        sub: vec![run("i=1")],
        sup: vec![run("n")],
        body: vec![run("x")],
    };
    let (vs, _rx) = compile_math(vec![node], true, None);
    let runs = text_runs(&vs);
    let sub = runs.iter().find(|r| r.text == "i=1").expect("lower limit");
    let sup = runs.iter().find(|r| r.text == "n").expect("upper limit");
    let body = runs.iter().find(|r| r.text == "x").expect("body");
    let body_y = body.glyphs[0].y;
    assert!(sup.glyphs[0].y < body_y, "upper limit above the operator");
    assert!(sub.glyphs[0].y > body_y, "lower limit below the operator");
}

#[test]
fn accent_and_border_box_emit_geometry() {
    let acc = MathNode::Accent {
        chr: "\u{0304}".to_string(),
        base: vec![run("x")],
    };
    let bordered = MathNode::BorderBox {
        hide_top: false,
        hide_bot: false,
        hide_left: false,
        hide_right: false,
        strike_h: false,
        strike_v: false,
        strike_bltr: false,
        strike_tlbr: false,
        base: vec![run("y")],
    };
    let (vs, _rx) = compile_math(vec![acc, bordered], false, None);
    assert!(!math_paths(&vs, true).is_empty(), "accent bar rule");
    assert!(
        math_paths(&vs, false).len() >= 4,
        "border edges are stroked paths"
    );
}

#[test]
fn fallback_text_replaces_a_degraded_equation() {
    let mut node = run("x");
    for _ in 0..120 {
        node = MathNode::Group { items: vec![node] };
    }
    let (vs, _rx) = compile_math(vec![node], false, Some("a+b"));
    let runs = text_runs(&vs);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "a+b");
}

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("no assets in test".to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

#[test]
fn compiled_math_rasterizes_and_exports_svg() {
    let (vs, _rx) = compile_math(vec![fraction()], true, None);
    let store = NoAssets;

    // Stage 2 raster: glyphs + fraction rule reach the framebuffer.
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 640,
        height_px: 320,
    };
    let rendered = rasterizer
        .rasterize(&vs, &target, &store)
        .expect("math slide rasterizes");
    assert_eq!(rendered.pixels.len(), 640 * 320 * 4);

    // Stage 2 SVG: selectable math text plus the vector fraction rule.
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&vs, &store, SvgTextMode::SelectableText)
        .expect("math slide exports to svg");
    assert!(svg.contains("<text"), "math glyphs are selectable text");
    assert!(svg.contains("<path"), "fraction rule is a vector path");
}
