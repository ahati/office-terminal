//! PowerPoint / DrawingML line metrics (parity port).
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts` — natural line box, whole
//!   point `spcPts` rounding (#1610 ascent-share model).
//! - `packages/core/src/drawingml-text/metrics.ts` — `drawingMlLineHeight`,
//!   `drawingMlSpacedLineBox`, `drawingMlParagraphSpacing`,
//!   `drawingMlBlockTop`.
//! - `packages/core/src/drawingml-text/align.ts` — `drawingMlLineX`,
//!   `drawingMlLineShouldJustify`.
//! - `packages/pptx/src/renderer.ts:5351-5476` — how a line derives its
//!   natural box, its percentage spacing unit, and its `spcFirstLastPara`
//!   edges.
//!
//! Everything here is pure `f32` arithmetic: only IEEE-754 basic operations
//! (`+ - * /`, `min`/`max`, `floor`) are used, so the kernels are
//! bit-identical on every platform. The reference runs on `f64`; every value
//! that reaches layout is quantised afterwards (whole points, or the export's
//! 1/100 in = 0.72 pt unit), which is many orders of magnitude above the f32
//! ulp, so the difference is never observable.

/// `SYNTHETIC_ASCENT_EM` — `core/src/text/synthetic-em-box.ts:15`.
pub const SYNTHETIC_ASCENT_EM: f32 = 0.8;

/// `SYNTHETIC_DESCENT_EM` — `core/src/text/synthetic-em-box.ts:16`.
pub const SYNTHETIC_DESCENT_EM: f32 = 0.2;

/// One run's face contribution to a line: PowerPoint's #1610 ascent share.
///
/// `usWinAscent / (usWinAscent + usWinDescent)`, or, for a face whose
/// `fsSelection` sets `USE_TYPO_METRICS`, `(sTypoAscender + sTypoLineGap) /
/// (sTypoAscender − sTypoDescender + sTypoLineGap)` —
/// ref `powerpoint-line-metrics.ts:115-134`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    pub ascent_share: f32,
}

impl FaceMetrics {
    /// The split of a run's **drawn** face.
    ///
    /// PowerPoint uses the face it actually paints with (the latin slot of the
    /// script segment, `renderer.ts:1928-1938`), read from the pinned catalogue
    /// ([`crate::text::faces`]); an uncatalogued face falls back to the
    /// installed-face probe in [`crate::text::shape::ascent_share_for`].
    pub fn for_family(family: &str, bold: bool, italic: bool) -> Self {
        Self {
            ascent_share: crate::text::shape::ascent_share_for(family, bold, italic),
        }
    }
}

/// A resolved `a:lnSpc` / `a:spcBef` / `a:spcAft` value.
///
/// - `None`: omitted.
/// - `Some((true, val))`: `a:spcPct@val`, thousandths of a percent
///   (`100_000` = 100 %).
/// - `Some((false, val))`: `a:spcPts@val`, **points** (the caller divides the
///   raw 1/100 pt model value by 100) — ref `renderer.ts:4902-4904`.
pub type SpaceLine = Option<(bool, f32)>;

/// `powerPointExactLinePoints` (`powerpoint-line-metrics.ts:317`).
///
/// PowerPoint rounds `spcPts` to whole points before laying the line out,
/// half up: 40.5 → 41, 45.25 → 45, 48.33 → 48, 49.25 → 49, 50.75 → 51.
/// Font sizes stay exact (10.5, 13.33, 40.5 and 55.5 pt keep a 1.2 × size
/// line).
pub fn exact_line_points(points: f32) -> f32 {
    (points + 0.5).floor()
}

