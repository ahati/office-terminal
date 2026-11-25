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
//! box centre, so fonts, wrapping, spacing and alignment need no vertical code
//! path. `eaVert` re-enters with `eaVertUpright = true` and uprights the
//! East-Asian glyphs (UAX#50): [`EaVertFrame`] and [`ea_vert_pieces`].
//!
//! Approximations are documented per item:
//! - `mongolianVert` only tests `vert`/`eaVert` at `renderer.ts:5034`, so it is
//!   treated like `vert` here.
//! - Stacked text is the reference's one-grapheme-per-column reduction: the
//!   full `layoutStackedText` also balances multi-glyph columns, applies
//!   `a:lnSpc` per column and seeds `spcBef`/`spcAft` gaps, none of which can
//!   change a single-glyph column.

use crate::contracts::AffineTransform;
use crate::text::linebreak::rules::is_cjk_break_char;

/// How `a:bodyPr@vert` reshapes the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VertMode {
    /// `horz` and every value the reference draws horizontally.
    Horz,
    /// `vert` / `vert270` / `mongolianVert`: swapped layout box plus a frame
    /// rotation. `angle_deg` is the canvas rotation of `renderer.ts:5042`
    /// (`vertRot`): positive = clockwise in the y-down layout frame, i.e.
    /// `kx = -sin`, `ky = sin` like
    /// `compiler::transform::compute_shape_transform`.
    Rotated { angle_deg: f32 },
    /// `eaVert`: the same +90° frame, re-entered with
    /// `eaVertUpright = true` (`renderer.ts:5034`, `5083`), so East-Asian
    /// glyphs stand upright while Latin ones rotate with the frame
    /// (UAX#50). [`EaVertFrame`] carries the per-glyph uprighting.
    EaVert,
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
            "vert" | "mongolianVert" => VertMode::Rotated { angle_deg: 90.0 },
            // ref renderer.ts:5083: `eaVert` re-enters with uprighting.
            "eaVert" => VertMode::EaVert,
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
        VertMode::Rotated { angle_deg } => rotated_frame(angle_deg, box_w, box_h),
        // `eaVert` shares `vert`'s swapped box and +90° frame (`renderer.ts:5034`).
        VertMode::EaVert => rotated_frame(90.0, box_w, box_h),
        _ => (box_w as f32, box_h as f32, None),
    }
}

/// The swapped box and frame of the rotated modes (`renderer.ts:5073-5083`).
fn rotated_frame(angle_deg: f32, box_w: i64, box_h: i64) -> (f32, f32, Option<AffineTransform>) {
    (
        box_h as f32,
        box_w as f32,
        Some(frame_transform(angle_deg, box_w as f32, box_h as f32)),
    )
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

// ── `eaVert` per-glyph orientation (UAX #50, `vertical-text.ts`) ─────────────

/// UAX #50 `Vertical_Orientation` reduced to the two draw behaviours
/// `drawEaVertRunWithCapability` distinguishes (`vertical-text.ts:90-170`):
/// stand the glyph up, or leave it lying sideways with the rotated frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orient {
    /// `vo=U` / `vo=Tu` / a substituted `vo=Tr` form: drawn with the −90°
    /// counter-rotation that cancels the frame (`vertical-text.ts:143-151`).
    Upright,
    /// `vo=R`: drawn with the frame, i.e. rotated 90° clockwise on the slide
    /// (`vertical-text.ts:160-169`).
    Rotated,
}

/// `ー` U+30FC, `〜` U+301C, `～` U+FF5E: the `vo=Tr` long marks. Their UAX #50
/// fallback is to rotate with the frame (`vertical-text.ts:143-160`,
/// `vertical-orientation.ts` `VERTICAL_TR_LONG_MARKS`); a face that does carry
/// the FE31/FE32 vertical form draws the substituted glyph upright instead
/// ([`vertical_form`]).
fn rotates_with_the_frame(cp: u32) -> bool {
    // The Tr fallback set whose FE13/FE14 forms were dropped: long marks,
    // wave dashes and the fullwidth colon. The semicolon stays upright.
    matches!(cp, 0x30fc | 0x301c | 0xff5e | 0xff1a)
}

