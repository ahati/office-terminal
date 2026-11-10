//! Live OMML equation typesetting (PRD 09).
//!
//! The engine compiles the `pptx-model` math AST into a box-and-glue tree
//! ([`box_model`]), lays it out with TeX Appendix G rules ([`layout`],
//! [`layout_atoms`], [`layout_structures`]) and flattens it into the two native
//! vector backends:
//!
//! * glyph runs → [`VectorCommand::DrawText`] (`PositionedGlyphRun`)
//! * rules, delimiters, braces, borders → [`VectorCommand::DrawPath`]
//!
//! Text layout (`compiler::text`) consumes [`MathPiece`] values: inline math is
//! appended to the running line box, display math becomes its own centred line
//! block.

pub mod box_model;
pub mod layout;
pub mod layout_atoms;
pub mod layout_structures;
pub mod paths;

use crossbeam_channel::Sender;
use ooxml_common::math::MathNode;

use crate::contracts::{
    AffineTransform, LineCap, LineJoin, PathSegment, PositionedGlyph, PositionedGlyphRun,
    VectorCommand, VectorPaint, VectorPath, VectorStroke, WarningEvent,
};

use box_model::{glyph_width, MathBox, MathBoxKind, MathGlyphRun, MathMetrics, MathStyle};

/// How a synthesized math path is painted.
#[derive(Clone, Debug, PartialEq)]
pub enum MathPathPaint {
    /// Filled solid (fraction bars, radical vinculums, over/under bars).
    Fill([u8; 4]),
    /// Stroked outline (extensible delimiters, braces, borders, arrows).
    Stroke { color: [u8; 4], width: f32 },
}

/// A synthesized math path plus its paint, in baseline-relative y-down coords.
#[derive(Clone, Debug, PartialEq)]
pub struct MathPath {
    pub path: VectorPath,
    pub paint: MathPathPaint,
}

/// Parameters for one equation run.
#[derive(Clone, Copy, Debug)]
pub struct MathParams<'a> {
    /// `true` for `m:oMathPara` block math, `false` for inline `m:oMath`.
    pub display: bool,
    pub font_size_pt: f32,
    pub color: [u8; 4],
    /// Authoring-time plain-text preview used when layout degrades.
    pub fallback: Option<&'a str>,
}

impl Default for MathParams<'_> {
    fn default() -> Self {
        Self {
            display: false,
            font_size_pt: 18.0,
            color: [0, 0, 0, 255],
            fallback: None,
        }
    }
}

/// A laid-out equation fragment ready to flow into a text line.
#[derive(Clone, Debug, Default)]
pub struct MathPiece {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub runs: Vec<PositionedGlyphRun>,
    pub paths: Vec<MathPath>,
    /// A resource guard tripped; the content is the fallback/partial prefix.
    pub degraded: bool,
}

impl MathPiece {
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.paths.is_empty() && self.width <= 0.0
    }
}

/// Destination of a flattened equation in slide coordinates.
#[derive(Clone, Copy, Debug)]
pub struct MathPlacement {
    pub x: f32,
    /// Slide-space y of the equation baseline.
    pub baseline_y: f32,
    pub transform: AffineTransform,
}

impl MathPlacement {
    pub fn new(x: f32, baseline_y: f32, transform: AffineTransform) -> Self {
        Self {
            x,
            baseline_y,
            transform,
        }
    }
}

/// Typeset `nodes` into a flowable [`MathPiece`] (PRD 09 §5 fallbacks applied).
pub fn layout_math(
    nodes: &[MathNode],
    params: MathParams<'_>,
    warning_sink: &Sender<WarningEvent>,
) -> MathPiece {
    let style = if params.display {
        MathStyle::display()
    } else {
        MathStyle::text()
    };
    let mut ctx = layout::LayoutCtx::new(params.font_size_pt, params.color, warning_sink);
    let mut root = layout::layout_nodes(nodes, style, &mut ctx, 0);
    let metrics = MathMetrics::root(params.font_size_pt);

    let mut degraded = false;
    if let Some(limit) = ctx.breach {
        let _ = warning_sink.send(WarningEvent::ResourceLimitHit { limit });
        degraded = true;
        // PRD 09 §5.1: prefer the authored preview text; otherwise keep the
        // partially laid-out prefix and mark it with an ellipsis.
        if let Some(fallback) = params.fallback.filter(|text| !text.trim().is_empty()) {
            let text: String = fallback.chars().take(200).collect();
            root = MathBox::glyph_run(&text, metrics.size_pt, params.color);
        } else {
            root = MathBox::hbox(vec![
                root,
                MathBox::space(metrics.thin_space),
                MathBox::glyph_run("\u{2026}", metrics.size_pt, params.color),
            ]);
        }
    }

    let mut piece = MathPiece {
        width: root.width,
        ascent: root.height,
        descent: root.depth,
        degraded,
        ..Default::default()
    };
    flatten_box(&root, 0.0, 0.0, params.color, &mut piece);
    piece
}

