//! Line breaking and column-fill measurement for a text body.
//!
//! Shared by the emission pass ([`super::emit::compile_text_body_with_markers`])
//! and the shrink-on-overflow scale search ([`super::shrink`]): the search only
//! reads [`BrokenBody::anchor_h`], while the emission pass places every line
//! the breaker produced. The function is deterministic in its inputs, so a
//! measurement pass and the emission pass at the same font scale yield
//! identical line boxes.

use crate::text::columns::{self, ColumnPlan};
use crate::text::emit::{LineLayout, Measurer};
use crate::text::linebreak::{break_paragraph, BreakOptions};
use crate::text::paragraphs::{self, ParagraphLayout};
use crate::text::{metrics, natural_box};

/// The wrapped layout of one text body: positioned line boxes, the column
/// plan that distributes them over `bodyPr@numCol` tracks, and the tallest
/// column's height (the block the vertical anchor measures against).
pub(super) struct BrokenBody {
    pub(super) lines: Vec<LineLayout>,
    pub(super) plan: ColumnPlan,
    pub(super) anchor_h: f32,
}

/// Breaks the built paragraphs into line boxes and measures the columns.
///
/// `avail_w` is the per-column content width (insets and column gaps already
/// subtracted); `do_wrap` is the emission pass's wrap decision; `reduction` is
/// the stored `normAutofit@lnSpcReduction` (percentage line spacing only).
#[allow(clippy::too_many_arguments)]
pub(super) fn break_body_lines(
    text_body: &pptx_model::TextBody,
    paragraphs: &[ParagraphLayout],
    measurer: &Measurer,
    avail_w: f32,
    do_wrap: bool,
    reduction: f32,
    t_ins: f32,
    b_ins: f32,
    layout_h: f32,
    num_col: usize,
) -> BrokenBody {
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
            measurer,
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
            let (nat_ascent, nat_descent) = natural_box::natural_box(measurer, line, para, &pieces);
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
    BrokenBody {
        lines,
        plan,
        anchor_h,
    }
}
