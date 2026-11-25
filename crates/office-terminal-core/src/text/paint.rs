//! Glyph placement for a laid-out text body, plus the stacked WordArt path.
//!
//! Split out of [`super::emit`] so the layout half (wrap / spacing / anchor /
//! baselines) and the paint half (per-line pen, justification gaps, run
//! emission) stay independently testable and under the 600-line file limit.
//!
//! Reference: `renderer.ts:5680-5925` (tab/justify draw loop), `4849-5028`
//! (stacked WordArt) and the kernels in `packages/core/src/drawingml-text/`.

use std::cell::RefCell;
use std::collections::HashMap;

use super::decor;
use super::emit::{marker_visible, LineLayout, Measurer, StyleDef};
use super::paragraphs::ParagraphLayout;
use crate::compiler::math;
use crate::contracts::{AffineTransform, PositionedGlyph, PositionedGlyphRun, VectorCommand};
use crate::text::bidi::{BidiSegment, LineVisualOrder};
use crate::text::bullets::MarkerBitmap;
use crate::text::justify::{self, JustifyMode, JustifyPlan};
use crate::text::linebreak::LineItem;
use crate::text::measure::{StyleId, TextMeasure};
use crate::text::metrics;
use crate::text::shape;
use crate::text::vertical::{self, EaVertFrame};

/// `baselineDrawSizePx` (`renderer.ts:1549-1553`): PowerPoint paints a
/// non-zero DrawingML `rPr@baseline` run at 65 % of its authored size. The
/// authored size still owns the line height and the baseline offset.
const BASELINE_DRAW_SCALE: f32 = 0.65;

impl StyleDef {
    /// A non-zero `rPr@baseline` enters the shrunk-glyph path.
    fn shifted(&self) -> bool {
        matches!(self.baseline, Some(value) if value != 0)
    }

    /// Glyph size the measurer and the painter shape with — the authored size
    /// unless the run carries a non-zero baseline (`renderer.ts:1832`).
    pub(super) fn draw_size_emu(&self) -> f32 {
        if self.shifted() {
            self.size_emu * BASELINE_DRAW_SCALE
        } else {
            self.size_emu
        }
    }

    /// `fontSizePx` reported to the text layer: the draw size
    /// (`renderer.ts:6233` emits `fontSize: drawSizePx`).
    pub(super) fn draw_size_pt(&self) -> f32 {
        if self.shifted() {
            self.size_pt * BASELINE_DRAW_SCALE
        } else {
            self.size_pt
        }
    }