/// `drawingMlLineHeight` (`metrics.ts:47`): `a:lnSpc` (ECMA-376 §21.1.2.2.5)
/// and `a:normAutofit@lnSpcReduction` (§21.1.2.1.3).
///
/// Percentage spacing multiplies the natural line box; point spacing replaces
/// it. The stored normal-autofit `reduction` applies only to percentage
/// spacing (including the implicit 100 %), never to `spcPts`.
///
/// This is the shared DrawingML kernel and does not round `spcPts`; the
/// PowerPoint path rounds through [`exact_line_points`] (applied by
/// [`spaced_line_box`], mirroring `renderer.ts:5440`).
pub fn line_height(natural_height: f32, space: SpaceLine, px_per_pt: f32, reduction: f32) -> f32 {
    match space {
        Some((false, val)) => val * px_per_pt,
        Some((true, val)) => natural_height * (val / 100_000.0) * (1.0 - reduction),
        None => natural_height * (1.0 - reduction),
    }
}

/// `powerPointNaturalLine` (`powerpoint-line-metrics.ts:296`).
///
/// Each run claims `1.2 × its own size`, split by its face's ascent share.
/// The line unions the runs' ascent and descent parts and rescales them into
/// `1.2 × the largest size`, so `ascent + descent` is exactly that height
/// (mixed fonts *and* mixed sizes; taking the largest descent instead is off
/// by up to 5 px, the largest ascent by 13 px).
///
/// A line with no measurable part (no runs, or all zero-sized) takes the
/// synthetic 0.8 / 0.2 split of the box.
pub fn natural_line_box(runs: &[(f32, FaceMetrics)]) -> (f32, f32) {
    let mut max_size = 0.0f32;
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;
    for &(size, face) in runs {
        max_size = max_size.max(size);
        ascent = ascent.max(1.2 * size * face.ascent_share);
        descent = descent.max(1.2 * size * (1.0 - face.ascent_share));
    }
    let height = 1.2 * max_size;
    let measurable = ascent + descent;
    // `!(ascent + descent > 0)` in the reference, NaN included.
    if measurable.is_nan() || measurable <= 0.0 {
        return (
            height * SYNTHETIC_ASCENT_EM,
            height * SYNTHETIC_DESCENT_EM,
        );
    }
    let above = height * ascent / measurable;
    (above, height - above)
}

/// `drawingMlSpacedLineBox` (`metrics.ts:97`): where `a:lnSpc` puts the
/// baseline inside the spaced line.
///
/// Natural box `a`/`d`, `L = a + d`; spaced height `H` from
/// [`line_height`]. With `k = max(0, d − 0.25·L)` (deep-descent faces such as
/// Meiryo and Yu Gothic have `k > 0`):
/// - `H = L`: the natural box is kept.
/// - `H < L`: `d' = min(d, 0.25·H + k)`.
/// - `H > L`: while `0.25·H <= d` the natural descent is kept and all the
///   extra space goes above; past that point `d' = 0.25·H + k`. The switch is
///   a real step of `k` at `H = 4d`, not a gradual change.
///
/// The ascent is the rest of `H`, so a run of same-size lines steps by exactly
/// `H`. `spcPts` is rounded to whole points first
/// (`renderer.ts:5440-5441`); the percentage itself is **not** rounded by the
/// PowerPoint path.
pub fn spaced_line_box(
    ascent: f32,
    descent: f32,
    space: SpaceLine,
    px_per_pt: f32,
    reduction: f32,
) -> (f32, f32) {
    let natural = ascent + descent;
    let space = match space {
        Some((false, val)) => Some((false, exact_line_points(val))),
        other => other,
    };
    let height = line_height(natural, space, px_per_pt, reduction);
    if height == natural {
        return (ascent, descent);
    }
    let k = (descent - 0.25 * natural).max(0.0);
    let below = if height < natural {
        descent.min(0.25 * height + k)
    } else if 0.25 * height <= descent {
        descent
    } else {
        0.25 * height + k
    };
    (height - below, below)
}

