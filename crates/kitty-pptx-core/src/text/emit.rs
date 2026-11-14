use std::cell::RefCell;
use std::collections::HashMap;

use crossbeam_channel::Sender;

use crate::compiler::parse_hex_color;
use crate::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand, WarningEvent};
use crate::text::bidi;
use crate::text::bullets::MarkerBitmap;
use crate::text::linebreak::{break_paragraph, BreakOptions, LineItem};
use crate::text::measure::{StyleId, TextMeasure};
use crate::text::metrics;
use crate::text::shape;
use crate::text::vertical::{self, VertMode};

use super::columns;
use super::natural_box;
use super::paint;
use super::preflight;
use super::paragraphs::{self, ParagraphLayout};

/// One authored text segment's resolved styling.
///
/// `Measurer::push` interns equal styles, so two adjacent runs with identical
/// metadata share one `StyleId` and the breaker coalesces them — the port of the
/// reference's structural `sameStyle` comparison (`renderer.ts:2000-2019`, which
/// includes `highlight` in the identity so two markers never bleed).
#[derive(Clone, Debug, PartialEq)]
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
    /// `rPr@spc` (ECMA-376 §21.1.2.3.9, `ST_TextPoint` §20.1.10.74) as EMU per
    /// inter-code-point boundary. `0` = normal tracking
    /// (`renderer.ts:1885`, `1899`).
    pub(super) letter_spacing_emu: f32,
    /// `rPr/a:highlight` (ECMA-376 §21.1.2.3.4) marker colour, resolved.
    pub(super) highlight: Option<[u8; 4]>,
}

impl StyleDef {
    /// `letterSpacingPx * (codePointCount - 1)` (`measureTextAdvance`,
    /// `renderer.ts:1690-1691`): tracking only separates code points.
    pub(super) fn spacing_advance(&self, text: &str) -> f32 {
        if self.letter_spacing_emu == 0.0 {
            return 0.0;
        }
        let boundaries = text.chars().count().saturating_sub(1) as f32;
        self.letter_spacing_emu * boundaries
    }
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

    /// Intern `style`: equal definitions reuse one `StyleId`, which is what
    /// lets the breaker coalesce equal-style runs (`renderer.ts:2000-2019`).
    pub(super) fn push(&mut self, style: StyleDef) -> StyleId {
        if let Some(index) = self.styles.iter().position(|known| *known == style) {
            return StyleId(index);
        }
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
        // built from `drawSizePx`, `renderer.ts:1859-1877`), including the
        // run's `rPr@spc` tracking (`measureTextAdvance`, `renderer.ts:1672`).
        let width = shape::measure(text, &def.family, def.draw_size_emu(), def.bold, def.italic)
            + def.spacing_advance(text);
        self.cache.borrow_mut().insert(key, width);
        width
    }

    fn space(&self, style: StyleId) -> f32 {
        self.advance(" ", style)
    }
}

pub(super) struct LineLayout {
    para_index: usize,
    pub(super) items: Vec<LineItem>,
    pub(super) width: f32,
    ascent: f32,
    descent: f32,
    top_gap: f32,
    after_gap: f32,
    /// First-line indent the layout applied (`textXOffset`,
    /// `renderer.ts:5479`): 0 for a paragraph that carries a marker.
    first_line_indent: f32,
    pub(super) first_line: bool,
    /// Last line of its paragraph (`isLastLine` in the draw loop).
    pub(super) is_last_line: bool,
    /// Closed by an authored `<a:br/>` (`line.endsWithBreak`).
    pub(super) ends_with_break: bool,
}

/// `renderTextBody` for bodies without an `a:prstTxWarp`.
///
/// No bitmap probe: a picture marker whose bitmap has not been decoded paints
/// nothing, exactly the reference's `peekCachedBitmap` miss
/// (`renderer.ts:5812`). See [`compile_text_body_with_markers`].
#[allow(clippy::too_many_arguments)]
pub fn compile_text_body(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    default_shape_text_color: Option<&str>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    compile_text_body_with_markers(
        text_body,
        box_w,
        box_h,
        base_transform,
        default_shape_text_color,
        None,
        warning_sink,
    )
}

