use std::cell::RefCell;
use std::collections::HashMap;

use crossbeam_channel::Sender;

use crate::compiler::math::{self, MathParams, MathPiece};
use crate::compiler::parse_hex_color;
use crate::compiler::text::{map_bullet_char, resolve_font_family};
use crate::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand, WarningEvent};
use crate::text::linebreak::{break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem};
use crate::text::measure::{StyleId, TabAlign, TabStop, TextMeasure};
use crate::text::metrics;
use crate::text::shape;
use crate::text::vertical::{self, VertMode};

use super::paint;

/// One authored text segment's resolved styling.
#[derive(Clone, Debug)]
pub(super) struct StyleDef {
    pub(super) family: String,
    /// Authored size in EMU: owns the line box (`natural_line_box`).
    pub(super) size_emu: f32,
    pub(super) size_pt: f32,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) color: [u8; 4],
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
    /// `rPr@baseline` in thousandths of a point: positive = superscript
    /// (raised), negative = subscript (lowered). `0`/`None` = on the baseline.
    pub(super) baseline: Option<i32>,
}

/// Measurement adapter: `advance()` shapes with the same face the painter uses.
pub(super) struct Measurer {
    styles: Vec<StyleDef>,
    cache: RefCell<HashMap<(usize, String), f32>>,
}

