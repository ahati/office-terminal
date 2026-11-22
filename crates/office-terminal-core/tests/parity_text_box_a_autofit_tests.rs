//! `firstline-indent-autofit.test.ts` and `sp-autofit-top-anchor.test.ts`.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/renderer.ts:1530-1607` — `firstLineIndentPxFor`
//!   (§21.1.2.2.7: only a NON-bullet paragraph consumes the signed indent) and
//!   `naturalWidthExceedsBbox`.
//! - `renderer.ts:5866-5900` — the draw-side `textXOffset` / `effectiveTextX`
//!   and the `ctr` / `r` regions that the expanded first line reuses.
//! - `renderer.ts:5530-5575` — the anchor block, including the zero-height
//!   `anchor="b"` shape that grows upward.
//!
//! The indent probes use the specs' own stub measurer (10 units per code point,
//! 10 per space) so the wrap budgets hit their exact numbers. The anchor probes
//! use `support::Stack`, which drives the production kernels in the reference's
//! order with the catalogued face share (`text/shape.rs` resolves the share
//! from the host face instead, so the spec's usWin numbers are only reachable
//! with the share passed in).

#[path = "text_box_a_support/mod.rs"]
mod support;

use office_terminal_core::text::linebreak::{break_paragraph, BreakItem, BreakOptions, LineItem};
use office_terminal_core::text::metrics::align_x;
use office_terminal_core::text::{StyleId, TextMeasure};
use support::{
    body_with, close, compile, first_glyph_x, line_first_xs, line_pts, para, text, text_face,
    Para, Stack, ARIAL, EMU_PER_PT, MEIRYO,
};

const S: StyleId = StyleId(0);
/// 10 units of advance per code point, 10 units per space.
static LEN10: Len10 = Len10;

struct Len10;

impl TextMeasure for Len10 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 10.0
    }

    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

fn wrap_chars(text: &str, max_width: f32, first_line_indent: f32) -> Vec<usize> {
    let items = [BreakItem::Text {
        text: text.to_string(),
        style: S,
    }];
    let options = BreakOptions {
        max_width,
        first_line_indent,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer: &LEN10,
    };
    break_paragraph(&items, &options)
        .iter()
        .map(|line| {
            line.items
                .iter()
                .map(|item| match item {
                    LineItem::Text { text, .. } => text.chars().count(),
                    _ => 1,
                })
                .sum::<usize>()
        })
        .collect()
}

// ── firstline-indent-autofit.test.ts ───────────────────────────────────────

/// `it('control: returns TRUE for the SAME positive indent on a NON-bullet
/// paragraph')`: 18 glyphs (180 units) do not fit the 135.6-unit first line
/// that a +50 indent leaves, so the paragraph wraps.
#[test]
fn positive_indent_narrows_the_first_line_budget() {
    // Box 200, insets 7.2 + 7.2, indent +50 ⇒ 135.6.
    let wide = wrap_chars(&"あ".repeat(18), 185.6, 0.0);
    let narrow = wrap_chars(&"あ".repeat(18), 185.6, 50.0);
    assert_eq!(wide.len(), 1, "{wide:?}");
    assert_eq!(narrow.len(), 2, "{narrow:?}");
    // 13 glyphs fit the narrowed budget, 14 do not (135.6 / 10).
    assert_eq!(wrap_chars(&"あ".repeat(13), 185.6, 50.0).len(), 1);
    assert_eq!(wrap_chars(&"あ".repeat(14), 185.6, 50.0).len(), 2);
}

/// `it('draws a negative non-bullet first-line indent left of marL')`: the
/// signed indent moves the first line's pen by exactly −50 pt.
#[test]
fn negative_indent_moves_the_first_line_left() {
    let (zero, neg) = (0i64, -(50.0 * EMU_PER_PT).round() as i64);
    let first_x = |indent: i64| {
        let body = body_with(
            &[para(vec![text("AAAA", 20.0)]).indent(indent)],
            "t",
            (0.0, 0.0, 0.0, 0.0),
            "none",
            None,
            None,
            false,
            20.0,
        );
        // wrap="none" so only the draw offset is in play.
        let mut body = body;
        body.wrap = "none".to_string();
        first_glyph_x(&compile(&body, 200.0, 200.0))
    };
    close(
        (first_x(neg) - first_x(zero)) / EMU_PER_PT,
        -50.0,
        "negative first-line indent",
    );
}