/// [`compile_text_body`] with a bitmap probe for `<a:buBlip>` picture markers.
///
/// `bitmaps` is the decoded-image seam the reference reads through
/// `peekCachedBitmap` when it paints a picture marker (`renderer.ts:5862-5877`).
/// Without it (or for a path the probe does not know) no marker image is
/// placed — the marker appears once its bitmap is available, never blocking.
#[allow(clippy::too_many_arguments)]
pub fn compile_text_body_with_markers(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    default_shape_text_color: Option<&str>,
    bitmaps: Option<&dyn MarkerBitmap>,
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
        letter_spacing_emu: 0.0,
        highlight: None,
    });

    // `a:bodyPr@vert` dispatch (renderer.ts:5021-5035). Stacked WordArt is a
    // separate draw path in the reference; the rotated modes only swap the
    // layout box and post-multiply a frame rotation.
    let mode = VertMode::of(&text_body.vert);
    if let VertMode::Stacked { rtl } = mode {
        return super::stacked::compile_stacked(
            text_body,
            box_w,
            box_h,
            base_transform,
            &mut measurer,
            default_style,
            default_color,
            rtl,
            bitmaps,
            warning_sink,
        );
    }
    let (layout_w, layout_h, frame) = vertical::layout_frame(mode, box_w, box_h);
    let transform = frame.map_or(*base_transform, |frame| base_transform.post_concat(&frame));
    // `eaVert` per-glyph uprighting (`renderer.ts:5083` `eaVertUpright`); the
    // upright cells are painted with the element transform instead of the frame.
    let ea = vertical::ea_vert_frame(mode, box_w, box_h, base_transform);
    let content_w = (layout_w - l_ins - r_ins).max(0.0);
    let num_col = text_body.num_col.max(1) as usize;
    let column_gap = text_body.spc_col.max(0) as f32;
    let column_width = if num_col > 1 {
        (content_w - (num_col - 1) as f32 * column_gap).max(0.0) / num_col as f32
    } else {
        content_w
    };
    let avail_w = column_width;

    let paragraphs = paragraphs::build_paragraphs(
        text_body,
        default_style,
        font_scale,
        default_color,
        &mut measurer,
        warning_sink,
    );

    // `spAutoFit`: a shape grows instead of wrapping when every paragraph's
    // natural width fits the box. If one overflows, the ordinary wrap applies.
    let sp_grows = text_body.auto_fit == "sp"
        && !preflight::natural_width_exceeds_bbox(
            &paragraphs,
            layout_w,
            l_ins,
            r_ins,
            &measurer,
        );
    let do_wrap = text_body.wrap != "none" && !sp_grows;
    let mut lines: Vec<LineLayout> = Vec::new();
    for (p_index, para) in paragraphs.iter().enumerate() {
        let budget = (avail_w - para.mar_l - para.mar_r).max(0.0);
        // `firstLineIndentPxFor(hasBullet, indentPx)` (`renderer.ts:1542-1544`):
        // a marker already owns the gutter, so it contributes no text indent.
        let first_line_indent = if para.marker.is_some() {
            0.0
        } else {
            para.indent
        };
        let opts = BreakOptions {
            max_width: if do_wrap { budget } else { f32::INFINITY },
            first_line_indent,
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
            let pieces = paragraphs::line_math_pieces(line, para, &mut math_cursor);
            let (nat_ascent, nat_descent) = natural_box::natural_box(&measurer, line, para, &pieces);
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
                first_line_indent,
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
    // The final paragraph's trailing space-after cannot by itself fill a
    // column (ECMA-376 §21.1.2.1.1 bodyPr@numCol).
    let trailing_after = lines.last().map(|line| line.after_gap).unwrap_or(0.0);
    let column_capacity = (layout_h - t_ins - b_ins).max(0.0);
    let plan = columns::plan_columns(
        lines.len(),
        (total_h - trailing_after).max(0.0),
        column_capacity,
        num_col,
    );
    let mut column_heights = vec![0.0f32; plan.used_columns.max(1)];
    for (index, line) in lines.iter().enumerate() {
        let column = plan.line_col.get(index).copied().unwrap_or(0);
        if let Some(height) = column_heights.get_mut(column) {
            *height += line.top_gap + line.ascent + line.descent + line.after_gap;
        }
    }
    let anchor_h = column_heights
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    let mut cur_y = metrics::block_top(
        &text_body.vertical_anchor,
        0.0,
        layout_h,
        t_ins,
        b_ins,
        anchor_h,
    );
    let column_top = cur_y;

    let mut final_runs: Vec<PositionedGlyphRun> = Vec::new();
    let mut commands: Vec<VectorCommand> = Vec::new();

    let mut previous_column = 0usize;
    for (line_index, line) in lines.iter_mut().enumerate() {
        let column = plan.line_col.get(line_index).copied().unwrap_or(0);
        if column != previous_column {
            cur_y = column_top;
            previous_column = column;
        }
        let x_shift = columns::column_x(
            column,
            num_col,
            column_width,
            column_gap,
            text_body.rtl_col,
        );
        cur_y += line.top_gap;
        let para = &paragraphs[line.para_index];
        let indent = if line.first_line {
            line.first_line_indent
        } else {
            0.0
        };
        // `renderer.ts:5683-5722`: a line carrying inline tabs resolves them at
        // draw time in the reading frame, then measures the line with those
        // widths (`renderer.ts:5730-5740`) — the anchor below and the RTL pen
        // both use the resulting width.
        let has_tab = line
            .items
            .iter()
            .any(|item| matches!(item, LineItem::Tab { .. }));
        if has_tab {
            paint::resolve_line_tabs(
                &mut line.items,
                para,
                avail_w,
                indent,
                para.rtl,
                &measurer,
            );
            line.width = line.items.iter().map(paint::item_advance).sum();
        }
        // `renderer.ts:5689-5692`: engage UAX#9 only when the base direction is
        // RTL or the line carries a strong-RTL character.
        let segments = paint::bidi_segments(&line.items);
        let visual = (para.rtl || bidi::segments_have_rtl(&segments))
            .then(|| bidi::compute_line_visual_order(&segments, para.rtl));
        let region_left = l_ins + x_shift + para.mar_l;
        let region_w = (avail_w - para.mar_l - para.mar_r - indent).max(0.0);
        // `leadingEdgeX = textX + textMaxW` (`renderer.ts:5811`).
        let leading_edge = l_ins + x_shift + avail_w - para.mar_r;
        // Reading-frame marker placement under an RTL base (issue #930): the
        // marker's RIGHT edge sits on the leading edge and the text is laid
        // contiguously to its left, so the pen reserves the marker advance
        // (`rtlMarkerReservePx`, `renderer.ts:5815-5830`, `5860-5869`).
        let reserve = if line.first_line && para.rtl {
            paragraphs::marker_advance(&measurer, para, bitmaps)
        } else {
            0.0
        };
        let marker_x = if reserve > 0.0 {
            leading_edge - reserve
        } else {
            // `bulletX = bx + lPad + marLPx + indentPx` (`renderer.ts:5245`).
            region_left + para.indent
        };
        // `penX` (`renderer.ts:5880-5904`): a tabbed line is placed in the
        // READING frame — the RTL pen right-anchors at the leading edge, and a
        // tab stop is absolute from the leading text-inset edge, so paragraph
        // alignment must not add a second offset (#913/#916).
        let line_x = if reserve > 0.0 || has_tab {
            if para.rtl {
                leading_edge - reserve - line.width
            } else {
                region_left + indent
            }
        } else {
            metrics::align_x(&para.alignment, region_left, region_w, line.width, para.rtl) + indent
        };
        let baseline_y = cur_y + line.ascent;
        let justified = paint::justify_plan(para, line, region_w);

        paint::place_line(
            &measurer,
            para,
            line,
            line_x,
            marker_x,
            baseline_y,
            &transform,
            ea.as_ref(),
            justified.as_ref(),
            visual.as_ref(),
            bitmaps,
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

/// Whether `para` paints a marker on the given line: the first line only, and
/// only when that line carries content (`lineHasContent`,
/// `renderer.ts:5485-5488`).
pub(super) fn marker_visible(para: &ParagraphLayout, line: &LineLayout) -> bool {
    line.first_line
        && para.marker.is_some()
        && line
            .items
            .iter()
            .any(|item| !matches!(item, LineItem::Tab { .. }))
}
