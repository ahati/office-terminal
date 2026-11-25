//! List-marker painting and run highlights.
//!
//! Both paint a background/fill item in the paragraph's gutter or under the
//! glyphs, so they live together outside the layout half of `text::paint`:
//!
//! - `<a:buChar>` / `<a:buAutoNum>` / `<a:buBlip>` draw site
//!   (`renderer.ts:5838-5877`)
//! - `paintHighlight` (`renderer.ts:764-785`), whose marker band is
//!   `highlightBox` (`packages/core/src/text/highlight-box.ts:26-32`)

use super::emit::Measurer;
use super::paint;
use super::paragraphs::{self, MarkerPlan, ParagraphLayout};
use crate::contracts::{
    AffineTransform, FillRule, NormalizedRect, PathSegment, PositionedGlyphRun, RectF32, VectorCommand, VectorPaint,
    VectorPath,
};
use crate::text::bullets::MarkerBitmap;

/// `highlightBox` (`packages/core/src/text/highlight-box.ts:26-32`): the marker
/// band starts `0.85 em` above the baseline and is `1.1 em` tall. ECMA-376
/// fixes no geometry for §21.1.2.3.4; these are the shared library band, used
/// whenever the face's #1610 usWin box is unavailable.
const HIGHLIGHT_TOP_EM: f32 = 0.85;
const HIGHLIGHT_HEIGHT_EM: f32 = 1.1;

/// Paint the paragraph's list marker in its gutter (`renderer.ts:5838-5877`).
///
/// A glyph marker is one run on the line baseline at `marker_x`; a picture
/// marker is a `DrawImage` whose height is the text-derived `buSz*` size and
/// whose width keeps the decoded bitmap's aspect ratio, bottom-aligned to the
/// baseline. An undecoded bitmap paints nothing (`renderer.ts:5812`).
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_marker(
    measurer: &Measurer,
    para: &ParagraphLayout,
    marker_x: f32,
    baseline_y: f32,
    transform: &AffineTransform,
    bitmaps: Option<&dyn MarkerBitmap>,
    out: &mut Vec<PositionedGlyphRun>,
    commands: &mut Vec<VectorCommand>,
) {
    match para.marker.as_ref() {
        Some(MarkerPlan::Label { text, style }) => {
            paint::emit_run(measurer, text, *style, marker_x, baseline_y, out);
        }
        Some(MarkerPlan::Image { rel_id, size_emu }) => {
            push_marker_image(rel_id, *size_emu, marker_x, baseline_y, transform, bitmaps, commands)
        }
        None => {}
    }
}

/// One picture marker: `h = bulletImage.sizePx`, `w = h × bitmap aspect`,
/// `imgY = baseline - h` (`renderer.ts:5858-5877`). `x` is the marker's left
/// edge; the stacked path centres it on its column axis instead.
pub(super) fn push_marker_image(
    rel_id: &str,
    height_emu: f32,
    x: f32,
    baseline_y: f32,
    transform: &AffineTransform,
    bitmaps: Option<&dyn MarkerBitmap>,
    commands: &mut Vec<VectorCommand>,
) {
    let Some(width) = paragraphs::picture_width(rel_id, height_emu, bitmaps) else {
        return;
    };
    if width <= 0.0 || height_emu <= 0.0 {
        return;
    }
    let dest = RectF32::new(x, baseline_y - height_emu, width, height_emu);
    commands.push(VectorCommand::draw_image(
        rel_id.to_string(),
        NormalizedRect::default(),
        dest,
        1.0,
        *transform,
    ));
}

/// `paintHighlight` (`renderer.ts:764-785`): fill the marker box in the
/// highlight colour before the glyphs are drawn, so the glyphs (and their
/// decorations) sit on top.
pub(super) fn push_highlight(
    commands: &mut Vec<VectorCommand>,
    x: f32,
    baseline: f32,
    width: f32,
    size_emu: f32,
    color: [u8; 4],
    transform: &AffineTransform,
) {
    if width <= 0.0 || size_emu <= 0.0 {
        return;
    }
    let top = baseline - size_emu * HIGHLIGHT_TOP_EM;
    let height = size_emu * HIGHLIGHT_HEIGHT_EM;
    commands.push(VectorCommand::draw_path(
        rect_path(x, top, width, height),
        Some(VectorPaint::Solid(color)),
        None,
        *transform,
    ));
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> VectorPath {
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