/// Compile one equation into native display-list commands.
pub fn compile_math_element(
    nodes: &[MathNode],
    params: MathParams<'_>,
    placement: &MathPlacement,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let piece = layout_math(nodes, params, warning_sink);
    let mut commands = Vec::with_capacity(piece.paths.len() + 1);
    for path in &piece.paths {
        let shifted = translate_math_path(path, placement.x, placement.baseline_y);
        commands.push(math_path_command(&shifted, &placement.transform));
    }
    if !piece.runs.is_empty() {
        let runs = offset_runs(piece.runs, placement.x, placement.baseline_y);
        commands.push(VectorCommand::DrawText {
            runs,
            transform: placement.transform,
        });
    }
    commands
}

/// Shift a path set horizontally (used when flowing math inside a line).
pub fn offset_math_paths(paths: Vec<MathPath>, dx: f32) -> Vec<MathPath> {
    paths
        .into_iter()
        .map(|path| translate_math_path(&path, dx, 0.0))
        .collect()
}

/// Shift already-flattened runs horizontally.
pub fn offset_runs(runs: Vec<PositionedGlyphRun>, dx: f32, dy: f32) -> Vec<PositionedGlyphRun> {
    runs.into_iter()
        .map(|mut run| {
            for glyph in &mut run.glyphs {
                glyph.x += dx;
                glyph.y += dy;
            }
            run
        })
        .collect()
}

/// Translate a math path by `(dx, dy)`.
pub fn translate_math_path(path: &MathPath, dx: f32, dy: f32) -> MathPath {
    MathPath {
        path: translate_path(&path.path, dx, dy),
        paint: path.paint.clone(),
    }
}

/// Translate every segment of a vector path.
pub fn translate_path(path: &VectorPath, dx: f32, dy: f32) -> VectorPath {
    let segments = path
        .segments
        .iter()
        .map(|segment| match *segment {
            PathSegment::MoveTo { x, y } => PathSegment::MoveTo {
                x: x + dx,
                y: y + dy,
            },
            PathSegment::LineTo { x, y } => PathSegment::LineTo {
                x: x + dx,
                y: y + dy,
            },
            PathSegment::QuadTo { x1, y1, x, y } => PathSegment::QuadTo {
                x1: x1 + dx,
                y1: y1 + dy,
                x: x + dx,
                y: y + dy,
            },
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => PathSegment::CubicTo {
                x1: x1 + dx,
                y1: y1 + dy,
                x2: x2 + dx,
                y2: y2 + dy,
                x: x + dx,
                y: y + dy,
            },
            PathSegment::Close => PathSegment::Close,
        })
        .collect();
    VectorPath::new(segments, path.fill_rule)
}

/// Convert a painted math path into a display-list command.
pub fn math_path_command(path: &MathPath, transform: &AffineTransform) -> VectorCommand {
    match &path.paint {
        MathPathPaint::Fill(color) => VectorCommand::draw_path(
            path.path.clone(),
            Some(VectorPaint::Solid(*color)),
            None,
            *transform,
        ),
        MathPathPaint::Stroke { color, width } => VectorCommand::draw_path(
            path.path.clone(),
            None,
            Some(VectorStroke {
                color: *color,
                width_emu: width.max(1.0) as i64,
                dash_segments: None,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
            }),
            *transform,
        ),
    }
}

fn flatten_box(b: &MathBox, x: f32, up: f32, color: [u8; 4], out: &mut MathPiece) {
    match &b.kind {
        MathBoxKind::Empty | MathBoxKind::Space => {}
        MathBoxKind::HBox(children) => {
            for placed in children {
                flatten_box(&placed.child, x + placed.dx, up, color, out);
            }
        }
        MathBoxKind::VBox(children) => {
            for placed in children {
                flatten_box(&placed.child, x + placed.dx, up + placed.dy, color, out);
            }
        }
        MathBoxKind::Glyphs(run) => out.runs.push(glyph_run_to_placed(run, x, up)),
        MathBoxKind::Rule { thickness, color } => {
            out.paths.push(MathPath {
                path: box_model::rect_path(x, -up - thickness, b.width, *thickness),
                paint: MathPathPaint::Fill(*color),
            });
        }
        MathBoxKind::Paths { paths, thickness } => {
            for path in paths {
                out.paths.push(MathPath {
                    path: translate_path(path, x, -up),
                    paint: MathPathPaint::Stroke {
                        color,
                        width: *thickness,
                    },
                });
            }
        }
    }
}

