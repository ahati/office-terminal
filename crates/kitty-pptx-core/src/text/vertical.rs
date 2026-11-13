//! Vertical and stacked text frames (`a:bodyPr@vert`, ECMA-376 §20.1.10.83).
//!
//! Reference: `/tmp/oooxml/packages/pptx/src/renderer.ts` `renderTextBody`
//! (5021-5083: dispatch, rotated sub-frame) and
//! `packages/core/src/drawingml-text/stacked.ts` (`wordArtVert` /
//! `wordArtVertRtl` columns).
//!
//! The rotated modes are a *frame* transform: the body is laid out exactly as
//! horizontal text inside the box with its width and height swapped, and the
//! resulting `DrawText` transform is post-multiplied by a rotation about the
//! box centre. Fonts, wrapping, spacing and alignment therefore need no
//! vertical-specific code path.
//!
//! Approximations are documented per item:
//! - `eaVert` re-enters the reference with `eaVertUpright = true`, which stands
//!   CJK glyphs upright and rotates only the Latin ones (UAX#50). Here every
//!   glyph rotates with the frame; per-glyph upright substitution is not
//!   implemented.
//! - `mongolianVert` is rendered horizontally by the reference (it only tests
//!   `vert`/`eaVert` at `renderer.ts:5034`). ST_TextVerticalType still means
//!   top-to-bottom text, so it is treated like `vert` here.
//! - Stacked text is the reference's one-grapheme-per-column reduction: the
//!   full `layoutStackedText` also balances multi-glyph columns, applies
//!   `a:lnSpc` per column and seeds `spcBef`/`spcAft` gaps, none of which can
//!   change a single-glyph column.

use crate::contracts::{AffineTransform, RectF32};
use crate::text::measure::StyleId;

/// How `a:bodyPr@vert` reshapes the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VertMode {
    /// `horz` and every value the reference draws horizontally.
    Horz,
    /// `vert` / `eaVert` / `vert270` / `mongolianVert`: swapped layout box plus
    /// a frame rotation. `angle_deg` is the canvas rotation of
    /// `renderer.ts:5042` (`vertRot`): positive = clockwise in the y-down
    /// layout frame, i.e. `kx = -sin`, `ky = sin` like
    /// `compiler::transform::compute_shape_transform`.
    Rotated { angle_deg: f32 },
    /// `wordArtVert` (`rtl == false`) / `wordArtVertRtl`: one grapheme per
    /// column, glyphs upright.
    Stacked { rtl: bool },
}

impl VertMode {
    /// `a:bodyPr@vert` → mode (`renderer.ts:5021-5035`).
    pub fn of(vert: &str) -> Self {
        match vert {
            // ref renderer.ts:5034 `isVert = vert === 'vert' || vert === 'eaVert'`
            // (plus `mongolianVert`, see the module docs).
            "vert" | "eaVert" | "mongolianVert" => VertMode::Rotated { angle_deg: 90.0 },
            // ref renderer.ts:5035 `isVert270`: `vertRot = -90`.
            "vert270" => VertMode::Rotated { angle_deg: -90.0 },
            // ref renderer.ts:5021-5028 stacked WordArt.
            "wordArtVert" => VertMode::Stacked { rtl: false },
            "wordArtVertRtl" => VertMode::Stacked { rtl: true },
            _ => VertMode::Horz,
        }
    }
}

/// Layout extents plus the frame rotation to post-multiply onto the element
/// transform. The rotated modes lay out in the box with `w` and `h` swapped
/// (`renderer.ts:5073-5083`); every other mode keeps the authored box.
pub fn layout_frame(mode: VertMode, box_w: i64, box_h: i64) -> (f32, f32, Option<AffineTransform>) {
    match mode {
        VertMode::Rotated { angle_deg } => (
            box_h as f32,
            box_w as f32,
            Some(frame_transform(angle_deg, box_w as f32, box_h as f32)),
        ),
        _ => (box_w as f32, box_h as f32, None),
    }
}

/// The rotated sub-frame of `renderer.ts:5070-5083`:
/// `ctx.translate(cx, cy); ctx.rotate(angle)` with the recursion entered at
/// `(-bh/2, -bw/2)`.
///
/// A layout point `(px, py)` (origin at the top-left of the swapped box, so
/// `px ∈ [0, box_h]`, `py ∈ [0, box_w]`) maps to box-local
/// `centre + R(angle) · (px − box_h/2, py − box_w/2)`. Written as a product of
/// affines: `T(centre) · R(angle) · T(−box_h/2, −box_w/2)`.
pub fn frame_transform(angle_deg: f32, box_w: f32, box_h: f32) -> AffineTransform {
    let rad = angle_deg.to_radians();
    let (sin, cos) = (rad.sin(), rad.cos());
    let rotation = AffineTransform {
        sx: cos,
        ky: sin,
        kx: -sin,
        sy: cos,
        tx: 0.0,
        ty: 0.0,
    };
    AffineTransform::translate(box_w * 0.5, box_h * 0.5)
        .post_concat(&rotation)
        .post_concat(&AffineTransform::translate(-box_h * 0.5, -box_w * 0.5))
}

