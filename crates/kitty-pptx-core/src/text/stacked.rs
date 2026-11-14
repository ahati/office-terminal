//! Stacked WordArt (`a:bodyPr@vert="wordArtVert" / "wordArtVertRtl"`).
//!
//! Split out of [`super::paint`] to keep both halves under the file limit.
//! Reference: `renderer.ts:4849-5028` (`renderStackedTextBody`) and
//! `core/src/drawingml-text/stacked.ts` (`layoutStackedText`).

use std::f32::consts::FRAC_PI_2;

use crossbeam_channel::Sender;

use super::decor;
use super::emit::Measurer;
use super::paragraphs::{self, MarkerPlan};
use super::paint;
use crate::compiler::math;
use crate::contracts::{AffineTransform, PositionedGlyphRun, RectF32, VectorCommand, WarningEvent};
use crate::text::bullets::MarkerBitmap;
use crate::text::faces;
use crate::text::linebreak::BreakItem;
use crate::text::measure::{StyleId, TextMeasure};
use crate::text::metrics;
use crate::text::metrics::FaceMetrics;
use crate::text::shape;
use crate::text::vertical;

/// DrawingML's stacked-cell factor (`STACKED_CELL_FACTOR`).
const STACKED_CELL_FACTOR: f32 = 7.0 / 6.0;

fn is_ascii_bracket(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>')
}

fn cell_metrics(measurer: &Measurer, style: StyleId) -> (f32, f32, f32, f32) {
    let def = measurer.style(style);
    let size = def.draw_size_emu().max(1.0);
    let face = faces::power_point_face_metrics(&def.family, def.bold, def.italic);
    let (box_ascent, box_descent) = face
        .as_ref()
        .and_then(|metrics| metrics.glyph)
        .map(|box_| (box_.ascent, box_.descent))
        .unwrap_or_else(|| {
            let (ascent, descent) = metrics::natural_line_box(&[(
                size,
                FaceMetrics {
                    ascent_share: shape::ascent_share_for(&def.family, def.bold, def.italic),
                },
            )]);
            (ascent / size, descent / size)
        });
    let cell = STACKED_CELL_FACTOR * (box_ascent + box_descent) * size;
    let descent = box_descent * size;
    (cell, cell - descent, descent, size)
}

fn push_clusters(
    measurer: &mut Measurer,
    stream: &[(char, StyleId)],
    paragraph: usize,
    break_before: bool,
    out: &mut Vec<vertical::StackedGlyph>,
) {
    if stream.is_empty() {
        return;
    }
    let text: String = stream.iter().map(|(ch, _)| *ch).collect();
    let mut cursor = 0usize;
    let mut first = break_before;
    for cluster in vertical::graphemes(&text) {
        let Some((_, style)) = stream.get(cursor) else {
            break;
        };
        let style = *style;
        let (cell, ascent, descent, size) = cell_metrics(measurer, style);
        let width = measurer.advance(cluster, style).max(1.0);
        let (painted, rotation, advance) = if cluster.chars().count() == 1 {
            let ch = cluster.chars().next().unwrap_or(' ');
            if let Some(form) = vertical::vertical_form(ch) {
                (form.to_string(), 0.0, size)
            } else if is_ascii_bracket(ch) {
                (cluster.to_string(), FRAC_PI_2, width)
            } else {
                (cluster.to_string(), 0.0, cell)
            }
        } else {
            (cluster.to_string(), 0.0, cell)
        };
        out.push(vertical::StackedGlyph {
            text: painted,
            style,
            advance,
            width,
            thickness: cell,
            ascent,
            descent,
            rotation,
            object: None,
            paragraph,
            break_before: first,
        });
        first = false;
        cursor += cluster.chars().count();
    }
}

