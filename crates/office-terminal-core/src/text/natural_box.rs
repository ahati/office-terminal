//! Natural line box for a broken line, including math segments.
//!
//! Split out of [`super::paint`] to keep both modules under the file-size
//! limit. The reference model is `renderer.ts:5260-5438`: a line with an
//! equation fails the face-metric gate and uses the synthetic 0.8/0.2 split of
//! `1.2 × maxSizePx`, where a math segment claims `(ascent + descent) / 1.2`.

use super::emit::Measurer;
use super::linebreak::{BrokenLine, LineItem};
use super::paragraphs::ParagraphLayout;
use crate::compiler::math::MathPiece;
use crate::text::metrics::{self, FaceMetrics};
use crate::text::shape;

/// DrawingML's natural line box is `1.2 ×` the line's effective size
/// (`powerpoint-line-metrics.ts:305`). A math segment contributes the size its
/// own visual box needs, but that size is compared in the same 1.2 em unit:
/// `effSize = max(runSize, (ascent + descent) / 1.2)`
/// (`renderer.ts:5285-5286`).
const NATURAL_LINE_SCALE: f32 = 1.2;

/// `powerPointNaturalLine` over the line's runs, adjusted for any equation the
/// line carries.
///
/// A line with an equation fails the reference's face-metric gate, so its box
/// is the synthetic 0.8/0.2 split of `1.2 × maxSizePx`; a math segment claims
/// `(ascent + descent) / 1.2` of that size (`renderer.ts:5285-5298`). A
/// fractions-heavy `m:oMathPara` therefore still grows the line, while a short
/// display equation keeps the ordinary 1.2 em pitch.
pub(super) fn natural_box(
    measurer: &Measurer,
    line: &BrokenLine,
    para: &ParagraphLayout,
    pieces: &[&MathPiece],
) -> (f32, f32) {
    let mut sizes: Vec<(f32, FaceMetrics)> = Vec::new();
    for item in &line.items {
        let style = match item {
            LineItem::Text { style, .. } => *style,
            LineItem::Tab { style, .. } => *style,
            LineItem::Object { style, .. } => *style,
        };
        let def = measurer.style(style);
        // Always the authored size: a baseline-shifted run is painted at 65 %
        // but still owns its full 1.2 em line box (`renderer.ts:1549-1553`).
        sizes.push((
            def.size_emu,
            FaceMetrics {
                ascent_share: shape::ascent_share_for(&def.family, def.bold, def.italic),
            },
        ));
    }
    if sizes.is_empty() {
        let def = measurer.style(para.default_style);
        sizes.push((
            def.size_emu,
            FaceMetrics {
                ascent_share: shape::ascent_share_for(&def.family, def.bold, def.italic),
            },
        ));
    }
    let natural = metrics::natural_line_box(&sizes);
    if pieces.is_empty() {
        return natural;
    }

    // A line carrying an equation fails the reference's face-metric gate
    // (`renderer.ts:5289` `metricOk = false`), so its natural box is the
    // synthetic 0.8/0.2 split of `1.2 × maxSizePx`, where the math segment
    // claims `(ascent + descent) / 1.2` of that size. Before this port the
    // box was widened directly to `1.4 × (ascent + descent)`, which made a
    // short display equation (a single 20 pt `x`) pitch at 1.1876 em instead
    // of the reference's 1.2 em.
    let mut max_size = sizes.iter().map(|(size, _)| *size).fold(0.0_f32, f32::max);
    for piece in pieces {
        let eff_size = (piece.ascent + piece.descent) / NATURAL_LINE_SCALE;
        max_size = max_size.max(eff_size);
    }
    let height = NATURAL_LINE_SCALE * max_size;
    (
        height * metrics::SYNTHETIC_ASCENT_EM,
        height * metrics::SYNTHETIC_DESCENT_EM,
    )
}
