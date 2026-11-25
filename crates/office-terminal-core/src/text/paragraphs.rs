//! Paragraph input resolution: `paragraphInputRuns` + the paragraph defaults
//! (`renderer.ts:1700-2020`), plus list-marker resolution (`text::bullets`).
//!
//! Split out of [`super::emit`] to keep both halves under the file limit: this
//! module turns `pptx_model` paragraphs into [`ParagraphLayout`] (break items,
//! tab stops, spacing, alignment, marker), and `emit` owns the line loop.

use std::cell::RefCell;
use std::collections::HashMap;

use crossbeam_channel::Sender;

use crate::compiler::math::{self, MathParams, MathPiece};
use crate::compiler::parse_hex_color;
use crate::compiler::text::resolve_font_family;
use crate::contracts::WarningEvent;
use crate::text::bullets::{self, AutoNumCounters, Marker, MarkerBitmap, MarkerContext};
use crate::text::measure::{StyleId, TabAlign, TabStop, TextMeasure};
use crate::text::metrics;

use super::emit::{Measurer, StyleDef};
use super::slots::{self, SlotFaces};

/// The paragraph's list marker, as the paint pass needs it: a glyph label
/// (`renderer.ts:5849`) or a picture marker (`renderer.ts:5873`).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MarkerPlan {
    Label { text: String, style: StyleId },
    Image { rel_id: String, size_emu: f32 },
}

pub(super) struct ParagraphLayout {
    pub(super) items: Vec<BreakItem>,
    /// Math pieces in `BreakItem::Object` order.
    pub(super) math: Vec<MathPiece>,
    pub(super) alignment: String,
    pub(super) mar_l: f32,
    pub(super) mar_r: f32,
    /// `a:pPr@indent` (raw, signed): the marker's gutter offset
    /// (`bulletX = textX + indentPx`, `renderer.ts:5245`).
    pub(super) indent: f32,
    pub(super) rtl: bool,
    /// `a:pPr/a:tabLst` stops, already in EMU from the text-inset edge.
    pub(super) tab_stops: Vec<TabStop>,
    /// `a:pPr@defTabSz` else PowerPoint's 1-inch grid (renderer.ts:5704).
    pub(super) tab_size: f32,
    /// `a:pPr@eaLnBrk` (ECMA-376 §21.1.2.2.7, default true).
    pub(super) ea_ln_brk: bool,
    pub(super) space_line: metrics::SpaceLine,
    pub(super) space_before: metrics::SpaceLine,
    pub(super) space_after: metrics::SpaceLine,
    pub(super) default_style: StyleId,
    pub(super) object_cursor: RefCell<usize>,
    /// The marker drawn once, in the first line's gutter (`renderer.ts:5494`).
    pub(super) marker: Option<MarkerPlan>,
}

use crate::text::linebreak::BreakItem;