/// `it('extends only the first wrap line and keeps continuation lines at
/// marL')`: 40 glyphs with marL 50 and indent −50 wrap as 20 / 15 / 5 and
/// start at 0 / 50 / 50.
#[test]
fn only_the_first_line_takes_the_expanded_region() {
    // textMaxW = 200 − marL 50 = 150; the −50 indent expands line 1 to 200.
    let rows = wrap_chars(&"あ".repeat(40), 150.0, -50.0);
    assert_eq!(rows, vec![20usize, 15, 5]);

    // Draw side: the first line starts at 0 (marL 50 + indent −50), the
    // continuation lines at marL (50).
    let body = body_with(
        &[para(vec![text("AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA", 20.0)])
            .mar_l(50 * 12_700)
            .indent(-(50 * 12_700))],
        "t",
        (0.0, 0.0, 0.0, 0.0),
        "none",
        None,
        None,
        false,
        20.0,
    );
    let xs: Vec<f32> = line_first_xs(&compile(&body, 200.0, 200.0))
        .iter()
        .map(|x| x / EMU_PER_PT)
        .collect();
    assert!(xs.len() >= 2, "expected a wrapped paragraph: {xs:?}");
    close(xs[0], 0.0, "first line x");
    for x in &xs[1..] {
        close(*x, 50.0, "continuation x");
    }
}

/// `it.each(['l', 'ctr', 'r'])('uses the same expanded first-line region for
/// %s alignment')`: with marL 50, indent −50 and a 40-unit line in a 200-unit
/// body, the pens are 0 / 80 / 160 (`renderer.ts:5877-5899`).
#[test]
fn alignment_uses_the_expanded_first_line_region() {
    // l: effectiveTextX = align(textX 50, textMaxW 150) + indent −50.
    close(align_x("l", 50.0, 150.0, 40.0, false) + -50.0, 0.0, "l");
    // ctr: region (effectiveTextX 0, width textMaxW − indent = 200).
    close(align_x("ctr", 0.0, 200.0, 40.0, false), 80.0, "ctr");
    // r: region (textX 50, width textMaxW 150).
    close(align_x("r", 50.0, 150.0, 40.0, false), 160.0, "r");
}

/// `it('does not request wrapping when text fits the expanded first-line
/// region')`: 18 glyphs (180) fit 185.6 but not the 135.6 an indent of 0
/// leaves.
#[test]
fn expanded_first_line_no_longer_wraps() {
    assert_eq!(wrap_chars(&"あ".repeat(18), 185.6, 0.0).len(), 1);
    assert_eq!(wrap_chars(&"あ".repeat(18), 135.6, 0.0).len(), 2);
}

/// `it('still applies a POSITIVE non-bullet first-line indent at draw time
/// (regression guard)')`.
#[test]
fn positive_indent_still_shifts_the_first_line() {
    let first_x = |indent: i64| {
        let mut body = body_with(
            &[para(vec![text("AAAA", 20.0)]).indent(indent)],
            "t",
            (0.0, 0.0, 0.0, 0.0),
            "none",
            None,
            None,
            false,
            20.0,
        );
        body.wrap = "none".to_string();
        first_glyph_x(&compile(&body, 200.0, 200.0))
    };
    close(
        (first_x((50.0 * EMU_PER_PT) as i64) - first_x(0)) / EMU_PER_PT,
        50.0,
        "positive first-line indent",
    );
}

// ── sp-autofit-top-anchor.test.ts ──────────────────────────────────────────