/// One grapheme measured by the host, before column placement.
#[derive(Clone, Debug)]
pub struct StackedGlyph {
    pub text: String,
    pub style: StyleId,
    /// Advance along the column (an upright glyph's width).
    pub advance: f32,
    /// Column thickness the glyph claims (its face cell).
    pub thickness: f32,
    pub ascent: f32,
    pub descent: f32,
}

/// A placed stacked glyph: left edge of the glyph box and its baseline.
#[derive(Clone, Debug)]
pub struct StackedSlot {
    pub text: String,
    pub style: StyleId,
    pub x: f32,
    pub baseline: f32,
}

/// Column placement of `layoutStackedText` (`stacked.ts:137-260`) reduced to
/// one glyph per column.
///
/// `rect` is the text rectangle after insets. `anchor` places the column block
/// across the columns (`t` = the start side: left for `wordArtVert`, right for
/// `wordArtVertRtl`; `b` = the end side; `ctr` = centred) and `anchor_ctr`
/// centres the block's longest column instead (`stacked.ts` "anchorCtr centres
/// the block's longest column"). `alignment` places the glyph along its column:
/// `l` = top, `ctr` = centre, `r`/the fill alignments = bottom.
pub fn place_stacked(
    glyphs: &[StackedGlyph],
    rect: RectF32,
    anchor: &str,
    alignment: &str,
    anchor_ctr: bool,
    rtl: bool,
) -> Vec<StackedSlot> {
    let total: f32 = glyphs.iter().map(|g| g.thickness).sum();
    let block_left = if anchor_ctr {
        rect.x + (rect.w - total) * 0.5
    } else {
        match anchor {
            "b" => rect.right() - total,
            "ctr" => rect.x + (rect.w - total) * 0.5,
            _ => rect.x,
        }
    };
    let mut out = Vec::with_capacity(glyphs.len());
    let mut offset = 0.0f32;
    for glyph in glyphs {
        // `wordArtVertRtl` stacks the same reading-order items right to left.
        let left = if rtl {
            block_left + total - offset - glyph.thickness
        } else {
            block_left + offset
        };
        let cell = glyph.ascent + glyph.descent;
        let top = match alignment {
            "ctr" => rect.y + (rect.h - cell) * 0.5,
            "r" | "just" | "justLow" | "dist" | "thaiDist" => rect.y + rect.h - cell,
            _ => rect.y,
        };
        out.push(StackedSlot {
            text: glyph.text.clone(),
            style: glyph.style,
            // The glyph cell is centred on the column axis.
            x: left + (glyph.thickness - glyph.advance) * 0.5,
            baseline: top + glyph.ascent,
        });
        offset += glyph.thickness;
    }
    out
}

/// Grapheme clusters, approximated: a base code point plus trailing combining
/// marks, variation selectors and ZWJ continuations, and paired regional
/// indicators. UAX#29's full `Grapheme_Cluster_Break` tables would need a new
/// dependency; the clusters that matter for stacked text (Latin base + mark,
/// CJK, emoji sequences, flags) are covered.
pub fn graphemes(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut prev: Option<char> = None;
    let mut regional_run = 0usize;
    for (index, ch) in text.char_indices() {
        let extends = match prev {
            None => false,
            Some(previous) => {
                is_extend(ch)
                    || previous == '\u{200d}'
                    || (is_regional_indicator(ch)
                        && is_regional_indicator(previous)
                        && regional_run % 2 == 1)
            }
        };
        if !extends && prev.is_some() {
            out.push(&text[start..index]);
            start = index;
        }
        prev = Some(ch);
        if is_regional_indicator(ch) {
            regional_run += 1;
        } else {
            regional_run = 0;
        }
    }
    if prev.is_some() {
        out.push(&text[start..]);
    }
    out
}

/// Combining marks, variation selectors and skin-tone modifiers.
fn is_extend(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036f
            | 0x0483..=0x0489
            | 0x0591..=0x05c7
            | 0x0610..=0x061a
            | 0x064b..=0x065f
            | 0x0670
            | 0x06d6..=0x06dc
            | 0x06df..=0x06e4
            | 0x0e31
            | 0x0e34..=0x0e3a
            | 0x0e47..=0x0e4e
            | 0x0eb1
            | 0x0eb4..=0x0ebc
            | 0x0ec8..=0x0ece
            | 0x17b4..=0x17d3
            | 0x17dd
            | 0x1ab0..=0x1aff
            | 0x1dc0..=0x1dff
            | 0x20d0..=0x20ff
            | 0xfe00..=0xfe0f
            | 0xfe20..=0xfe2f
            | 0x1f3fb..=0x1f3ff
            | 0xe0100..=0xe01ef
    )
}