/// `renderTextBody`'s per-paragraph resolution: paragraph defaults, tab stops,
/// runs, equations and the list marker, in authored order.
pub(super) fn build_paragraphs(
    text_body: &pptx_model::TextBody,
    default_style: StyleId,
    font_scale: f32,
    default_color: [u8; 4],
    measurer: &mut Measurer,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<ParagraphLayout> {
    let mut out: Vec<ParagraphLayout> = Vec::new();
    // One counter map per body: `autoNumCounters` (`renderer.ts:4876`).
    let mut counters = AutoNumCounters::new();
    for p in text_body.paragraphs.iter() {
        let def_size = p.def_font_size.or(text_body.default_font_size).unwrap_or(18.0) as f32;
        let def_color = p.def_color.as_deref().map(parse_hex_color).unwrap_or(default_color);
        let def_family = p.def_font_family.clone().unwrap_or_else(|| "Calibri".into());
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

        // `resolveBulletLabel` walks every paragraph: the empty-paragraph and
        // reset semantics are part of the counter map's behaviour.
        let label = counters.resolve_label(p);
        let marker = resolve_marker(
            p,
            label,
            text_body,
            def_family.as_str(),
            def_color,
            font_scale,
            measurer,
            warning_sink,
        );

        // Failed complete MCE alternatives: the authored preview replaces the
        // whole start..end run range (outermost range wins). In this offline
        // port the preview is preferred whenever it covers the branch, since
        // the live math engine cannot signal per-run conversion failure.
        let mut previews: HashMap<usize, (usize, String)> = HashMap::new();
        for fallback in &p.math_fallbacks {
            let (start, end) = (fallback.start, fallback.end);
            if start >= p.runs.len()
                || end > p.runs.len()
                || end <= start
                || !matches!(p.runs[start], pptx_model::TextRun::Math { .. })
                || fallback.text.is_empty()
            {
                continue;
            }
            let replace = previews
                .get(&start)
                .is_none_or(|(previous_end, _)| end >= *previous_end);
            if replace {
                previews.insert(start, (end, fallback.text.clone()));
            }
        }
        let mut preview_end = 0usize;

        // Transfer leading extenders across text run seams so a grapheme cluster
        // spanning run boundaries stays in the base run's font and style.
        let mut normalized_runs: Vec<pptx_model::TextRun> = p.runs.clone();
        let mut last_text_idx: Option<usize> = None;
        for i in 0..normalized_runs.len() {
            if let pptx_model::TextRun::Text(next) = &normalized_runs[i] {
                if let Some(prev_idx) = last_text_idx {
                    let prev_char = match &normalized_runs[prev_idx] {
                        pptx_model::TextRun::Text(t) => t.text.chars().last(),
                        _ => None,
                    };
                    if let Some(mut prev_char) = prev_char {
                        let mut take_bytes = 0usize;
                        for ch in next.text.chars() {
                            if !crate::text::linebreak::grapheme::gb_break(prev_char, ch, 0, false) {
                                take_bytes += ch.len_utf8();
                                prev_char = ch;
                            } else {
                                break;
                            }
                        }
                        if take_bytes > 0 {
                            let extenders = next.text[..take_bytes].to_string();
                            if let pptx_model::TextRun::Text(prev) = &mut normalized_runs[prev_idx] {
                                prev.text.push_str(&extenders);
                            }
                            if let pptx_model::TextRun::Text(next) = &mut normalized_runs[i] {
                                next.text.drain(..take_bytes);
                            }
                        }
                    }
                }
            }
            if let pptx_model::TextRun::Text(t) = &normalized_runs[i] {
                if !t.text.is_empty() {
                    last_text_idx = Some(i);
                }
            } else {
                last_text_idx = None;
            }
        }

        for (run_index, run) in normalized_runs.iter().enumerate() {
            if run_index < preview_end {
                continue;
            }
            if let Some((end, text)) = previews.get(&run_index) {
                preview_end = *end;
                if let pptx_model::TextRun::Math { font_size, color, .. } = &normalized_runs[run_index] {
                    let sz_pt = font_size.map(|s| s as f32).unwrap_or(def_size) * font_scale;
                    let id = measurer.push(StyleDef {
                        family: def_family.clone(),
                        size_emu: sz_pt * 12700.0,
                        size_pt: sz_pt,
                        bold: p.def_bold.or(text_body.default_bold).unwrap_or(false),
                        italic: p.def_italic.or(text_body.default_italic).unwrap_or(false),
                        color: color.as_deref().map(parse_hex_color).unwrap_or(def_color),
                        underline: false,
                        underline_style: None,
                        underline_color: None,
                        strikethrough: false,
                        strike_double: false,
                        baseline: None,
                        letter_spacing_emu: 0.0,
                        highlight: None,
                        lang: None,
                        reflection: None,
                    });
                    items.push(BreakItem::Text {
                        text: text.clone(),
                        style: id,
                    });
                }
                continue;
            }
            match run {
                pptx_model::TextRun::Text(data) => {
                    let authored = data
                        .font_family
                        .as_deref()
                        .or(p.def_font_family.as_deref())
                        .unwrap_or(&def_family);
                    let family = resolve_font_family(authored, warning_sink);
                    let sz_pt = data.font_size.map(|s| s as f32).unwrap_or(def_size) * font_scale;
                    let color = data.color.as_deref().map(parse_hex_color).unwrap_or(def_color);
                    let bold = data.bold.or(p.def_bold).or(text_body.default_bold).unwrap_or(false);
                    let italic = data
                        .italic
                        .or(p.def_italic)
                        .or(text_body.default_italic)
                        .unwrap_or(false);
                    let style = StyleDef {
                        family: family.clone(),
                        size_emu: sz_pt * 12700.0,
                        size_pt: sz_pt,
                        bold,
                        italic,
                        color,
                        underline: data.underline,
                        underline_style: data.underline_style.clone(),
                        underline_color: data.underline_color.as_deref().map(parse_hex_color),
                        strikethrough: data.strikethrough,
                        strike_double: data.strike_double,
                        // Run-only: neither `TextBody` nor `Paragraph` carries a
                        // baseline default, and the parser already cascaded
                        // `lstStyle`/master `defRPr@baseline` into the run and
                        // dropped an explicit `0` (`pptx-parser/src/text.rs:2557`).
                        baseline: data.baseline,
                        // `rPr@spc` is points → EMU like any run size
                        // (`renderer.ts:1885`).
                        letter_spacing_emu: data.letter_spacing.map(|spc| spc as f32 * 12700.0).unwrap_or(0.0),
                        // `rPr/a:highlight` (§21.1.2.3.4), resolved by the parser.
                        highlight: data.highlight.as_deref().map(parse_hex_color),
                        lang: data.lang.clone(),
                        reflection: data.reflection.as_ref().map(crate::compiler::shape_effects::reflection),
                    };
                    // Per-cluster `a:latin` / `a:ea` / `a:cs` slot dispatch
                    // (`renderer.ts:1930-1985`): one `BreakItem` per slot seam.
                    let faces = SlotFaces::resolve(
                        &family,
                        data.font_family_ea.as_deref(),
                        data.font_family_cs.as_deref(),
                        data.lang.as_deref(),
                        &data.text,
                        warning_sink,
                    );
                    slots::push_run_items(&mut items, &data.text, &style, &faces, measurer);
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
                    let run_color = color.as_deref().map(parse_hex_color).unwrap_or(default_color);
                    let id = measurer.push(StyleDef {
                        family: "Cambria Math".to_string(),
                        size_emu: sz_pt * 12700.0,
                        size_pt: sz_pt,
                        bold: false,
                        italic: false,
                        color: run_color,
                        underline: false,
                        underline_style: None,
                        underline_color: None,
                        strikethrough: false,
                        strike_double: false,
                        baseline: None,
                        letter_spacing_emu: 0.0,
                        highlight: None,
                        lang: None,
                        reflection: None,
                    });
                    let layout = |items: Vec<BreakItem>, math: Vec<MathPiece>, alignment: String| ParagraphLayout {
                        items,
                        math,
                        alignment,
                        mar_l: p.mar_l as f32,
                        mar_r: p.mar_r as f32,
                        indent: p.indent as f32,
                        rtl: p.rtl,
                        tab_stops: tab_stops.clone(),
                        tab_size,
                        ea_ln_brk: p.ea_ln_brk,
                        space_line: space_of(&p.space_line),
                        space_before: space_before(p),
                        space_after: space_after(p),
                        default_style,
                        object_cursor: RefCell::new(0),
                        marker: None,
                    };
                    let params = MathParams {
                        display: *display,
                        font_size_pt: sz_pt,
                        color: run_color,
                        fallback: fallback_text.as_deref(),
                    };
                    let piece = math::layout_math(nodes, params, warning_sink);
                    if piece.is_empty() {
                        if let Some(fallback) = fallback_text.as_deref().filter(|t| !t.is_empty()) {
                            items.push(BreakItem::Text {
                                text: fallback.to_string(),
                                style: id,
                            });
                        } else if !nodes.is_empty() {
                            items.push(BreakItem::Text {
                                text: "Equation unavailable".to_string(),
                                style: id,
                            });
                        } else if *display {
                            if !items.is_empty() {
                                let flushed = std::mem::take(&mut items);
                                let flushed_math = std::mem::take(&mut math_pieces);
                                out.push(layout(flushed, flushed_math, p.alignment.clone()));
                            }
                            let equation_items = vec![BreakItem::Object { width: 0.0, style: id }];
                            out.push(layout(equation_items, Vec::new(), "ctr".to_string()));
                        } else {
                            items.push(BreakItem::Object { width: 0.0, style: id });
                        }
                        continue;
                    }
                    let width = piece.width.max(1.0);
                    if *display {
                        // Display equations are their own centred block
                        // (renderer.ts:5087 `m:oMathPara`).
                        if !items.is_empty() {
                            let flushed = std::mem::take(&mut items);
                            let flushed_math = std::mem::take(&mut math_pieces);
                            out.push(layout(flushed, flushed_math, p.alignment.clone()));
                        }
                        items.push(BreakItem::Object { width, style: id });
                        math_pieces.push(piece);
                        let block = layout(
                            std::mem::take(&mut items),
                            std::mem::take(&mut math_pieces),
                            "ctr".to_string(),
                        );
                        out.push(ParagraphLayout {
                            mar_l: 0.0,
                            mar_r: 0.0,
                            indent: 0.0,
                            rtl: false,
                            tab_stops: Vec::new(),
                            space_line: None,
                            space_before: None,
                            space_after: None,
                            ..block
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
            space_before: space_before(p),
            space_after: space_after(p),
            default_style,
            object_cursor: RefCell::new(0),
            marker,
        });
    }
    out
}

/// Resolve the marker label/image/face/size/colour (`text::bullets`).
#[allow(clippy::too_many_arguments)]
fn resolve_marker(
    para: &pptx_model::Paragraph,
    label: Option<String>,
    text_body: &pptx_model::TextBody,
    def_family: &str,
    def_color: [u8; 4],
    font_scale: f32,
    measurer: &mut Measurer,
    warning_sink: &Sender<WarningEvent>,
) -> Option<MarkerPlan> {
    let context = MarkerContext {
        body_default_size_pt: text_body.default_font_size.unwrap_or(18.0) as f32,
        body_default_family: def_family,
        para_default_color: def_color,
        font_scale,
    };
    let Marker { label, image } = bullets::resolve_marker(para, label, &context, warning_sink);
    if let Some(label) = label {
        let id = measurer.push(StyleDef {
            family: label.family,
            size_emu: label.size_pt * 12700.0,
            size_pt: label.size_pt,
            bold: false,
            italic: false,
            color: label.color,
            underline: false,
            underline_style: None,
            underline_color: None,
            strikethrough: false,
            strike_double: false,
            baseline: None,
            letter_spacing_emu: 0.0,
            highlight: None,
            lang: None,
            reflection: None,
        });
        return Some(MarkerPlan::Label {
            text: label.text,
            style: id,
        });
    }
    image.map(|image| MarkerPlan::Image {
        rel_id: image.rel_id,
        size_emu: image.size_pt * 12700.0,
    })
}

/// The advance the marker reserves at the leading edge under an RTL base
/// (`rtlMarkerReservePx`, `renderer.ts:5817-5830`): a label's measured width, or
/// a picture marker's aspect-corrected width.
pub(super) fn marker_advance(measurer: &Measurer, para: &ParagraphLayout, bitmaps: Option<&dyn MarkerBitmap>) -> f32 {
    match &para.marker {
        None => 0.0,
        Some(MarkerPlan::Label { text, style }) => measurer.advance(text, *style),
        Some(MarkerPlan::Image { rel_id, size_emu }) => picture_width(rel_id, *size_emu, bitmaps).unwrap_or(0.0),
    }
}

/// The picture marker's painted width: height × the decoded bitmap's aspect
/// ratio (`renderer.ts:5866-5867`).
pub(super) fn picture_width(rel_id: &str, height_emu: f32, bitmaps: Option<&dyn MarkerBitmap>) -> Option<f32> {
    let (width_px, height_px) = bitmaps?.source_size(rel_id)?;
    if height_px == 0 {
        return Some(height_emu);
    }
    Some(height_emu * width_px as f32 / height_px as f32)
}

fn space_of(value: &Option<ooxml_common::text::SpaceLine>) -> metrics::SpaceLine {
    match value {
        Some(ooxml_common::text::SpaceLine::Pct { val }) => Some((true, *val as f32)),
        Some(ooxml_common::text::SpaceLine::Pts { val }) => Some((false, *val as f32)),
        None => None,
    }
}

fn space_before(p: &pptx_model::Paragraph) -> metrics::SpaceLine {
    p.space_before
        .map(|v| (false, v as f32 / 100.0))
        .or(p.space_before_pct.map(|v| (true, v as f32)))
}

fn space_after(p: &pptx_model::Paragraph) -> metrics::SpaceLine {
    p.space_after
        .map(|v| (false, v as f32 / 100.0))
        .or(p.space_after_pct.map(|v| (true, v as f32)))
}

/// The math pieces backing this line's `BreakItem::Object`s, in item order.
pub(super) fn line_math_pieces<'a>(
    line: &crate::text::linebreak::BrokenLine,
    para: &'a ParagraphLayout,
    cursor: &mut usize,
) -> Vec<&'a MathPiece> {
    let mut pieces: Vec<&MathPiece> = Vec::new();
    for item in &line.items {
        if matches!(item, crate::text::linebreak::LineItem::Object { .. }) {
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
