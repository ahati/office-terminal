use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crossbeam_channel::Sender;

use crate::compiler::parse_hex_color;
use crate::contracts::{AffineTransform, PositionedGlyphRun, VectorCommand, WarningEvent};
use crate::text::bidi;
use crate::text::bullets::MarkerBitmap;
use crate::text::linebreak::LineItem;
use crate::text::measure::{StyleId, TextMeasure, WrapWords};
use crate::text::metrics;
use crate::text::shape;
use crate::text::vertical::{self, VertMode};

use super::body_layout;
use super::columns;
use super::paint;
use super::preflight;
use super::paragraphs::{self, ParagraphLayout};
use super::shrink;

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
    pub(super) underline_style: Option<String>,
    pub(super) underline_color: Option<[u8; 4]>,
    pub(super) strikethrough: bool,
    pub(super) strike_double: bool,
    /// `rPr@baseline` in thousandths of a point: positive = superscript
    /// (raised), negative = subscript (lowered). `0`/`None` = on the baseline.
    pub(super) baseline: Option<i32>,
    /// `rPr@spc` (ECMA-376 §21.1.2.3.9, `ST_TextPoint` §20.1.10.74) as EMU per
    /// inter-code-point boundary. `0` = normal tracking
    /// (`renderer.ts:1885`, `1899`).
    pub(super) letter_spacing_emu: f32,
    /// `rPr/a:highlight` (ECMA-376 §21.1.2.3.4) marker colour, resolved.
    pub(super) highlight: Option<[u8; 4]>,
    /// `rPr@lang` / authored East Asian language, used as kinsoku seam evidence.
    pub(super) lang: Option<String>,
    /// Authored run-level reflection effect (`a:rPr/a:effectLst/a:reflection`).
    pub(super) reflection: Option<crate::contracts::VectorReflection>,
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

// `Measurer::push` interns styles through a hash map, so `StyleDef` keys need
// `Eq` + `Hash` consistent with the derived structural `PartialEq`. Float
// fields are canonicalised before hashing so `==`-equal values always hash
// equal (`-0.0 == 0.0`); a NaN field never compares equal and simply interns
// as a fresh style, exactly like the linear scan this replaces.
impl Eq for StyleDef {}

impl Hash for StyleDef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.family.hash(state);
        hash_f32(self.size_emu, state);
        hash_f32(self.size_pt, state);
        self.bold.hash(state);
        self.italic.hash(state);
        self.color.hash(state);
        self.underline.hash(state);
        self.underline_style.hash(state);
        self.underline_color.hash(state);
        self.strikethrough.hash(state);
        self.strike_double.hash(state);
        self.baseline.hash(state);
        hash_f32(self.letter_spacing_emu, state);
        self.highlight.hash(state);
        self.lang.hash(state);
        match &self.reflection {
            Some(reflection) => {
                true.hash(state);
                reflection.blur_radius_emu.hash(state);
                hash_f32(reflection.start_opacity, state);
                hash_f32(reflection.start_pos, state);
                hash_f32(reflection.end_opacity, state);
                hash_f32(reflection.end_pos, state);
                reflection.distance_emu.hash(state);
                hash_f64(reflection.direction_deg, state);
                hash_f32(reflection.scale_x, state);
                hash_f32(reflection.scale_y, state);
                reflection.align.hash(state);
            }
            None => false.hash(state),
        }
    }
}

/// Hash-consistent with `f32::eq`: collapse `-0.0` onto `+0.0` (they compare
/// equal) and give the never-equal NaN a digest derived from its bits.
fn hash_f32<H: Hasher>(value: f32, state: &mut H) {
    let canonical = if value == 0.0 { 0.0f32 } else { value };
    canonical.to_bits().hash(state);
}

fn hash_f64<H: Hasher>(value: f64, state: &mut H) {
    let canonical = if value == 0.0 { 0.0f64 } else { value };
    canonical.to_bits().hash(state);
}

/// Measurement adapter: `advance()` shapes with the same face the painter uses.
pub(super) struct Measurer {
    styles: Vec<StyleDef>,
    /// Interned `StyleDef` -> id: the wrap phase re-pushes the same authored
    /// run styles for every probe, so the lookup is a hash probe instead of a
    /// linear structural scan over every previously interned style.
    index: HashMap<StyleDef, usize>,
    /// `style id -> measured width`. The inner key is interned `Arc<str>` so a
    /// probe hit (`get(text)`, `Borrow<str>`) never allocates; only a
    /// genuinely new prefix text pays one `Arc` for its cached width.
    cache: RefCell<HashMap<usize, HashMap<Arc<str>, f32>>>,
}