/// `drawingMlParagraphSpacing` (`metrics.ts:121`): `a:spcBef` (§21.1.2.2.10)
/// / `a:spcAft` (§21.1.2.2.9) in px.
///
/// `spcPts` is absolute. For `spcPct`, Excel/PowerPoint's base is the natural
/// single-line height of the adjacent line — the paragraph's first line for
/// `spcBef`, its last line for `spcAft` — which the caller passes as
/// `natural_line_height` as `1.2 × the largest size` of that line
/// (`pctSpacingUnit`, ref `renderer.ts:5399`, `5446-5463`). The base stays the
/// natural line when the paragraph's own `lnSpc` is 80 %, 150 % or a point
/// value.
///
/// `a:bodyPr@spcFirstLastPara` (default false) gates the *edges* of the body
/// (`renderer.ts:5472-5476`): the first paragraph's `spcBef` and the last
/// paragraph's `spcAft` move no glyph unless the attribute is an explicit 1.
/// That gate belongs to the body orchestrator, not this kernel.
pub fn paragraph_spacing(space: SpaceLine, natural_line_height: f32, px_per_pt: f32) -> f32 {
    match space {
        None => 0.0,
        Some((false, val)) => val * px_per_pt,
        Some((true, val)) => natural_line_height * (val / 100_000.0),
    }
}

/// `drawingMlBlockTop` (`metrics.ts:133`): ECMA-376 §20.1.7.2
/// `a:bodyPr@anchor` for the line block.
///
/// The text rectangle is the shape box inset by `tIns`/`bIns` (width/height
/// clamped at 0, ref `drawingMlTextRect`). `ctr` is **unclamped**, so a block
/// taller than the box overflows both edges; `b` clamps at the rectangle
/// bottom. Anything else (`t`, `just`, `dist`, omitted) returns the rectangle
/// top.
pub fn block_top(
    anchor: &str,
    box_top: f32,
    box_h: f32,
    inset_t: f32,
    inset_b: f32,
    content_h: f32,
) -> f32 {
    let rect_top = box_top + inset_t;
    let rect_h = (box_h - inset_t - inset_b).max(0.0);
    match anchor {
        "ctr" => rect_top + (rect_h - content_h) / 2.0,
        "b" => rect_top + (rect_h - content_h).max(0.0),
        _ => rect_top,
    }
}

/// `drawingMlLineX` (`align.ts:6`): `a:pPr@algn` (§20.1.10.59) in a physical
/// horizontal text region. `just`/`justLow`/`dist`/`thaiDist` get their
/// unexpanded leading origin from here.
///
/// The slack is **signed**: PowerPoint preserves the negative slack when text
/// overflows a centred/right box (that is `allowOverflow = true`, the mode the
/// renderer uses for both the `ctr` and `r` pen at `renderer.ts:5888` and
/// `5895`). Under `rtl`, an `l`/omitted alignment puts the line at the right
/// edge of the region.
pub fn align_x(
    alignment: &str,
    region_left: f32,
    region_width: f32,
    natural: f32,
    rtl: bool,
) -> f32 {
    let slack = region_width - natural;
    if alignment == "ctr" {
        return region_left + slack / 2.0;
    }
    if alignment == "r" || (rtl && (alignment == "l" || alignment.is_empty())) {
        return region_left + slack;
    }
    region_left
}