/// U+1F1E6..=U+1F1FF: a flag is a pair of regional indicators.
fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1f1e6..=0x1f1ff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(mode: VertMode, w: f32, h: f32) -> AffineTransform {
        let (_, _, frame) = layout_frame(mode, w as i64, h as i64);
        frame.unwrap_or_else(|| panic!("expected a rotated frame"))
    }

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    /// ref renderer.ts:5034-5042: `vert` rotates +90 (canvas), `vert270` -90.
    #[test]
    fn mode_mapping_matches_the_reference() {
        assert_eq!(VertMode::of("horz"), VertMode::Horz);
        assert_eq!(VertMode::of(""), VertMode::Horz);
        assert_eq!(VertMode::of("vert"), VertMode::Rotated { angle_deg: 90.0 });
        assert_eq!(
            VertMode::of("eaVert"),
            VertMode::Rotated { angle_deg: 90.0 }
        );
        assert_eq!(
            VertMode::of("vert270"),
            VertMode::Rotated { angle_deg: -90.0 }
        );
        assert_eq!(
            VertMode::of("wordArtVert"),
            VertMode::Stacked { rtl: false }
        );
        assert_eq!(
            VertMode::of("wordArtVertRtl"),
            VertMode::Stacked { rtl: true }
        );
    }

    /// `vert` (canvas +90, i.e. -90 in the y-up convention) sends the layout's
    /// top-left corner to the box's top-right and the layout's +x axis down.
    #[test]
    fn vert_frame_maps_the_swapped_box_onto_the_authored_box() {
        let t = placed(VertMode::of("vert"), 200.0, 100.0);
        let (x, y) = t.transform_point(0.0, 0.0);
        close(x, 200.0);
        close(y, 0.0);
        // Layout +x (line flow) runs down the page.
        let (dx, dy) = t.transform_point(10.0, 0.0);
        close(dx, 200.0);
        close(dy, 10.0);
        // Layout +y (line stacking) runs leftwards.
        let (sx, sy) = t.transform_point(0.0, 10.0);
        close(sx, 190.0);
        close(sy, 0.0);
        // The swapped layout centre lands on the box centre.
        let (cx, cy) = t.transform_point(50.0, 100.0);
        close(cx, 100.0);
        close(cy, 50.0);
    }

    /// `vert270` is the mirror image: the layout starts at the bottom-left.
    #[test]
    fn vert270_frame_starts_at_the_bottom_left() {
        let t = placed(VertMode::of("vert270"), 200.0, 100.0);
        let (x, y) = t.transform_point(0.0, 0.0);
        close(x, 0.0);
        close(y, 100.0);
        let (dx, dy) = t.transform_point(10.0, 0.0);
        close(dx, 0.0);
        close(dy, 90.0);
    }

    #[test]
    fn graphemes_keep_combining_marks_and_flags_together() {
        assert_eq!(graphemes("abc"), vec!["a", "b", "c"]);
        assert_eq!(graphemes("e\u{301}x"), vec!["e\u{301}", "x"]);
        assert_eq!(graphemes("日本"), vec!["日", "本"]);
        assert_eq!(graphemes("\u{1f1e9}\u{1f1ea}\u{1f1eb}").len(), 2);
        assert!(graphemes("").is_empty());
    }

    /// One grapheme per column; `wordArtVertRtl` mirrors the columns.
    #[test]
    fn stacked_columns_follow_the_direction() {
        let glyphs: Vec<StackedGlyph> = ["A", "B", "C"]
            .iter()
            .map(|text| StackedGlyph {
                text: (*text).to_string(),
                style: StyleId(0),
                advance: 8.0,
                thickness: 10.0,
                ascent: 8.0,
                descent: 2.0,
            })
            .collect();
        let rect = RectF32::new(0.0, 0.0, 40.0, 100.0);
        let ltr = place_stacked(&glyphs, rect, "t", "ctr", false, false);
        assert_eq!(ltr.len(), 3);
        close(ltr[0].x, 1.0);
        close(ltr[1].x, 11.0);
        close(ltr[2].x, 21.0);
        // Centred along the column: cell 10 in a 100 high box.
        close(ltr[0].baseline, 45.0 + 8.0);

        let rtl = place_stacked(&glyphs, rect, "t", "ctr", false, true);
        close(rtl[0].x, 21.0);
        close(rtl[2].x, 1.0);
        // `anchorCtr` centres the block instead of pinning it to `anchor`.
        let centred = place_stacked(&glyphs, rect, "t", "ctr", true, false);
        close(centred[0].x, 6.0);
    }
}
