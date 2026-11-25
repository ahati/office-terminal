//! `spAutoFit` preflight: does a paragraph need wrapping?
//!
//! ECMA-376 §20.1.10.5 asks the editor to grow a shape to fit its text. The
//! reference only lets such a shape wrap when a paragraph's natural line width
//! exceeds the text box; otherwise the line stays intact and the shape grows.
//! Shared by the wrap decision and (in the reference) the growth measurement.
//!
//! ref `renderer.ts:1539-1580` (`naturalWidthExceedsBbox`).

use crate::text::linebreak::BreakItem;
use crate::text::measure::TextMeasure;

use super::emit::Measurer;
use super::paragraphs::ParagraphLayout;

/// Whether any paragraph's natural (unwrapped) first-line width overflows the
/// text box. A bullet does not consume first-line indent; a non-bullet
/// negative indent legitimately extends the line left of `marL`.
pub(super) fn natural_width_exceeds_bbox(
    paragraphs: &[ParagraphLayout],
    box_w: f32,
    l_ins: f32,
    r_ins: f32,
    measurer: &Measurer,
) -> bool {
    let avail = (box_w - l_ins - r_ins).max(0.0);
    for para in paragraphs {
        let first_indent = if para.marker.is_some() { 0.0 } else { para.indent };
        let mut width = para.mar_l + first_indent + para.mar_r;
        for item in &para.items {
            match item {
                BreakItem::Text { text, style } => {
                    width += measurer.advance(text, *style);
                }
                BreakItem::Object { width: object_w, .. } => width += *object_w,
                // Tabs resolve against authored stops and are unadjudicated
                // under spAutoFit; keep wrapping for them.
                BreakItem::Tab | BreakItem::LineBreak => return true,
            }
        }
        if width > avail {
            return true;
        }
    }
    false
}
