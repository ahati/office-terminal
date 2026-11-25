//! Stacked WordArt (`a:bodyPr@vert="wordArtVert" / "wordArtVertRtl"`).
//!
//! Split out of [`super::paint`] to keep both halves under the file limit.
//! Reference: `renderer.ts:4849-5028` (`renderStackedTextBody`) and
//! `core/src/drawingml-text/stacked.ts` (`layoutStackedText`).

use std::f32::consts::FRAC_PI_2;

use crossbeam_channel::Sender;

use super::decor;
use super::emit::Measurer;
use super::paint;
use super::paragraphs::{self, MarkerPlan};
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
        let def = measurer.style(style);
        let (painted, rotation, advance) = if cluster.chars().count() == 1 {
            let ch = cluster.chars().next().unwrap_or(' ');
            if is_ascii_bracket(ch) {
                (cluster.to_string(), FRAC_PI_2, width)
            } else if ch >= '\u{0080}' && super::face_features::has_vrt2_feature(&def.family, def.bold, def.italic, ch)
            {
                // vrt2-only coverage outside Basic Latin (e.g. ∥ in Yu Gothic):
                // stands unturned in a 1 em cell as its horizontal glyph.
                (cluster.to_string(), 0.0, size)
            } else if super::face_features::has_vert_feature(&def.family, def.bold, def.italic, ch) {
                // vert coverage: ∥ turns; vertical_form stands upright.
                if ch == '\u{2225}' {
                    (cluster.to_string(), FRAC_PI_2, width)
                } else if let Some(form) = vertical::vertical_form(ch) {
                    (form.to_string(), 0.0, size)
                } else {
                    (cluster.to_string(), FRAC_PI_2, width)
                }
            } else if let Some(form) = vertical::vertical_form(ch) {
                // When a long mark has no vert feature coverage (e.g. bold JhengHei),
                // it rotates horizontal glyph rather than substituting.
                if matches!(ch, '\u{30fc}' | '\u{301c}') {
                    (cluster.to_string(), FRAC_PI_2, width)
                } else {
                    (form.to_string(), 0.0, size)
                }
            } else if ch == '\u{2225}' {
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

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> crate::contracts::VectorPath {
    use crate::contracts::{FillRule, PathSegment, VectorPath};
    VectorPath::new(
        vec![
            PathSegment::MoveTo { x, y },
            PathSegment::LineTo { x: x + w, y },
            PathSegment::LineTo { x: x + w, y: y + h },
            PathSegment::LineTo { x, y: y + h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    )
}

fn slot_rotation(slot: &vertical::StackedSlot, base: &AffineTransform) -> AffineTransform {
    if slot.rotation == 0.0 {
        return *base;
    }
    let cx = slot.x + slot.width * 0.5;
    let cy = slot.baseline - slot.ascent + slot.advance * 0.5;
    let rotation = AffineTransform::translate(cx, cy)
        .post_concat(&AffineTransform::from_rotation(slot.rotation))
        .post_concat(&AffineTransform::translate(-cx, -cy));
    base.post_concat(&rotation)
}

/// Stacked underline is one vertical frame per column, on the column's leading
/// side at the drawing face's descent. Strikes cross each glyph in its own
/// frame; the invisible paragraph mark inherits the last run's strike unless an
/// explicit `endParaRPr` turns it off.
#[allow(clippy::too_many_arguments)]
fn push_stacked_decorations(
    text_body: &pptx_model::TextBody,
    paragraphs: &[super::paragraphs::ParagraphLayout],
    slots: &[vertical::StackedSlot],
    measurer: &Measurer,
    base_transform: &AffineTransform,
    commands: &mut Vec<VectorCommand>,
) {
    struct UnderlineSpan {
        column: usize,
        style: StyleId,
        size: f32,
        color: [u8; 4],
        left: f32,
        top: f32,
        bottom: f32,
        descent: f32,
    }
    let mut span: Option<UnderlineSpan> = None;
    let flush = |span: &mut Option<UnderlineSpan>, commands: &mut Vec<VectorCommand>| {
        let Some(span) = span.take() else {
            return;
        };
        let weight = (span.size * 0.05).max(12_700.0);
        let x = span.left + span.descent - 2.5 * 12_700.0 - weight * 0.5;
        commands.push(VectorCommand::draw_path(
            rect_path(x, span.top, weight, (span.bottom - span.top).max(1.0)),
            Some(crate::contracts::VectorPaint::Solid(span.color)),
            None,
            *base_transform,
        ));
    };

    for slot in slots {
        if slot.object.is_some() {
            flush(&mut span, commands);
            continue;
        }
        let def = measurer.style(slot.style);
        if !def.underline {
            flush(&mut span, commands);
            continue;
        }
        let left = slot.x - (slot.thickness - slot.width) * 0.5;
        let top = slot.baseline - slot.ascent;
        let bottom = top + slot.advance;
        let color = def.underline_color.unwrap_or(def.color);
        let same = span.as_ref().is_some_and(|span| {
            span.column == slot.column
                && span.color == color
                && measurer.style(span.style).underline_style == def.underline_style
        });
        if same {
            if let Some(span) = span.as_mut() {
                span.bottom = span.bottom.max(bottom);
                span.descent = span.descent.max(slot.descent);
                span.size = span.size.max(def.size_emu);
            }
        } else {
            flush(&mut span, commands);
            span = Some(UnderlineSpan {
                column: slot.column,
                style: slot.style,
                size: def.size_emu,
                color,
                left,
                top,
                bottom,
                descent: slot.descent,
            });
        }
    }
    flush(&mut span, commands);

    // Per-glyph strikes.
    for slot in slots {
        if slot.object.is_some() {
            continue;
        }
        let def = measurer.style(slot.style);
        if !def.strikethrough {
            continue;
        }
        let h = (def.size_emu * 0.05).max(12_700.0);
        let base_y = slot.baseline - def.size_emu * 0.35 - h * 0.5;
        let transform = slot_rotation(slot, base_transform);
        commands.push(VectorCommand::draw_path(
            rect_path(slot.x, base_y, slot.width.max(1.0), h),
            Some(crate::contracts::VectorPaint::Solid(def.color)),
            None,
            transform,
        ));
        if def.strike_double {
            commands.push(VectorCommand::draw_path(
                rect_path(slot.x, base_y + h * 1.8, slot.width.max(1.0), h),
                Some(crate::contracts::VectorPaint::Solid(def.color)),
                None,
                transform,
            ));
        }
    }

    // Paragraph mark: an invisible cell after the paragraph's last glyph.
    for (paragraph_index, slots) in slots.iter().filter(|slot| slot.object.is_none()).fold(
        Vec::<(usize, Vec<&vertical::StackedSlot>)>::new(),
        |mut acc, slot| {
            match acc.last_mut() {
                Some((index, list)) if *index == slot.paragraph => list.push(slot),
                _ => acc.push((slot.paragraph, vec![slot])),
            }
            acc
        },
    ) {
        let Some(last) = slots.last() else {
            continue;
        };
        let end_run = text_body
            .paragraphs
            .get(paragraph_index)
            .and_then(|para| para.end_run_properties.as_deref());
        let mark_style = end_run
            .and_then(|run| run.font_size)
            .map(|size| (size as f32 * 12700.0, end_run))
            .unwrap_or((measurer.style(last.style).size_emu, end_run));
        let (size, end_run) = mark_style;
        let struck = end_run
            .map(|run| run.strikethrough)
            .unwrap_or_else(|| measurer.style(last.style).strikethrough);
        if !struck {
            continue;
        }
        let color = end_run
            .and_then(|run| run.color.as_deref())
            .map(crate::compiler::style::parse_hex_color)
            .unwrap_or_else(|| measurer.style(last.style).color);
        let h = (size * 0.05).max(12_700.0);
        let x = last.x + last.width.max(1.0);
        let y = last.baseline - size * 0.35 - h * 0.5;
        commands.push(VectorCommand::draw_path(
            rect_path(x, y, 8.0 * 12_700.0, h),
            Some(crate::contracts::VectorPaint::Solid(color)),
            None,
            *base_transform,
        ));
    }
    let _ = paragraphs;
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
        let mut break_before = paragraph_index > 0;
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
                    } else {
                        // Empty equation (e.g. no engine): reserve the mark cell!
                        let (cell, ascent, descent, size) = cell_metrics(measurer, *style);
                        glyphs.push(vertical::StackedGlyph {
                            text: String::new(),
                            style: *style,
                            advance: cell,
                            width: (*width).max(size),
                            thickness: cell,
                            ascent,
                            descent,
                            rotation: 0.0,
                            object: None,
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
    push_stacked_decorations(text_body, &paragraphs, &slots, measurer, base_transform, &mut commands);
    for slot in &slots {
        if let Some(object) = slot.object {
            if let Some(piece) = paragraphs.get(slot.paragraph).and_then(|para| para.math.get(object)) {
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
            if let Some(run) = runs.last_mut() {
                run.decorations = Default::default();
            }
        } else {
            let mut rotated: Vec<PositionedGlyphRun> = Vec::new();
            paint::emit_run(measurer, &slot.text, slot.style, slot.x, slot.baseline, &mut rotated);
            for run in &mut rotated {
                run.decorations = Default::default();
            }
            commands.push(VectorCommand::DrawText {
                runs: rotated,
                transform: slot_rotation(slot, base_transform),
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