impl Measurer {
    fn new() -> Self {
        Self {
            styles: Vec::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn push(&mut self, style: StyleDef) -> StyleId {
        self.styles.push(style);
        StyleId(self.styles.len() - 1)
    }

    pub(super) fn style(&self, id: StyleId) -> &StyleDef {
        &self.styles[id.0]
    }
}

impl TextMeasure for Measurer {
    fn advance(&self, text: &str, style: StyleId) -> f32 {
        let key = (style.0, text.to_string());
        if let Some(width) = self.cache.borrow().get(&key).copied() {
            return width;
        }
        let def = self.style(style);
        // The breaker measures with the face the painter draws (`seg.font` is
        // built from `drawSizePx`, `renderer.ts:1859-1877`).
        let width = shape::measure(text, &def.family, def.draw_size_emu(), def.bold, def.italic);
        self.cache.borrow_mut().insert(key, width);
        width
    }

    fn space(&self, style: StyleId) -> f32 {
        self.advance(" ", style)
    }
}

pub(super) struct ParagraphLayout {
    pub(super) items: Vec<BreakItem>,
    /// Math pieces in `BreakItem::Object` order.
    pub(super) math: Vec<MathPiece>,
    pub(super) alignment: String,
    mar_l: f32,
    mar_r: f32,
    indent: f32,
    pub(super) rtl: bool,
    /// `a:pPr/a:tabLst` stops, already in EMU from the text-inset edge.
    tab_stops: Vec<TabStop>,
    /// `a:pPr@defTabSz` else PowerPoint's 1-inch grid (renderer.ts:5704).
    tab_size: f32,
    /// `a:pPr@eaLnBrk` (ECMA-376 §21.1.2.2.7, default true).
    ea_ln_brk: bool,
    space_line: metrics::SpaceLine,
    space_before: metrics::SpaceLine,
    space_after: metrics::SpaceLine,
    pub(super) default_style: StyleId,
    pub(super) object_cursor: RefCell<usize>,
}

pub(super) struct LineLayout {
    para_index: usize,
    pub(super) items: Vec<LineItem>,
    pub(super) width: f32,
    ascent: f32,
    descent: f32,
    top_gap: f32,
    after_gap: f32,
    indent: f32,
    first_line: bool,
    /// Last line of its paragraph (`isLastLine` in the draw loop).
    pub(super) is_last_line: bool,
    /// Closed by an authored `<a:br/>` (`line.endsWithBreak`).
    pub(super) ends_with_break: bool,
}

fn space_of(value: &Option<ooxml_common::text::SpaceLine>) -> metrics::SpaceLine {
    match value {
        Some(ooxml_common::text::SpaceLine::Pct { val }) => Some((true, *val as f32)),
        Some(ooxml_common::text::SpaceLine::Pts { val }) => Some((false, *val as f32)),
        None => None,
    }
}

fn pts_of_hundredths(value: Option<i64>) -> metrics::SpaceLine {
    value.map(|v| (false, v as f32 / 100.0))
}

fn pct_of_thousandths(value: Option<f64>) -> metrics::SpaceLine {
    value.map(|v| (true, v as f32))
}

/// `renderTextBody` for bodies without an `a:prstTxWarp`.
#[allow(clippy::too_many_arguments)]
pub fn compile_text_body(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    default_shape_text_color: Option<&str>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let l_ins = text_body.l_ins as f32;
    let r_ins = text_body.r_ins as f32;
    let t_ins = text_body.t_ins as f32;
    let b_ins = text_body.b_ins as f32;
    let font_scale = text_body.font_scale.unwrap_or(1.0) as f32;
    let reduction = text_body.ln_spc_reduction.unwrap_or(0.0) as f32;

    let mut measurer = Measurer::new();
    let default_color = parse_hex_color(default_shape_text_color.unwrap_or("000000"));
    let body_size = text_body.default_font_size.unwrap_or(18.0) as f32;
    let default_style = measurer.push(StyleDef {
        family: "Calibri".to_string(),
        size_emu: body_size * font_scale * 12700.0,
        size_pt: body_size * font_scale,
        bold: text_body.default_bold.unwrap_or(false),
        italic: text_body.default_italic.unwrap_or(false),
        color: default_color,
        underline: false,
        strikethrough: false,
        baseline: None,
    });

    // `a:bodyPr@vert` dispatch (renderer.ts:5021-5035). Stacked WordArt is a
    // separate draw path in the reference; the rotated modes only swap the
    // layout box and post-multiply a frame rotation.
    let mode = VertMode::of(&text_body.vert);
    if let VertMode::Stacked { rtl } = mode {
        return paint::compile_stacked(
            text_body,
            box_w,
            box_h,
            base_transform,
            &mut measurer,
            default_style,
            default_color,
            rtl,
            warning_sink,
        );
    }
    let (layout_w, layout_h, frame) = vertical::layout_frame(mode, box_w, box_h);
    let transform = frame.map_or(*base_transform, |frame| base_transform.post_concat(&frame));
    // `eaVert` per-glyph uprighting (`renderer.ts:5083` `eaVertUpright`); the
    // upright cells are painted with the element transform instead of the frame.
    let ea = vertical::ea_vert_frame(mode, box_w, box_h, base_transform);
    let avail_w = (layout_w - l_ins - r_ins).max(0.0);

    let paragraphs = build_paragraphs(
        text_body,
        default_style,
        font_scale,
        default_color,
        &mut measurer,
        warning_sink,
    );

    let do_wrap = text_body.wrap != "none";
    let mut lines: Vec<LineLayout> = Vec::new();

    for (p_index, para) in paragraphs.iter().enumerate() {
        let budget = (avail_w - para.mar_l - para.mar_r).max(0.0);
        let opts = BreakOptions {
            max_width: if do_wrap { budget } else { f32::INFINITY },
            first_line_indent: para.indent,
            tab_stops: &para.tab_stops,
            default_tab_size: para.tab_size,
            east_asian_line_break: para.ea_ln_brk,
            measurer: &measurer,
        };
        let broken = break_paragraph(&para.items, &opts);
        let line_count = broken.len().max(1);
        // A paragraph's equations are consumed by its `BreakItem::Object`s in
        // line order, the same order the paint pass walks them in.
        let mut math_cursor = 0usize;
        for (line_index, line) in broken.iter().enumerate() {
            let first_line = line_index == 0;
            let is_last = line_index + 1 == line_count;
            let pieces = line_math_pieces(line, para, &mut math_cursor);
            let (nat_ascent, nat_descent) = paint::natural_box(&measurer, line, para, &pieces);
            let (ascent, descent) = metrics::spaced_line_box(
                nat_ascent,
                nat_descent,
                para.space_line,
                12700.0,
                reduction,
            );
            let natural = nat_ascent + nat_descent;
            let mut top_gap = 0.0;
            if first_line && (text_body.spc_first_last_para || p_index > 0) {
                top_gap = metrics::paragraph_spacing(para.space_before, natural, 12700.0);
            }
            let mut after_gap = 0.0;
            if is_last && (text_body.spc_first_last_para || p_index + 1 < paragraphs.len()) {
                after_gap = metrics::paragraph_spacing(para.space_after, natural, 12700.0);
            }
            lines.push(LineLayout {
                para_index: p_index,
                items: line.items.clone(),
                width: line.width,
                ascent,
                descent,
                top_gap,
                after_gap,
                indent: para.indent,
                first_line,
                is_last_line: is_last,
                ends_with_break: line.ends_with_break,
            });
        }
    }

    let total_h: f32 = lines
        .iter()
        .map(|l| l.top_gap + l.ascent + l.descent + l.after_gap)
        .sum();
    let mut cur_y = metrics::block_top(
        &text_body.vertical_anchor,
        0.0,
        layout_h,
        t_ins,
        b_ins,
        total_h,
    );

    let mut final_runs: Vec<PositionedGlyphRun> = Vec::new();
    let mut commands: Vec<VectorCommand> = Vec::new();

    for line in &lines {
        cur_y += line.top_gap;
        let para = &paragraphs[line.para_index];
        let indent = if line.first_line { line.indent } else { 0.0 };
        let region_left = l_ins + para.mar_l;
        let region_w = (avail_w - para.mar_l - para.mar_r - indent).max(0.0);
        let line_x =
            metrics::align_x(&para.alignment, region_left, region_w, line.width, para.rtl) + indent;
        let baseline_y = cur_y + line.ascent;
        let justified = paint::justify_plan(para, line, region_w);

        paint::place_line(
            &measurer,
            para,
            line,
            line_x,
            baseline_y,
            &transform,
            ea.as_ref(),
            justified.as_ref(),
            &mut final_runs,
            &mut commands,
        );
        cur_y += line.ascent + line.descent + line.after_gap;
    }

    if !final_runs.is_empty() {
        commands.push(VectorCommand::DrawText {
            runs: final_runs,
            transform,
        });
    }
    commands
}

pub(super) fn build_paragraphs(
    text_body: &pptx_model::TextBody,
    default_style: StyleId,
    font_scale: f32,
    default_color: [u8; 4],
    measurer: &mut Measurer,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<ParagraphLayout> {
    let mut out: Vec<ParagraphLayout> = Vec::new();
    for (p_index, p) in text_body.paragraphs.iter().enumerate() {
        let def_size = p
            .def_font_size
            .or(text_body.default_font_size)
            .unwrap_or(18.0) as f32;
        let def_color = p
            .def_color
            .as_deref()
            .map(parse_hex_color)
            .unwrap_or(default_color);
        let def_family = p
            .def_font_family
            .clone()
            .unwrap_or_else(|| "Calibri".into());
        // `a:pPr@defTabSz` else PowerPoint's universal 1-inch grid
        // (renderer.ts:5702-5706); 0 disables the default grid.
        let tab_size = p.def_tab_sz.map(|v| v as f32).unwrap_or(914_400.0);
        // `a:tabLst` stops are absolute from the text-inset edge
        // (renderer.ts:5697-5700), so they stay in EMU.
        let tab_stops: Vec<TabStop> = p
            .tab_stops
            .iter()
            .map(|stop| TabStop {
                pos: stop.pos as f32,
                align: tab_align(&stop.algn),
            })
            .collect();
        let mut items: Vec<BreakItem> = Vec::new();
        let mut math_pieces: Vec<MathPiece> = Vec::new();

        if let Some(label) = bullet_label(p, p_index) {
            let b_size = def_size * font_scale;
            let id = measurer.push(StyleDef {
                family: "Carlito".to_string(),
                size_emu: b_size * 12700.0,
                size_pt: b_size,
                bold: false,
                italic: false,
                color: def_color,
                underline: false,
                strikethrough: false,
                baseline: None,
            });
            items.push(BreakItem::Text {
                text: label,
                style: id,
            });
        }

        for run in &p.runs {
            match run {
                pptx_model::TextRun::Text(data) => {
                    let family = resolve_font_family(
                        data.font_family
                            .as_deref()
                            .or(p.def_font_family.as_deref())
                            .unwrap_or(&def_family),
                        warning_sink,
                    );
                    let sz_pt = data.font_size.map(|s| s as f32).unwrap_or(def_size) * font_scale;
                    let color = data
                        .color
                        .as_deref()
                        .map(parse_hex_color)
                        .unwrap_or(def_color);
                    let bold = data
                        .bold
                        .or(p.def_bold)
                        .or(text_body.default_bold)
                        .unwrap_or(false);
                    let italic = data
                        .italic
                        .or(p.def_italic)
                        .or(text_body.default_italic)
                        .unwrap_or(false);
                    let id = measurer.push(StyleDef {
                        family,
                        size_emu: sz_pt * 12700.0,
                        size_pt: sz_pt,
                        bold,
                        italic,
                        color,
                        underline: data.underline,
                        strikethrough: data.strikethrough,
                        // Run-only: neither `TextBody` nor `Paragraph` carries a
                        // baseline default, and the parser already cascaded
                        // `lstStyle`/master `defRPr@baseline` into the run and
                        // dropped an explicit `0` (`pptx-parser/src/text.rs:2557`).
                        baseline: data.baseline,
                    });
                    push_text_items(&mut items, &data.text, id);
                }
                pptx_model::TextRun::Break { .. } => items.push(BreakItem::LineBreak),
                pptx_model::TextRun::Math {
                    nodes,
                    display,
                    font_size,
                    color,
                    fallback_text,
                } => {
                    let sz_pt = font_size.map(|s| s as f32).unwrap_or(def_size) * font_scale;
                    let params = MathParams {
                        display: *display,
                        font_size_pt: sz_pt,
                        color: parse_hex_color(color.as_deref().unwrap_or("000000")),
                        fallback: fallback_text.as_deref(),
                    };
                    let piece = math::layout_math(nodes, params, warning_sink);
                    if piece.is_empty() {
                        continue;
                    }
                    let width = piece.width.max(1.0);
                    let id = measurer.push(StyleDef {
                        family: "Cambria Math".to_string(),
                        size_emu: sz_pt * 12700.0,
                        size_pt: sz_pt,
                        bold: false,
                        italic: false,
                        color: default_color,
                        underline: false,
                        strikethrough: false,
                        baseline: None,
                    });
                    if *display {
                        // Display equations are their own centred block
                        // (renderer.ts:5087 `m:oMathPara`).
                        if !items.is_empty() {
                            out.push(ParagraphLayout {
                                items: std::mem::take(&mut items),
                                math: std::mem::take(&mut math_pieces),
                                alignment: p.alignment.clone(),
                                mar_l: p.mar_l as f32,
                                mar_r: p.mar_r as f32,
                                indent: p.indent as f32,
                                rtl: p.rtl,
                                tab_stops: tab_stops.clone(),
                                tab_size,
                                ea_ln_brk: p.ea_ln_brk,
                                space_line: space_of(&p.space_line),
                                space_before: pts_of_hundredths(p.space_before)
                                    .or(pct_of_thousandths(p.space_before_pct)),
                                space_after: pts_of_hundredths(p.space_after)
                                    .or(pct_of_thousandths(p.space_after_pct)),
                                default_style,
                                object_cursor: RefCell::new(0),
                            });
                        }
                        items.push(BreakItem::Object { width, style: id });
                        math_pieces.push(piece);
                        out.push(ParagraphLayout {
                            items: std::mem::take(&mut items),
                            math: std::mem::take(&mut math_pieces),
                            alignment: "ctr".to_string(),
                            mar_l: 0.0,
                            mar_r: 0.0,
                            indent: 0.0,
                            rtl: false,
                            tab_stops: Vec::new(),
                            tab_size,
                            ea_ln_brk: p.ea_ln_brk,
                            space_line: None,
                            space_before: None,
                            space_after: None,
                            default_style,
                            object_cursor: RefCell::new(0),
                        });
                        continue;
                    }
                    items.push(BreakItem::Object { width, style: id });
                    math_pieces.push(piece);
                }
            }
        }

        out.push(ParagraphLayout {
            items,
            math: math_pieces,
            alignment: p.alignment.clone(),
            mar_l: p.mar_l as f32,
            mar_r: p.mar_r as f32,
            indent: p.indent as f32,
            rtl: p.rtl,
            tab_stops,
            tab_size,
            ea_ln_brk: p.ea_ln_brk,
            space_line: space_of(&p.space_line),
            space_before: pts_of_hundredths(p.space_before)
                .or(pct_of_thousandths(p.space_before_pct)),
            space_after: pts_of_hundredths(p.space_after).or(pct_of_thousandths(p.space_after_pct)),
            default_style,
            object_cursor: RefCell::new(0),
        });
    }
    out
}

/// The math pieces backing this line's `BreakItem::Object`s, in item order.
fn line_math_pieces<'a>(
    line: &BrokenLine,
    para: &'a ParagraphLayout,
    cursor: &mut usize,
) -> Vec<&'a MathPiece> {
    let mut pieces: Vec<&MathPiece> = Vec::new();
    for item in &line.items {
        if matches!(item, LineItem::Object { .. }) {
            if let Some(piece) = para.math.get(*cursor) {
                pieces.push(piece);
            }
            *cursor += 1;
        }
    }
    pieces
}

/// `ST_TabAlign` (`l`/`ctr`/`r`/`dec`). An unknown value falls back to `l`,
/// the reference's `algn ?? 'l'` default (`tab.ts:43-47`).
fn tab_align(algn: &str) -> TabAlign {
    match algn {
        "ctr" => TabAlign::Center,
        "r" => TabAlign::Right,
        "dec" => TabAlign::Decimal,
        _ => TabAlign::Left,
    }
}

fn push_text_items(items: &mut Vec<BreakItem>, text: &str, style: StyleId) {
    for (i, part) in text.split('\t').enumerate() {
        if i > 0 {
            items.push(BreakItem::Tab);
        }
        if !part.is_empty() {
            items.push(BreakItem::Text {
                text: part.to_string(),
                style,
            });
        }
    }
}

fn bullet_label(p: &pptx_model::Paragraph, index: usize) -> Option<String> {
    match &p.bullet {
        pptx_model::Bullet::Char {
            ch, font_family, ..
        } => Some(format!("{} ", map_bullet_char(ch, font_family.as_deref()))),
        pptx_model::Bullet::AutoNum { start_at, .. } => {
            Some(format!("{}. ", start_at.unwrap_or(1) + index as u32))
        }
        _ => None,
    }
}