    /// `baselineShift = -(baseline / 100000) * sizePx` (`renderer.ts:5981`):
    /// a positive `baseline` moves the glyph baseline UP (the canvas y axis
    /// points down), and the authored size, not the shrunk draw size, gives the
    /// offset.
    pub(super) fn baseline_shift_emu(&self) -> f32 {
        match self.baseline {
            Some(value) if value != 0 => -(value as f32 / 100_000.0) * self.size_emu,
            _ => 0.0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn place_line(
    measurer: &Measurer,
    para: &ParagraphLayout,
    line: &LineLayout,
    line_x: f32,
    marker_x: f32,
    baseline_y: f32,
    transform: &AffineTransform,
    ea: Option<&EaVertFrame>,
    justify: Option<&JustifyPlan>,
    visual: Option<&LineVisualOrder>,
    bitmaps: Option<&dyn MarkerBitmap>,
    final_runs: &mut Vec<PositionedGlyphRun>,
    commands: &mut Vec<VectorCommand>,
) {
    if marker_visible(para, line) {
        decor::paint_marker(
            measurer, para, marker_x, baseline_y, transform, bitmaps, final_runs, commands,
        );
    }
    let mut pen_x = 0.0f32;
    // Position in the line's flat unit stream, see `justify::JustifyPlan`.
    let mut unit = 0usize;
    // `eaVert` upright cells, painted outside the +90° frame (`renderer.ts:5034`).
    let mut upper = ea.map(|frame| Upper {
        frame,
        runs: Vec::new(),
    });
    for step in 0..line.items.len() {
        // `renderer.ts:5936-5940`: draw the segments in UAX#9 L2 visual order,
        // each at the running pen (textAlign is already left, so `pen_x` stays
        // the segment's left edge). Without bidi the order is logical.
        let index = match visual {
            Some(visual) => visual.order.get(step).copied().unwrap_or(step),
            None => step,
        };
        let item = &line.items[index];
        match item {
            LineItem::Text { text, style, width } => {
                let extra = place_text(
                    measurer,
                    text,
                    *style,
                    line_x + pen_x,
                    baseline_y,
                    justify.map(|plan| (plan, unit)),
                    upper.as_mut(),
                    final_runs,
                );
                // Run-level highlight (§21.1.2.3.4): the box covers the glyph
                // advance plus the justification stretch and sits under the
                // glyphs (`renderer.ts:5987-5994`).
                let def = measurer.style(*style);
                if let Some(color) = def.highlight {
                    decor::push_highlight(
                        commands,
                        line_x + pen_x,
                        baseline_y + def.baseline_shift_emu(),
                        width + extra,
                        def.draw_size_emu(),
                        color,
                        transform,
                    );
                }
                pen_x += width + extra;
                unit += text.chars().count();
            }
            LineItem::Tab { width, .. } => {
                pen_x += width + gap_at(justify, unit);
                unit += 1;
            }
            LineItem::Object { width, .. } => {
                let extra = gap_at(justify, unit);
                let offset = line_x + pen_x + extra;
                let mut cursor = para.object_cursor.borrow_mut();
                if let Some(piece) = para.math.get(*cursor) {
                    let mut runs = piece.runs.clone();
                    for run in &mut runs {
                        for glyph in &mut run.glyphs {
                            glyph.x += offset;
                            glyph.y += baseline_y;
                        }
                    }
                    final_runs.extend(runs);
                    for path in &piece.paths {
                        let shifted = math::translate_math_path(path, offset, baseline_y);
                        commands.push(math::math_path_command(&shifted, transform));
                    }
                }
                *cursor += 1;
                pen_x += width + extra;
                unit += 1;
            }
        }
    }
    if let Some(upper) = upper {
        if !upper.runs.is_empty() {
            // The upright cells sit in slide coordinates already, so only the
            // element transform applies (`EaVertFrame::upright_origin`).
            commands.push(VectorCommand::DrawText {
                runs: upper.runs,
                transform: upper.frame.base,
            });
        }
    }
}

/// The upright `eaVert` cells of one line, collected for a DrawText command of
/// its own (they are not painted inside the +90° frame).
struct Upper<'a> {
    frame: &'a EaVertFrame,
    runs: Vec<PositionedGlyphRun>,
}

/// The justification plan for one line, or `None` when it stays ragged.
///
/// ref renderer.ts:5885-5925: only the four fill alignments justify, the
/// paragraph's final line and a line closed by `<a:br/>` stay natural under
/// `just`, and a tabbed or bidi line is never re-spaced.
pub(super) fn justify_plan(para: &ParagraphLayout, line: &LineLayout, avail: f32) -> Option<JustifyPlan> {
    let mode = match para.alignment.as_str() {
        "just" | "justLow" => JustifyMode::Just,
        "dist" => JustifyMode::Dist,
        "thaiDist" => JustifyMode::ThaiDist,
        _ => return None,
    };
    if !metrics::should_justify(&para.alignment, line.is_last_line, line.ends_with_break) {
        return None;
    }
    // ref renderer.ts:5917 `justifyMode && !paraNeedsBidi && !hasTab` (the bidi
    // probe is approximated by the paragraph's base direction).
    if para.rtl || line.items.iter().any(|item| matches!(item, LineItem::Tab { .. })) {
        return None;
    }
    justify::justify_line(
        &line.items,
        avail,
        line.width,
        mode,
        line.is_last_line,
        line.ends_with_break,
    )
}

/// The advance one [`LineItem`] occupies under the current (draw-time) widths.
pub(super) fn item_advance(item: &LineItem) -> f32 {
    match item {
        LineItem::Text { width, .. } | LineItem::Tab { width, .. } | LineItem::Object { width, .. } => *width,
    }
}

/// `renderer.ts:5683-5722`: resolve the line's inline tabs at DRAW time in the
/// logical READING frame.
///
/// The pen starts at the leading indent — `marR` under an RTL base, because the
/// draw-side first-line indent only shifts the LTR pen — and the trailing limit
/// is the text area including both paragraph margins. Each tab therefore
/// measures an absolute distance from the leading text-inset edge; the UAX#9 L2
/// pass later mirrors the resulting cells physically for an RTL base.
pub(super) fn resolve_line_tabs(
    items: &mut [LineItem],
    para: &ParagraphLayout,
    avail_w: f32,
    first_line_indent: f32,
    base_rtl: bool,
    measurer: &Measurer,
) {
    let Some(tab_style) = items.iter().find_map(|item| match item {
        LineItem::Tab { style, .. } => Some(*style),
        _ => None,
    }) else {
        return;
    };
    let start_pen = if base_rtl { para.mar_r } else { first_line_indent };
    let no_stop_gap = measurer.space(tab_style);
    crate::text::tabs::resolve_tab_widths(items, &para.tab_stops, start_pen, avail_w, no_stop_gap, para.tab_size);
}

/// The line's painted segments in logical order, for the UAX#9 L2 pass
/// (`LineItem::Tab` is Bidi_Class `S`, an equation or picture marker is the
/// neutral U+FFFC placeholder).
pub(super) fn bidi_segments(items: &[LineItem]) -> Vec<BidiSegment<'_>> {
    items
        .iter()
        .map(|item| match item {
            LineItem::Text { text, .. } => BidiSegment::Text(text.as_str()),
            LineItem::Tab { .. } => BidiSegment::Tab,
            LineItem::Object { .. } => BidiSegment::Object,
        })
        .collect()
}

/// `per_gap` when the plan opens a boundary immediately before flat unit
/// `unit` (`JustifyPlan::gap_after` is indexed by boundary, len = units + 1).
fn gap_at(plan: Option<&JustifyPlan>, unit: usize) -> f32 {
    match plan {
        Some(plan) if plan.gap_after.get(unit).copied().unwrap_or(false) => plan.per_gap,
        _ => 0.0,
    }
}

/// Paint one text item, splitting it at every open justification boundary the
/// way `justifyLine` splits a segment (`text-justify.ts:118-141`), and return
/// the extra advance the gaps added. The pen still advances by the breaker's
/// item width, so the split never shifts what follows.
///
/// `justification` pairs the line's plan with this item's offset in the flat
/// unit stream (`JustifyPlan::gap_after` is indexed by boundary).
#[allow(clippy::too_many_arguments)]
fn place_text(
    measurer: &Measurer,
    text: &str,
    style: StyleId,
    x: f32,
    baseline_y: f32,
    justification: Option<(&JustifyPlan, usize)>,
    mut upper: Option<&mut Upper<'_>>,
    out: &mut Vec<PositionedGlyphRun>,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let mut extra = 0.0f32;
    let Some((plan, unit_start)) = justification else {
        place_piece(measurer, text, style, x, baseline_y, upper, out);
        return extra;
    };
    let mut pen = 0.0f32;
    let mut start = 0usize;
    for (boundary, (offset, _ch)) in text.char_indices().enumerate() {
        if !plan.gap_after.get(unit_start + boundary).copied().unwrap_or(false) {
            continue;
        }
        // A gap only ever falls between two code points, so the piece before
        // it is never empty (`line-gaps.ts:139-171`).
        if boundary > 0 {
            let piece = &text[start..offset];
            pen += place_piece(
                measurer,
                piece,
                style,
                x + pen + extra,
                baseline_y,
                upper.as_deref_mut(),
                out,
            );
            start = offset;
        }
        extra += plan.per_gap;
    }
    place_piece(measurer, &text[start..], style, x + pen + extra, baseline_y, upper, out);
    extra
}

/// Paint one justification piece at pen `x` and return the advance it used.
fn place_piece(
    measurer: &Measurer,
    text: &str,
    style: StyleId,
    x: f32,
    baseline_y: f32,
    upper: Option<&mut Upper<'_>>,
    out: &mut Vec<PositionedGlyphRun>,
) -> f32 {
    match upper {
        None => {
            emit_run(measurer, text, style, x, baseline_y, out);
            measurer.advance(text, style)
        }
        Some(upper) => emit_ea_vert(measurer, text, style, x, baseline_y, upper, out),
    }
}

/// `drawEaVertRunWithCapability` (`vertical-text.ts:96-170`) for one text item:
/// every East-Asian cluster is stood upright in its own cell, consecutive
/// sideways (Latin / digit / ASCII punctuation) clusters stay one rotated run.
///
/// Approximated: the reference counter-rotates each cell about its centre and
/// re-centres substituted punctuation ink with `inkCenterAboveMiddlePx`; here the
/// em box is centred on the column axis and the ink correction is 0. The cell
/// advance is the authored cluster's, exactly as the reference measures the
/// original code point (`vertical-text.ts:118-121`), so no half-em compression
/// is applied to punctuation cells.
fn emit_ea_vert(
    measurer: &Measurer,
    text: &str,
    style: StyleId,
    x: f32,
    baseline_y: f32,
    upper: &mut Upper<'_>,
    out: &mut Vec<PositionedGlyphRun>,
) -> f32 {
    let def = measurer.style(style);
    let centre_above = vertical::em_centre_above(
        def.draw_size_emu(),
        shape::ascent_share_for(&def.family, def.bold, def.italic),
    );
    let baseline = baseline_y + def.baseline_shift_emu();
    let mut pen = 0.0f32;
    for piece in vertical::ea_vert_pieces(text, |ch| has_glyph(measurer, style, ch)) {
        let measure_advance = measurer.advance(&piece.measured, style);
        let cell_pitch = measure_advance + def.letter_spacing_emu * (piece.measured.chars().count() as f32);
        if piece.upright {
            // One run per upright cell: both paint backends place a run from its
            // first glyph origin, so a column cannot share a run. The reference
            // likewise fills one code point at a time (`vertical-text.ts:143`).
            let (glyph_x, glyph_y) =
                upper
                    .frame
                    .upright_origin(x + pen + cell_pitch * 0.5, baseline, measure_advance, centre_above);
            emit_run_at(measurer, &piece.painted, style, glyph_x, glyph_y, &mut upper.runs);
        } else {
            emit_run(measurer, &piece.painted, style, x + pen, baseline_y, out);
        }
        pen += cell_pitch;
    }
    pen
}

thread_local! {
    /// `(family, bold, italic, char)` → the resolved stack shapes a real glyph
    /// (id 0 is `.notdef`), the vertical-form capability probe of
    /// `vertical-text.ts:171-183`.
    static GLYPH_PRESENCE: RefCell<HashMap<(String, bool, bool, char), bool>> =
        RefCell::new(HashMap::new());
}

/// Whether the resolved face stack has `vert` feature coverage for long marks or
/// can draw `ch`.
fn has_glyph(measurer: &Measurer, style: StyleId, ch: char) -> bool {
    let def = measurer.style(style);
    if matches!(ch, '\u{30fc}' | '\u{301c}' | '\u{ff5e}') {
        return super::face_features::has_vert_feature(&def.family, def.bold, def.italic, ch);
    }
    let key = (def.family.clone(), def.bold, def.italic, ch);
    if let Some(present) = GLYPH_PRESENCE.with(|cache| cache.borrow().get(&key).copied()) {
        return present;
    }
    let present = shape::shape(&ch.to_string(), &def.family, def.draw_size_emu(), def.bold, def.italic)
        .glyphs
        .first()
        .is_some_and(|(id, _, _)| *id != 0);
    GLYPH_PRESENCE.with(|cache| {
        cache.borrow_mut().insert(key, present);
    });
    present
}

/// Shape and place one run at pen position `x` on `baseline_y`.
///
/// A run carrying a non-zero `rPr@baseline` shapes at 65 % of its authored
/// size and rides the shifted baseline (`renderer.ts:1552`, `5981`); the line
/// box itself still comes from the authored size (see [`natural_box`]).
pub(super) fn emit_run(
    measurer: &Measurer,
    text: &str,
    style: StyleId,
    x: f32,
    baseline_y: f32,
    out: &mut Vec<PositionedGlyphRun>,
) {
    let def = measurer.style(style);
    let run_baseline = baseline_y + def.baseline_shift_emu();
    emit_run_at(measurer, text, style, x, run_baseline, out);
}

/// Shape `text` and push one run on the ABSOLUTE baseline `baseline` (EMU): the
/// caller has already applied any `rPr@baseline` shift.
fn emit_run_at(
    measurer: &Measurer,
    text: &str,
    style: StyleId,
    x: f32,
    baseline: f32,
    out: &mut Vec<PositionedGlyphRun>,
) {
    let def = measurer.style(style);
    let shaped = shape::shape(text, &def.family, def.draw_size_emu(), def.bold, def.italic);
    let glyphs: Vec<PositionedGlyph> = shaped
        .glyphs
        .iter()
        .map(|(id, glyph_x, advance)| PositionedGlyph {
            glyph_id: *id,
            x: x + *glyph_x,
            y: baseline,
            advance: *advance,
            warp_u: None,
        })
        .collect();
    // The ids are paintable straight from the IR only when the shaping used
    // ONE face and that face is the family's own cosmic match: the raster then
    // re-resolves the same face and paints without re-shaping. A run shaped
    // across fallback faces (or through a fallback face because the family
    // lacks the weight/coverage) keeps the re-shaping raster path.
    let single_face = crate::text::fonts::with_face_db(|db| {
        crate::text::fonts::cosmic_family_face_id(db, &def.family, def.bold, def.italic)
    })
    .is_some_and(|expected| !shaped.font_ids.is_empty() && shaped.font_ids.iter().all(|id| *id == expected));
    out.push(PositionedGlyphRun {
        text: text.to_string(),
        font_family: def.family.clone(),
        font_size_pt: def.draw_size_pt(),
        color: def.color,
        bold: def.bold,
        italic: def.italic,
        glyphs,
        decorations: super::run_decorations::resolve(def),
        single_face,
        face: None,
        warp: None,
        reflection: def.reflection.clone(),
    });
}