/// The U+FE10–FE19 / U+FE30–FE4F vertical presentation forms are by definition
/// already vertical, so an authored one stays upright.
fn is_vertical_presentation(cp: u32) -> bool {
    matches!(cp, 0xfe10..=0xfe19 | 0xfe30..=0xfe4f)
}

/// UAX #50 `Vertical_Orientation` of one code point (`vertical-text.ts:96`).
///
/// `vo=U` is the ideographic / kana / Hangul / fullwidth set the line breaker
/// already classifies (`cjk-ranges.ts:57-64`, reused through
/// [`is_cjk_break_char`] so the two engines share one table); everything else —
/// Latin, digits, ASCII punctuation, arrows, symbols — is `vo=R`. Punctuation
/// inside the ideographic ranges (`、。「」（`, `！？`) is upright here, with the
/// Tr long marks rotated as noted above; the reference resolves the same two
/// draw classes from the full generated UCD table.
///
/// Approximated: the supplementary-plane ideographs (U+20000–U+3FFFD) are
/// outside `is_cjk_break_char` and therefore rotate; the reference stands them
/// up.
pub fn orientation(ch: char) -> Orient {
    let cp = ch as u32;
    if is_vertical_presentation(cp) || (is_cjk_break_char(cp) && !rotates_with_the_frame(cp)) {
        Orient::Upright
    } else {
        Orient::Rotated
    }
}

/// The U+FE1x / U+FE3x vertical presentation form of a horizontal glyph, or
/// `None` (`vertical-orientation.ts`: `verticalFormSubstitute` for `vo=Tu`,
/// `verticalBracketFormSubstitute` for `vo=Tr`).
///
/// The Tu part is the corner-hanging punctuation; the Tr part is the fullwidth
/// bracket family, including the white lenticular `〖〗` whose FE17/FE18 form
/// the reference's substitute faces do contain. `！`/`？` are deliberately absent
/// (they are already the correct vertical shape, #771), as are the fullwidth
/// colon/semicolon whose FE13/FE14 forms are missing from most faces (#969) and
/// which therefore take the geometric fallback instead.
pub fn vertical_form(ch: char) -> Option<char> {
    Some(match ch {
        // vo=Tu, corner-hanging (`VERTICAL_FORM_MAP`): ， 、 。
        '\u{ff0c}' => '\u{fe10}',
        '\u{3001}' => '\u{fe11}',
        '\u{3002}' => '\u{fe12}',
        // vo=Tr brackets (`VERTICAL_BRACKET_FORM_MAP`): （）｛｝〔〕【】《》〈〉「」『』〖〗
        '\u{ff08}' => '\u{fe35}',
        '\u{ff09}' => '\u{fe36}',
        '\u{ff5b}' => '\u{fe37}',
        '\u{ff5d}' => '\u{fe38}',
        '\u{3014}' => '\u{fe39}',
        '\u{3015}' => '\u{fe3a}',
        '\u{3010}' => '\u{fe3b}',
        '\u{3011}' => '\u{fe3c}',
        '\u{300a}' => '\u{fe3d}',
        '\u{300b}' => '\u{fe3e}',
        '\u{3008}' => '\u{fe3f}',
        '\u{3009}' => '\u{fe40}',
        '\u{300c}' => '\u{fe41}',
        '\u{300d}' => '\u{fe42}',
        '\u{300e}' => '\u{fe43}',
        '\u{300f}' => '\u{fe44}',
        '\u{3016}' => '\u{fe17}',
        '\u{3017}' => '\u{fe18}',
        // The Tr long marks: `ー`/`〜` are the horizontal marks whose vertical
        // form IS the U+FE3x rotation (FE31/FE32), so a face that has them
        // draws the substituted glyph upright instead of rotating the original.
        '\u{30fc}' => '\u{fe31}',
        '\u{301c}' => '\u{fe32}',
        _ => return None,
    })
}

/// One paint piece of an `eaVert` run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EaVertPiece {
    /// Stand the glyph up (UAX #50 `vo=U` / `vo=Tu`, or a substituted form).
    pub upright: bool,
    /// The glyph text to paint: the substituted vertical form when the face
    /// resolves one, else the authored cluster.
    pub painted: String,
    /// The authored cluster. Its advance sets the cell width: the reference
    /// measures the ORIGINAL code point and substitutes glyph-only
    /// (`vertical-text.ts:118-121`, measure == draw).
    pub measured: String,
}

