//! Stacked WordArt (`a:bodyPr@vert="wordArtVert" / "wordArtVertRtl"`).
//!
//! Split out of [`super::paint`] to keep both halves under the file limit.
//! Reference: `renderer.ts:4849-5028` (`renderStackedTextBody`).

use crossbeam_channel::Sender;

use super::decor;
use super::emit::Measurer;
use super::paragraphs::{self, MarkerPlan};
use super::paint;
use crate::contracts::{AffineTransform, PositionedGlyphRun, RectF32, VectorCommand, WarningEvent};
use crate::text::bullets::MarkerBitmap;
use crate::text::linebreak::BreakItem;
use crate::text::measure::{StyleId, TextMeasure};
use crate::text::metrics::{self, FaceMetrics};
use crate::text::shape;
use crate::text::vertical;

/// `renderStackedTextBody` reduced to one grapheme per column: upright glyphs
/// read down a column, columns left-to-right for `wordArtVert` and
/// right-to-left for `wordArtVertRtl`.
///
/// Approximations (see `text::vertical`): a grapheme is one column, so no
/// column ever wraps; `a:lnSpc`, `spcBef`/`spcAft` column gaps and inline
/// objects are not laid out, and the paragraph alignment is not applied along
/// the column (the single glyph is centred).
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_stacked(
    text_body: &pptx_model::TextBody,
    box_w: i64,
    box_h: i64,
    base_transform: &AffineTransform,
    measurer: &mut Measurer,
    default_style: StyleId,
    default_color: [u8; 4],
    rtl: bool,
    bitmaps: Option<&dyn MarkerBitmap>,
    warning_sink: &Sender<WarningEvent>,
) -> Vec<VectorCommand> {
    let font_scale = text_body.font_scale.unwrap_or(1.0) as f32;
    let paragraphs = paragraphs::build_paragraphs(
        text_body,
        default_style,
        font_scale,
        default_color,
        measurer,
        warning_sink,
    );
    let mut glyphs: Vec<vertical::StackedGlyph> = Vec::new();
    // Column-range of each paragraph in `glyphs`, so a marker can sit on its
    // paragraph's column (`drawMarker`, `renderer.ts:4944-4961`).
    let mut blocks: Vec<(usize, usize, Option<MarkerPlan>)> = Vec::new();
    for para in &paragraphs {
        let start = glyphs.len();
        for item in &para.items {
            let BreakItem::Text { text, style } = item else {
                continue;
            };
            let def = measurer.style(*style);
            let (ascent, descent) = metrics::natural_line_box(&[(
                def.size_emu,
                FaceMetrics {
                    ascent_share: shape::ascent_share_for(&def.family, def.bold, def.italic),
                },
            )]);
            for grapheme in vertical::graphemes(text) {
                let advance = measurer.advance(grapheme, *style).max(1.0);
                glyphs.push(vertical::StackedGlyph {
                    text: grapheme.to_string(),
                    style: *style,
                    advance,
                    thickness: advance,
                    ascent,
                    descent,
                });
            }
        }
        blocks.push((start, glyphs.len(), para.marker.clone()));
    }
    let rect = RectF32::new(
        text_body.l_ins as f32,
        text_body.t_ins as f32,
        (box_w as f32 - text_body.l_ins as f32 - text_body.r_ins as f32).max(0.0),
        (box_h as f32 - text_body.t_ins as f32 - text_body.b_ins as f32).max(0.0),
    );
    let slots = vertical::place_stacked(
        &glyphs,
        rect,
        &text_body.vertical_anchor,
        "ctr",
        text_body.anchor_ctr,
        rtl,
    );
    let mut runs: Vec<PositionedGlyphRun> = Vec::new();
    let mut commands: Vec<VectorCommand> = Vec::new();
    for slot in &slots {
        paint::emit_run(measurer, &slot.text, slot.style, slot.x, slot.baseline, &mut runs);
    }
    // §21.1.2.4.2 picture markers: one image per paragraph, centred on the
    // column axis at the column top (`drawMarker`, `renderer.ts:4944-4961`).
    for (start, end, marker) in &blocks {
        let Some(MarkerPlan::Image { rel_id, size_emu }) = marker else {
            continue;
        };
        let Some(slot) = slots.get(*start).filter(|_| *end > *start) else {
            continue;
        };
        // Centred on the column axis, bottom of the marker on the column
        // baseline (`drawMarker`, `renderer.ts:4952-4959`).
        let width = paragraphs::picture_width(rel_id, *size_emu, bitmaps).unwrap_or(0.0);
        let axis_x = slot.x + glyphs[*start].advance * 0.5;
        decor::push_marker_image(
            rel_id,
            *size_emu,
            axis_x - width * 0.5,
            slot.baseline,
            base_transform,
            bitmaps,
            &mut commands,
        );
    }
    if runs.is_empty() {
        return commands;
    }
    commands.push(VectorCommand::DrawText {
        runs,
        transform: *base_transform,
    });
    commands
}