/// `drawingMlLineShouldJustify` (`align.ts:24`): `just`/`justLow` leave the
/// final or manually terminated line ragged; `dist`/`thaiDist` justify every
/// line.
pub fn should_justify(alignment: &str, last_line: bool, ends_with_break: bool) -> bool {
    match alignment {
        "dist" | "thaiDist" => true,
        "just" | "justLow" => !last_line && !ends_with_break,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arial: usWinAscent 1854, usWinDescent 434 (`powerpoint-line-metrics.test.ts:210`).
    const ARIAL: FaceMetrics = FaceMetrics {
        ascent_share: 1854.0 / 2288.0,
    };

    /// The reference export quantises every baseline to a whole 1/100 in
    /// (0.72 pt at the specs' 1 pt = 1 unit scale): `Math.floor(y / U + 0.5)`,
    /// `powerpoint-line-metrics.test.ts:64-67`.
    fn snap(continuous: f32) -> i32 {
        (continuous / 0.72 + 0.5).floor() as i32
    }

    /// Baselines of `count` same-size lines; `space` is the paragraph's
    /// `a:lnSpc` (`None` = omitted). One canvas unit = 1 pt.
    fn baselines(size: f32, count: usize, space: SpaceLine) -> Vec<i32> {
        let (a, d) = natural_line_box(&[(size, ARIAL)]);
        let (a, d) = spaced_line_box(a, d, space, 1.0, 0.0);
        (0..count)
            .map(|i| snap(a + i as f32 * (a + d)))
            .collect()
    }

    const PCT100: SpaceLine = Some((true, 100_000.0));

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn arial_72pt_baselines() {
        // A-* control: two 72 pt Arial lines, PowerPoint 16.113.2 PDF.
        assert_eq!(baselines(72.0, 2, PCT100), [97, 217]);
        let (a, d) = natural_line_box(&[(72.0, ARIAL)]);
        close(a, 1.2 * 72.0 * 1854.0 / 2288.0);
        close(a + d, 86.4);
    }

    #[test]
    fn arial_100pt_baselines() {
        // The export rounds each baseline, not the 166.67-unit pitch.
        assert_eq!(baselines(100.0, 2, PCT100), [135, 302]);
    }

    #[test]
    fn fractional_sizes_keep_an_exact_1_2_line() {
        // Z-* control: 10.5 pt Arial, eight lines.
        assert_eq!(
            baselines(10.5, 8, PCT100),
            [14, 32, 49, 67, 84, 102, 119, 137]
        );
    }

    #[test]
    fn mixed_size_lines_union_and_rescale() {
        // 40 pt line, 40 + 100 pt mixed line, 40 pt line (#1610 control):
        // 1.2 × 40 = 48 and 1.2 × 100 = 120 pitches.
        let (a1, d1) = natural_line_box(&[(40.0, ARIAL)]);
        let (a2, d2) = natural_line_box(&[(40.0, ARIAL), (100.0, ARIAL)]);
        let (a3, _) = natural_line_box(&[(40.0, ARIAL)]);
        let y = [a1, a1 + d1 + a2, a1 + d1 + a2 + d2 + a3];
        assert_eq!(y.map(snap), [54, 202, 287]);
        close(a2 + d2, 120.0);
    }

    #[test]
    fn natural_box_unions_parts_then_rescales_into_1_2_max_size() {
        // Largest ascent alone would be 108, the largest descent 21.6; the
        // line is rescaled into 1.2 × 100 = 120 as 100 / 20.
        let (a, d) = natural_line_box(&[
            (100.0, FaceMetrics { ascent_share: 0.9 }),
            (20.0, FaceMetrics { ascent_share: 0.1 }),
        ]);
        close(a, 100.0);
        close(d, 20.0);
        // Degenerate shares and sizes keep the box consistent.
        assert_eq!(
            natural_line_box(&[(10.0, FaceMetrics { ascent_share: 0.0 })]),
            (0.0, 12.0)
        );
        assert_eq!(natural_line_box(&[(0.0, ARIAL)]), (0.0, 0.0));
    }

    #[test]
    fn spc_pct_80_on_20px_is_19_2() {
        // percentage-line-spacing: spcPct 80 → 0.96 × size, natural 1.2 × 20.
        let natural = 1.2 * 20.0;
        close(line_height(natural, Some((true, 80_000.0)), 1.0, 0.0), 19.2);
        let (a, d) = natural_line_box(&[(20.0, ARIAL)]);
        let (a, d) = spaced_line_box(a, d, Some((true, 80_000.0)), 1.0, 0.0);
        close(a + d, 19.2);
        // Exactly one pitch, so the second baseline steps by 19.2.
        let second = a + (a + d);
        close(second - a, 19.2);    }

    #[test]
    fn line_height_follows_drawingml_rule() {
        close(line_height(120.0, Some((true, 150_000.0)), 1.0, 0.0), 180.0);
        close(line_height(120.0, PCT100, 1.0, 0.1), 108.0);
        close(line_height(120.0, None, 1.0, 0.0), 120.0);
        close(line_height(120.0, None, 1.0, 0.1), 108.0);
        // spcPts replaces the box and ignores lnSpcReduction.
        close(line_height(120.0, Some((false, 18.0)), 1.0, 0.1), 18.0);
        close(line_height(120.0, Some((false, 12.0)), 2.0, 0.0), 24.0);
    }

    #[test]
    fn spc_pts_rounds_half_up_to_whole_points() {
        assert_eq!(exact_line_points(40.5), 41.0);
        assert_eq!(exact_line_points(45.25), 45.0);
        assert_eq!(exact_line_points(48.33), 48.0);
        assert_eq!(exact_line_points(49.25), 49.0);
        assert_eq!(exact_line_points(50.75), 51.0);
        assert_eq!(exact_line_points(114.0), 114.0);
        // The kernel rounds the same way before splitting.
        let (a, d) = natural_line_box(&[(100.0, ARIAL)]);
        assert_eq!(
            spaced_line_box(a, d, Some((false, 40.5)), 1.0, 0.0),
            spaced_line_box(a, d, Some((false, 41.0)), 1.0, 0.0)
        );
    }

    #[test]
    fn spaced_box_redivides_arial_100pt_spc_pts() {
        // #1610 controls: Arial 100 pt, spcPts 114 then 126 pt.
        assert_eq!(baselines(100.0, 2, Some((false, 114.0))), [127, 285]);
        assert_eq!(baselines(100.0, 2, Some((false, 126.0))), [131, 306]);
    }

    #[test]
    fn spaced_box_keeps_the_natural_split_when_h_equals_l() {
        // H = L returns the natural box verbatim, even with a deep descent
        // where 0.25·H + k would differ (the MS Gothic 120 pt control).
        assert_eq!(
            spaced_line_box(60.0, 60.0, Some((false, 120.0)), 1.0, 0.0),
            (60.0, 60.0)
        );
        // Past the 4d step the descent becomes 0.25·H + k: d = 60 implies
        // 4d = 240, so at spcPts 244 it is 61 + 30 and the ascent takes the rest.
        let (a, d) = spaced_line_box(60.0, 60.0, Some((false, 244.0)), 1.0, 0.0);
        close(d, 91.0);
        close(a, 153.0);
    }

    #[test]
    fn spaced_box_deep_descent_keeps_d_until_4d_then_steps_by_k() {
        // Meiryo 14 pt in the Excel natural box: d 9.31 pt, L 27.24 pt,
        // k = 9.31 − 0.25·27.24 = 2.5 (metrics.ts:100-108).
        let (a, d) = (17.93, 9.31);
        // H < L: descent is 0.25·H + k.
        let (a2, d2) = spaced_line_box(a, d, Some((false, 12.0)), 1.0, 0.0);
        close(d2, 5.5);
        close(a2, 6.5);
        // H > L but 0.25·H <= d: the natural descent is kept.
        let (a3, d3) = spaced_line_box(a, d, Some((false, 36.0)), 1.0, 0.0);
        close(d3, 9.31);
        close(a3, 26.69);
        // H > 4d: the step, 0.25·40 + 2.5 = 12.5 at spcPts 40.
        let (a4, d4) = spaced_line_box(a, d, Some((false, 40.0)), 1.0, 0.0);
        close(d4, 12.5);
        close(a4, 27.5);
    }

    #[test]
    fn spaced_box_applies_ln_spc_reduction_to_percent_only() {
        // N-Arial-90-10-t: normAutofit fontScale 90 %, lnSpcReduction 10 %,
        // 40 pt text (four lines).
        let (a, d) = natural_line_box(&[(40.0 * 0.9, ARIAL)]);
        let (a2, d2) = spaced_line_box(a, d, PCT100, 1.0, 0.1);
        let ys: Vec<i32> = (0..4).map(|i| snap(a2 + i as f32 * (a2 + d2))).collect();
        let natural_d = 1.2 * 36.0 * (1.0 - 1854.0 / 2288.0);
        close(a2 + d2, 1.2 * 36.0 * 0.9);
        // H < L: the natural descent is kept, the ascent shrinks by 10 %.
        close(d2, natural_d);
        close(a2, 1.2 * 36.0 * 0.9 - natural_d);
        assert_eq!(ys, [43, 97, 151, 205]);
    }

    #[test]
    fn percentage_paragraph_spacing_uses_the_adjacent_line() {
        // 20 pt line (24 pt unit): spcBef 50 % adds 12 pt; a 40 pt line
        // (48 pt unit) adds 48 pt at 100 % — percentage-paragraph-spacing.
        close(paragraph_spacing(Some((true, 50_000.0)), 24.0, 1.0), 12.0);
        close(
            paragraph_spacing(Some((true, 100_000.0)), 40.0 * 1.2, 1.0),
            48.0,
        );
        // The base does not follow the paragraph's own lnSpc: the 40 pt
        // line's 80 % box is 38.4 pt, but 100 % spcBef still adds 48 pt.
        let (a, d) = natural_line_box(&[(40.0, ARIAL)]);
        let (a, d) = spaced_line_box(a, d, Some((true, 80_000.0)), 1.0, 0.0);
        close(a + d, 38.4);
        close(
            paragraph_spacing(Some((true, 100_000.0)), 1.2 * 40.0, 1.0),
            48.0,
        );
        // spcPts is absolute, omitted spacing adds nothing.
        close(paragraph_spacing(Some((false, 18.0)), 48.0, 1.0), 18.0);
        close(paragraph_spacing(Some((false, 18.0)), 48.0, 2.0), 36.0);
        close(paragraph_spacing(None, 48.0, 1.0), 0.0);
    }

    #[test]
    fn block_top_anchors_like_drawingml() {
        // G-Arial-ctr: two 100 pt lines (240 pt) in a 420 pt box, 3.6 pt
        // insets, block top 3.6 + (420 − 7.2 − 240) / 2 = 90.
        close(block_top("ctr", 0.0, 420.0, 3.6, 3.6, 240.0), 90.0);
        close(block_top("t", 0.0, 420.0, 3.6, 3.6, 240.0), 3.6);
        close(block_top("b", 0.0, 420.0, 3.6, 3.6, 240.0), 176.4);
        // ctr is unclamped: an over-tall block overflows both edges.
        close(block_top("ctr", 0.0, 100.0, 0.0, 0.0, 120.0), -10.0);
        // b clamps at the rectangle bottom, t ignores the content.
        close(block_top("b", 0.0, 100.0, 0.0, 0.0, 120.0), 0.0);
        // A box thinner than its insets collapses the rectangle height.
        close(block_top("ctr", 0.0, 10.0, 20.0, 20.0, 0.0), 20.0);
    }

    #[test]
    fn align_x_keeps_the_signed_slack_for_ctr_and_r() {
        // Slack, centred: (200 − 100) / 2.
        close(align_x("ctr", 0.0, 200.0, 100.0, false), 50.0);
        // Overflow keeps the signed offset (renderer.ts:5888, 5895).
        close(align_x("ctr", 0.0, 100.0, 130.0, false), -15.0);
        close(align_x("r", 0.0, 100.0, 130.0, false), -30.0);
        // Right alignment and the rtl left fallback share the slack.
        close(align_x("r", 10.0, 100.0, 40.0, false), 70.0);
        close(align_x("l", 10.0, 100.0, 40.0, false), 10.0);
        close(align_x("l", 0.0, 100.0, 40.0, true), 60.0);
        close(align_x("", 0.0, 100.0, 40.0, true), 60.0);
        close(align_x("ctr", 0.0, 100.0, 40.0, true), 30.0);
        // Justified lines start at the region's leading edge.
        close(align_x("just", 5.0, 100.0, 40.0, false), 5.0);
        close(align_x("justLow", 5.0, 100.0, 140.0, true), 5.0);
    }

    #[test]
    fn should_justify_follows_the_alignment_rules() {
        assert!(should_justify("dist", true, true));
        assert!(should_justify("thaiDist", true, false));
        assert!(should_justify("just", false, false));
        assert!(should_justify("justLow", false, false));
        // Final and manually terminated lines stay ragged.
        assert!(!should_justify("just", true, false));
        assert!(!should_justify("just", false, true));
        assert!(!should_justify("justLow", true, true));
        // Other alignments never justify.
        for alignment in ["l", "ctr", "r", ""] {
            assert!(!should_justify(alignment, false, false));
        }
    }
}