impl Measurer {
    pub(super) fn new() -> Self {
        Self {
            styles: Vec::new(),
            index: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Intern `style`: equal definitions reuse one `StyleId`, which is what
    /// lets the breaker coalesce equal-style runs (`renderer.ts:2000-2019`).
    pub(super) fn push(&mut self, style: StyleDef) -> StyleId {
        if let Some(&index) = self.index.get(&style) {
            return StyleId(index);
        }
        let index = self.styles.len();
        self.index.insert(style.clone(), index);
        self.styles.push(style);
        StyleId(index)
    }

    pub(super) fn style(&self, id: StyleId) -> &StyleDef {
        &self.styles[id.0]
    }
}

impl TextMeasure for Measurer {
    fn advance(&self, text: &str, style: StyleId) -> f32 {
        if let Some(width) = self
            .cache
            .borrow()
            .get(&style.0)
            .and_then(|widths| widths.get(text))
            .copied()
        {
            return width;
        }
        let def = self.style(style);
        // The breaker measures with the face the painter draws (`seg.font` is
        // built from `drawSizePx`, `renderer.ts:1859-1877`), including the
        // run's `rPr@spc` tracking (`measureTextAdvance`, `renderer.ts:1672`).
        let width = shape::measure(text, &def.family, def.draw_size_emu(), def.bold, def.italic)
            + def.spacing_advance(text);
        self.cache
            .borrow_mut()
            .entry(style.0)
            .or_default()
            .insert(Arc::from(text), width);
        width
    }

    fn space(&self, style: StyleId) -> f32 {
        self.advance(" ", style)
    }

    fn wrap_words(&self, text: &str, style: StyleId) -> Option<WrapWords> {
        let def = self.style(style);
        let shaped =
            shape::shape_words(text, &def.family, def.draw_size_emu(), def.bold, def.italic)?;
        Some(WrapWords::from_shaped(shaped, def.letter_spacing_emu))
    }
}

pub(super) struct LineLayout {
    pub(super) para_index: usize,
    pub(super) items: Vec<LineItem>,
    pub(super) width: f32,
    pub(super) ascent: f32,
    pub(super) descent: f32,
    pub(super) top_gap: f32,
    pub(super) after_gap: f32,
    /// First-line indent the layout applied (`textXOffset`,
    /// `renderer.ts:5479`): 0 for a paragraph that carries a marker.
    pub(super) first_line_indent: f32,
    pub(super) first_line: bool,
    /// Last line of its paragraph (`isLastLine` in the draw loop).
    pub(super) is_last_line: bool,
    /// Closed by an authored `<a:br/>` (`line.endsWithBreak`).
    pub(super) ends_with_break: bool,
}

/// The body-level default run style every paragraph inherits when a run does
/// not override it (`defaultTextStyle`, `renderer.ts:4985-5001`). The stored
/// `normAutofit@fontScale` — or the derived shrink scale — is already folded
/// into `font_scale`, so the pushed sizes are the final ones.
pub(super) fn push_default_style(
    measurer: &mut Measurer,
    text_body: &pptx_model::TextBody,
    font_scale: f32,
    default_color: [u8; 4],
) -> StyleId {
    let body_size = text_body.default_font_size.unwrap_or(18.0) as f32;
    measurer.push(StyleDef {
        family: "Calibri".to_string(),
        size_emu: body_size * font_scale * 12700.0,
        size_pt: body_size * font_scale,
        bold: text_body.default_bold.unwrap_or(false),
        italic: text_body.default_italic.unwrap_or(false),
        color: default_color,
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
    })
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
    let mut font_scale = text_body.font_scale.unwrap_or(1.0) as f32;
    let reduction = text_body.ln_spc_reduction.unwrap_or(0.0) as f32;
    let default_color = parse_hex_color(default_shape_text_color.unwrap_or("000000"));

    // `a:bodyPr@vert` dispatch (renderer.ts:5021-5035). Stacked WordArt is a
    // separate draw path in the reference; the rotated modes only swap the
    // layout box and post-multiply a frame rotation.
    let mode = VertMode::of(&text_body.vert);
    if let VertMode::Stacked { rtl } = mode {
        let mut measurer = Measurer::new();
        let default_style =
            push_default_style(&mut measurer, text_body, font_scale, default_color);
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

    // ODF `style:shrink-to-fit` (model `shrink_on_overflow`): derive the
    // largest half-percent font scale whose wrapped layout fits the frame,
    // LibreOffice-style. DrawingML never sets the flag — a stored
    // `normAutofit@fontScale` (`Some`) already fixed the size, and an
    // `spAutoFit` body grows instead of shrinking.
    if text_body.shrink_on_overflow
        && text_body.font_scale.is_none()
        && text_body.auto_fit != "sp"
    {
        font_scale = shrink::derive_shrink_scale(
            text_body,
            default_color,
            avail_w,
            t_ins,
            b_ins,
            layout_h,
            reduction,
            warning_sink,
        );
    }

    let mut measurer = Measurer::new();
    let default_style = push_default_style(&mut measurer, text_body, font_scale, default_color);
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
    let broken = body_layout::break_body_lines(
        text_body,
        &paragraphs,
        &measurer,
        avail_w,
        do_wrap,
        reduction,
        t_ins,
        b_ins,
        layout_h,
        num_col,
    );
    let mut lines = broken.lines;
    let plan = broken.plan;
    let anchor_h = broken.anchor_h;
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