fn glyph_run_to_placed(run: &MathGlyphRun, x: f32, up: f32) -> PositionedGlyphRun {
    let em = run.font_size_pt * 12700.0;
    let mut glyphs = Vec::with_capacity(run.text.chars().count());
    let mut cursor = 0.0f32;
    for (i, ch) in run.text.chars().enumerate() {
        let advance = glyph_width(ch, em);
        glyphs.push(PositionedGlyph {
            glyph_id: i.min(u16::MAX as usize) as u16,
            x: x + cursor,
            y: -up,
            advance,
        });
        cursor += advance;
    }
    PositionedGlyphRun {
        text: run.text.clone(),
        font_family: run.font_family.to_string(),
        font_size_pt: run.font_size_pt,
        color: run.color,
        glyphs,
        underline: false,
        strikethrough: false,
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::*;

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

    #[test]
    fn fraction_flattens_to_rule_plus_two_glyph_groups() {
        let (tx, _rx) = unbounded();
        let piece = layout_math(&[fraction()], MathParams::default(), &tx);
        assert!(!piece.degraded);
        assert_eq!(piece.paths.len(), 1, "fraction bar");
        assert_eq!(piece.runs.len(), 2, "numerator + denominator");
        assert!(piece.width > 0.0 && piece.ascent > 0.0 && piece.descent > 0.0);
        assert!(matches!(piece.paths[0].paint, MathPathPaint::Fill(_)));
    }

    #[test]
    fn script_glyphs_are_scaled_to_seventy_percent() {
        let (tx, _rx) = unbounded();
        let node = MathNode::Sup {
            base: vec![run("x")],
            sup: vec![run("2")],
        };
        let piece = layout_math(&[node], MathParams::default(), &tx);
        let sup = piece
            .runs
            .iter()
            .find(|r| r.text == "2")
            .expect("superscript run");
        assert!((sup.font_size_pt - 18.0 * 0.70).abs() < 0.01);
    }

    #[test]
    fn delimiter_produces_stroked_paths() {
        let (tx, _rx) = unbounded();
        let node = MathNode::Delimiter {
            beg_char: "(".to_string(),
            end_char: ")".to_string(),
            items: vec![vec![run("x")]],
        };
        let piece = layout_math(&[node], MathParams::default(), &tx);
        assert!(piece.paths.len() >= 2);
        assert!(piece
            .paths
            .iter()
            .all(|p| matches!(p.paint, MathPathPaint::Stroke { .. })));
    }

    #[test]
    fn depth_guard_degrades_without_panicking() {
        let (tx, _rx) = unbounded();
        let mut node = run("x");
        for _ in 0..(layout::MAX_MATH_DEPTH + 10) {
            node = MathNode::Fraction {
                num: vec![node],
                den: Vec::new(),
                bar: None,
            };
        }
        let piece = layout_math(&[node], MathParams::default(), &tx);
        assert!(piece.degraded);
        assert!(!piece.runs.is_empty(), "ellipsis marker emitted");
    }

    #[test]
    fn degraded_uses_fallback_text_when_available() {
        let (tx, _rx) = unbounded();
        let mut node = run("x");
        for _ in 0..(layout::MAX_MATH_DEPTH + 10) {
            node = MathNode::Group { items: vec![node] };
        }
        let params = MathParams {
            fallback: Some("x+y"),
            ..MathParams::default()
        };
        let piece = layout_math(&[node], params, &tx);
        assert!(piece.degraded);
        assert_eq!(piece.runs.len(), 1);
        assert_eq!(piece.runs[0].text, "x+y");
    }

    #[test]
    fn display_math_is_taller_than_inline() {
        let (tx, _rx) = unbounded();
        let inline = layout_math(&[fraction()], MathParams::default(), &tx);
        let display = layout_math(
            &[fraction()],
            MathParams {
                display: true,
                ..MathParams::default()
            },
            &tx,
        );
        assert!(display.ascent + display.descent > inline.ascent + inline.descent);
    }

    #[test]
    fn translate_path_shifts_all_segments() {
        let path = box_model::rect_path(0.0, 0.0, 10.0, 10.0);
        let moved = translate_path(&path, 5.0, 7.0);
        match moved.segments.first() {
            Some(PathSegment::MoveTo { x, y }) => {
                assert!((*x - 5.0).abs() < 1e-6);
                assert!((*y - 7.0).abs() < 1e-6);
            }
            other => panic!("unexpected first segment: {other:?}"),
        }
    }

    #[test]
    fn compile_element_emits_paths_before_text() {
        let (tx, _rx) = unbounded();
        let placement = MathPlacement::new(100.0, 200.0, AffineTransform::identity());
        let commands = compile_math_element(&[fraction()], MathParams::default(), &placement, &tx);
        assert!(matches!(commands[0], VectorCommand::DrawPath { .. }));
        assert!(matches!(
            commands.last(),
            Some(VectorCommand::DrawText { .. })
        ));
    }
}