/// `it.each([Meiryo 12/18/24/32, Arial 12/18/24/32])('uses the PowerPoint
/// 120%% implicit pitch ... with and without spAutoFit')`.
#[test]
fn implicit_pitch_is_120_pct_of_the_size_in_both_fit_modes() {
    for face in ["Meiryo", "Arial"] {
        for size in [12.0, 18.0, 24.0, 32.0] {
            for auto_fit in ["none", "sp"] {
                let body = body_with(
                    &[para(vec![
                        text_face("一行目", size, face),
                        support::BREAK,
                        text_face("二行目", size, face),
                    ])],
                    "t",
                    (0.0, 0.0, 0.0, 0.0),
                    auto_fit,
                    None,
                    None,
                    false,
                    size,
                );
                let ys = line_pts(&compile(&body, 400.0, 46.0));
                assert_eq!(ys.len(), 2, "{face} {size} {auto_fit}");
                close(ys[1] - ys[0], size as f32 * 1.2, "implicit pitch");
            }
        }
    }
}

/// `it('keeps implicit multi-line pitch separate from the taller resolved font
/// box (#1473)')`: the first baseline is the usWin split of the 1.2 × size
/// line, and the block needs the sum of the 1.2 × size lines.
#[test]
fn multi_line_pitch_ignores_the_resolved_font_box() {
    let size = 32.0f32;
    let stack = Stack::new(vec![Para::of(vec![
        support::line(size, MEIRYO),
        support::line(size, MEIRYO),
    ])]);
    let ys = stack.baselines();
    close(ys[0], size * 1.2 * MEIRYO.ascent_share, "first baseline");
    close(
        ys[1] - ys[0],
        size * 1.2,
        "pitch must be the 1.2 × size line",
    );
    close(stack.needed_height(), 2.0 * size * 1.2, "needed height");
}

/// `it.each([['theme-resolved heading face', 32, 46], ['explicit Meiryo face',
/// 36, 50.9]])('seats the %s first line by its metric split, not by glyph
/// ink')`.
#[test]
fn first_line_sits_on_the_metric_split() {
    for (size, stored_height) in [(32.0f32, 46.0f32), (36.0, 50.9)] {
        let stack = Stack::new(vec![Para::one(size, MEIRYO)]).in_box(0.0, stored_height, 0.0, 0.0);
        let ys = stack.baselines();
        assert_eq!(ys.len(), 1);
        close(ys[0], size * 1.2 * MEIRYO.ascent_share, "metric split");
    }
}

/// `it.each(['ctr', 'b'])('anchors %s spAutoFit text in the authored box
/// without regrowing it')`: a 76.8 pt block in a 30 pt box overflows both
/// edges.
#[test]
fn anchored_sp_autofit_block_overflows_without_regrowing() {
    let size = 32.0f32;
    let block = 2.0 * size * 1.2;
    let box_height = 30.0f32;
    for anchor in ["ctr", "b"] {
        let stack = Stack::new(vec![Para::one(size, MEIRYO), Para::one(size, MEIRYO)])
            .in_box(0.0, box_height, 0.0, 0.0)
            .anchor(anchor);
        let top = if anchor == "ctr" {
            (box_height - block) / 2.0
        } else {
            box_height - block
        };
        close(
            stack.baselines()[0],
            top + size * 1.2 * MEIRYO.ascent_share,
            anchor,
        );
    }
}

/// `it('keeps the zero-height bottom-anchored shape growing upward')`.
#[test]
fn zero_height_bottom_anchor_grows_upward() {
    let size = 32.0f32;
    let line = size * 1.2;
    // bh = 0, anchor b: the block is placed above `by` (100).
    let stack = Stack::new(vec![Para::one(size, MEIRYO)]).in_box(100.0 - line, line, 0.0, 0.0);
    let stack = stack.anchor("b");
    close(
        stack.baselines()[0],
        100.0 - line + size * 1.2 * MEIRYO.ascent_share,
        "grows upward",
    );
}

/// `it('places Arial by its own split in fixed and spAutoFit shapes alike')`.
#[test]
fn arial_uses_its_own_split_in_both_fit_modes() {
    let size = 32.0f32;
    // A fixed 46 pt box and the shape regrown to its 38.4 pt content: the
    // `t` cursor is the box top in both, so the split alone decides.
    for box_height in [46.0f32, size * 1.2] {
        let stack = Stack::new(vec![Para::one(size, ARIAL)]).in_box(0.0, box_height, 0.0, 0.0);
        close(
            stack.baselines()[0],
            size * 1.2 * ARIAL.ascent_share,
            "Arial split",
        );
    }
}