fn is_tr_long_mark(ch: char) -> bool {
    matches!(ch, '\u{30fc}' | '\u{301c}' | '\u{ff5e}')
}

/// Split a run into `eaVert` paint pieces (`drawEaVertRunWithCapability`,
/// `vertical-text.ts:96-170`).
///
/// Each upright cluster is its own piece (the counter-rotated cell is drawn
/// alone); consecutive sideways clusters stay one rotated group. `has_glyph`
/// probes font-feature coverage for long marks or glyph presence for vertical
/// presentation forms (`vertical-text.ts:118-135`).
pub fn ea_vert_pieces(text: &str, has_glyph: impl Fn(char) -> bool) -> Vec<EaVertPiece> {
    let mut out: Vec<EaVertPiece> = Vec::new();
    let mut sideways: Option<(String, String)> = None;
    for cluster in graphemes(text) {
        let mut chars = cluster.chars();
        let first = chars.next();
        let is_single = first.is_some() && chars.next().is_none();
        let (upright, painted) = if let (true, Some(ch)) = (is_single, first) {
            if is_tr_long_mark(ch) {
                (has_glyph(ch), cluster.to_string())
            } else if ch == '\u{ff1b}' {
                (true, cluster.to_string())
            } else if ch == '\u{ff1a}' {
                (false, cluster.to_string())
            } else if let Some(form) = vertical_form(ch).filter(|f| has_glyph(*f)) {
                (true, form.to_string())
            } else if orientation(ch) == Orient::Upright {
                (true, cluster.to_string())
            } else {
                (false, cluster.to_string())
            }
        } else {
            let upright = first.is_some_and(|ch| orientation(ch) == Orient::Upright);
            (upright, cluster.to_string())
        };

        if upright {
            if let Some((painted, measured)) = sideways.take() {
                out.push(EaVertPiece {
                    upright: false,
                    painted,
                    measured,
                });
            }
            out.push(EaVertPiece {
                upright: true,
                painted,
                measured: cluster.to_string(),
            });
            continue;
        }
        match &mut sideways {
            Some((group, source)) => {
                group.push_str(&painted);
                source.push_str(cluster);
            }
            None => sideways = Some((painted, cluster.to_string())),
        }
    }
    if let Some((painted, measured)) = sideways {
        out.push(EaVertPiece {
            upright: false,
            painted,
            measured,
        });
    }
    out
}

/// The `eaVert` sub-frame plus the element transform it is entered with
/// (`renderer.ts:5034-5083`). An upright cell is placed OUTSIDE the frame — its
/// baseline runs along the slide axes — so its glyph coordinates are pre-mapped
/// through the frame and painted with the element transform alone
/// (`vertical-text.ts:143-151` counter-rotates the glyph about the cell centre,
/// which lands on the same point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EaVertFrame {
    /// The element transform the body is entered with (`base_transform`).
    pub base: AffineTransform,
    /// The +90° sub-frame (`frame_transform`).
    pub frame: AffineTransform,
}

impl EaVertFrame {
    /// Baseline origin in frame-local slide EMU of an upright cell whose em box
    /// is centred at layout `(centre_x, baseline_y − centre_above)`.
    ///
    /// `centre_x` is the cell centre along the column, `baseline_y` the line's
    /// alphabetic baseline (layout y) and `centre_above` the em-box centre above
    /// that baseline ([`em_centre_above`]); `advance` is the cell width.
    pub fn upright_origin(&self, centre_x: f32, baseline_y: f32, advance: f32, centre_above: f32) -> (f32, f32) {
        let (x, y) = self.frame.transform_point(centre_x, baseline_y - centre_above);
        (x - advance * 0.5, y + centre_above)
    }
}

/// The per-glyph uprighting frame for `eaVert`, `None` for every other mode
/// (`renderer.ts:5083` passes `eaVertUpright` only for `eaVert`).
pub fn ea_vert_frame(mode: VertMode, box_w: i64, box_h: i64, base: &AffineTransform) -> Option<EaVertFrame> {
    if mode != VertMode::EaVert {
        return None;
    }
    let (_, _, frame) = layout_frame(mode, box_w, box_h);
    frame.map(|frame| EaVertFrame { base: *base, frame })
}