/// `renderStackedTextBody`: upright glyphs flow down a column; overflow wraps
/// to the next column; `wordArtVertRtl` fills columns right-to-left.
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
    // Column-range of each paragraph's text in `glyphs`, so an image marker can
    // sit on its paragraph's first column (`renderer.ts:4944-4961`).
    let mut blocks: Vec<(usize, usize, Option<MarkerPlan>)> = Vec::new();
    for (paragraph_index, para) in paragraphs.iter().enumerate() {
        let start = glyphs.len();
        // Prefix cells: character and auto-number markers are upright cells at
        // the paragraph start (`stacked-text.ts:463`).
        if let Some(MarkerPlan::Label { text, style }) = &para.marker {
            let stream: Vec<(char, StyleId)> = text.chars().map(|ch| (ch, *style)).collect();
            push_clusters(measurer, &stream, paragraph_index, false, &mut glyphs);
        }
        let mut stream: Vec<(char, StyleId)> = Vec::new();
        let mut break_before = false;
        let mut math_cursor = 0usize;
        for item in &para.items {
            match item {
                BreakItem::Text { text, style } => {
                    stream.extend(text.chars().map(|ch| (ch, *style)));
                }
                BreakItem::LineBreak => {
                    push_clusters(measurer, &stream, paragraph_index, break_before, &mut glyphs);
                    stream.clear();
                    break_before = true;
                }
                BreakItem::Tab => {
                    // A tab has no stacked advance; treat it as a cell-sized
                    // space so authored separators do not vanish.
                    stream.push((' ', para.default_style));
                }
                BreakItem::Object { width, style } => {
                    push_clusters(measurer, &stream, paragraph_index, break_before, &mut glyphs);
                    stream.clear();
                    break_before = false;
                    if let Some(piece) = para.math.get(math_cursor) {
                        let (_cell, _ascent, _descent, _size) = cell_metrics(measurer, *style);
                        glyphs.push(vertical::StackedGlyph {
                            text: String::new(),
                            style: *style,
                            advance: (piece.ascent + piece.descent).max(1.0),
                            width: (*width).max(1.0),
                            thickness: (*width).max(1.0),
                            ascent: piece.ascent,
                            descent: piece.descent,
                            rotation: 0.0,
                            object: Some(math_cursor),
                            paragraph: paragraph_index,
                            break_before: false,
                        });
                    }
                    math_cursor += 1;
                }
            }
        }
        push_clusters(measurer, &stream, paragraph_index, break_before, &mut glyphs);
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
        "l",
        text_body.anchor_ctr,
        rtl,
    );
    let mut runs: Vec<PositionedGlyphRun> = Vec::new();
    let mut commands: Vec<VectorCommand> = Vec::new();
    for slot in &slots {
        if let Some(object) = slot.object {
            if let Some(piece) = paragraphs
                .get(slot.paragraph)
                .and_then(|para| para.math.get(object))
            {
                for path in &piece.paths {
                    let shifted = math::translate_math_path(path, slot.x, slot.baseline);
                    commands.push(math::math_path_command(&shifted, base_transform));
                }
                let mut math_runs = piece.runs.clone();
                for run in &mut math_runs {
                    for glyph in &mut run.glyphs {
                        glyph.x += slot.x;
                        glyph.y += slot.baseline;
                    }
                }
                runs.extend(math_runs);
            }
            continue;
        }
        if slot.rotation == 0.0 {
            paint::emit_run(measurer, &slot.text, slot.style, slot.x, slot.baseline, &mut runs);
        } else {
            let mut rotated: Vec<PositionedGlyphRun> = Vec::new();
            paint::emit_run(measurer, &slot.text, slot.style, slot.x, slot.baseline, &mut rotated);
            let cx = slot.x + slot.width * 0.5;
            let cy = slot.baseline - slot.ascent + slot.advance * 0.5;
            let rotation = AffineTransform::translate(cx, cy)
                .post_concat(&AffineTransform::from_rotation(slot.rotation))
                .post_concat(&AffineTransform::translate(-cx, -cy));
            commands.push(VectorCommand::DrawText {
                runs: rotated,
                transform: base_transform.post_concat(&rotation),
            });
        }
    }
    // §21.1.2.4.2 picture markers: one image per paragraph, centred on the
    // column axis at the column top (`renderer.ts:4944-4961`).
    for (start, end, marker) in &blocks {
        let Some(MarkerPlan::Image { rel_id, size_emu }) = marker else {
            continue;
        };
        let Some(slot) = slots.get(*start).filter(|_| *end > *start) else {
            continue;
        };
        let width = paragraphs::picture_width(rel_id, *size_emu, bitmaps).unwrap_or(0.0);
        let axis_x = slot.x + slot.width * 0.5;
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
    if !runs.is_empty() {
        commands.push(VectorCommand::DrawText {
            runs,
            transform: *base_transform,
        });
    }
    commands
}