/// Em-box centre above the baseline for the 1.2-em box [`crate::text::metrics`]
/// measures: `ascent − box/2` (`natural_line_box`). This is the cross-axis
/// anchor an upright cell centres on (`vertical-text.ts:127-131`, which reads
/// the face's font bounding box instead).
pub fn em_centre_above(size_emu: f32, ascent_share: f32) -> f32 {
    1.2 * size_emu * (ascent_share - 0.5)
}

/// Text-frame rotation for a stacked body under the shape's flips.
///
/// A stacked body is already upright in its own frame; a horizontal flip keeps
/// `wordArtVert` upright, while reversing a `wordArtVertRtl` column frame turns
/// the body 180°. Horizontal bodies are unaffected. ref
/// `stacked-text.test.ts:207` (`shapeTextRotation`).
mod stacked_layout;
pub use stacked_layout::{place_stacked, StackedGlyph, StackedSlot};

/// Text-frame rotation for a stacked body under the shape's flips.
pub fn shape_text_rotation(vert: &str, rotation: f32, flip_h: bool, flip_v: bool) -> f32 {
    if vert == "wordArtVertRtl" && flip_h != flip_v {
        (rotation + 180.0).rem_euclid(360.0)
    } else {
        rotation.rem_euclid(360.0)
    }
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
                    || (is_regional_indicator(ch) && is_regional_indicator(previous) && regional_run % 2 == 1)
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
    use crate::contracts::RectF32;
    use crate::text::measure::StyleId;

    fn placed(mode: VertMode, w: f32, h: f32) -> AffineTransform {
        let (_, _, frame) = layout_frame(mode, w as i64, h as i64);
        frame.unwrap_or_else(|| panic!("expected a rotated frame"))
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.01, "expected {expected}, got {actual}");
    }

    /// ref renderer.ts:5034-5042: `vert` rotates +90 (canvas), `vert270` -90.
    #[test]
    fn mode_mapping_matches_the_reference() {
        assert_eq!(VertMode::of("horz"), VertMode::Horz);
        assert_eq!(VertMode::of(""), VertMode::Horz);
        assert_eq!(VertMode::of("vert"), VertMode::Rotated { angle_deg: 90.0 });
        assert_eq!(VertMode::of("eaVert"), VertMode::EaVert);
        assert_eq!(VertMode::of("vert270"), VertMode::Rotated { angle_deg: -90.0 });
        assert_eq!(VertMode::of("wordArtVert"), VertMode::Stacked { rtl: false });
        assert_eq!(VertMode::of("wordArtVertRtl"), VertMode::Stacked { rtl: true });
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

    /// Glyphs stack down a column; overflow wraps to the next column and
    /// `wordArtVertRtl` mirrors the columns.
    #[test]
    fn stacked_columns_wrap_and_follow_the_direction() {
        let glyphs: Vec<StackedGlyph> = ["A", "B", "C"]
            .iter()
            .map(|text| StackedGlyph {
                text: (*text).to_string(),
                style: StyleId(0),
                advance: 8.0,
                width: 8.0,
                thickness: 10.0,
                ascent: 8.0,
                descent: 2.0,
                rotation: 0.0,
                object: None,
                paragraph: 0,
                break_before: false,
            })
            .collect();
        // One cell per column: each 8 px glyph fills the 10 px column and the
        // next glyph wraps.
        let rect = RectF32::new(0.0, 0.0, 40.0, 10.0);
        let ltr = place_stacked(&glyphs, rect, "t", "ctr", false, false);
        assert_eq!(ltr.len(), 3);
        close(ltr[0].x, 1.0);
        close(ltr[1].x, 11.0);
        close(ltr[2].x, 21.0);
        // Centred along the column: cell 8 in a 10 high box.
        close(ltr[0].baseline, 1.0 + 8.0);

        let rtl = place_stacked(&glyphs, rect, "t", "ctr", false, true);
        close(rtl[0].x, 21.0);
        close(rtl[2].x, 1.0);
        // `anchorCtr` centres the block instead of pinning it to `anchor`.
        let centred = place_stacked(&glyphs, rect, "t", "ctr", true, false);
        close(centred[0].x, 6.0);
    }
}
