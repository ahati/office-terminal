//! Parity ports of the reference text-flow specs (batch `text-flow-a`, part 1:
//! justification kernel).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/text-justify.test.ts`
//! (20 `it()` cases). Every probe feeds the spec's own segments/numbers to the
//! crate's `justify_line` and asserts the reference's `splitBefore` offsets and
//! `perGap` pitch. The reference annotates each laid-out segment with `jext` /
//! `splitBefore`; this kernel returns the equivalent boundary mask over the
//! line's flat unit stream, so `splitBefore = [1, 2]` reads `gap_positions() ==
//! [1, 2]` and `perGap == per_gap`.
//!
//! Coverage report: `tests/parity/coverage/text-flow-a.md`.

#[path = "parity_text_flow_a_support/mod.rs"]
mod support;

use office_terminal_core::text::justify::{justify_line, JustifyMode};
use office_terminal_core::text::linebreak::LineItem;
use support::{close, item, line_from_items, plan, B, S};

// ── text-justify.test.ts ───────────────────────────────────────────────────

/// `:24-26`: `just` leaves the final line natural.
#[test]
fn c01_just_leaves_the_last_line_null() {
    assert_eq!(
        justify_line(&[item("日本語", 60.0)], 120.0, 60.0, JustifyMode::Just, true, false),
        None
    );
}

/// `:28-33`: `dist` justifies the last line too — 3 CJK glyphs, 2 gaps, slack
/// 60 → perGap 30 with `splitBefore` `[1, 2]`.
#[test]
fn c02_dist_justifies_the_last_line() {
    let p = plan(&[item("日本語", 60.0)], 120.0, 60.0, JustifyMode::Dist, true, false);
    close(p.per_gap, 30.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 2]);
}

/// `:35-46`: `thaiDist` distributes space-free Thai at grapheme-cluster
/// boundaries (`[1, 3, 4]`, ◌ื stays glued to ม), slack 30 → 10 each.
#[test]
fn c03_thai_dist_opens_sea_cluster_boundaries() {
    let p = plan(&[item("เมือง", 60.0)], 90.0, 60.0, JustifyMode::ThaiDist, false, false);
    close(p.per_gap, 10.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 3, 4]);
}

/// `:47-52`: `dist` is NOT an alias of `thaiDist` — space-free Thai stays ragged.
#[test]
fn c04_dist_leaves_space_free_thai_ragged() {
    assert_eq!(
        justify_line(&[item("เมือง", 60.0)], 90.0, 60.0, JustifyMode::Dist, false, false),
        None
    );
}

/// `:54-60`: pure Latin widens inter-word spaces; gaps before 'w' (6) and 'f'
/// (12), slack 100 → 50 each, no trailing gap.
#[test]
fn c05_latin_widens_inter_word_spaces() {
    let p = plan(
        &[item("Hello world foo", 100.0)],
        200.0,
        100.0,
        JustifyMode::Just,
        false,
        false,
    );
    close(p.per_gap, 50.0, "perGap");
    assert_eq!(p.gap_positions(), [6, 12]);
}

/// `:62-66`: pure CJK widens every inter-character gap except after the final
/// glyph.
#[test]
fn c06_cjk_widens_inner_gaps_only() {
    let p = plan(&[item("日本語", 60.0)], 120.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 30.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 2]);
}

/// `:68-74`: mixed `EC市場で` — no gap inside the Latin "EC", gaps at C|市,
/// 市|場, 場|で; slack 30 → 10.
#[test]
fn c07_mixed_latin_cjk_gaps() {
    let p = plan(
        &[item("EC市場で", 100.0)],
        130.0,
        100.0,
        JustifyMode::Just,
        false,
        false,
    );
    close(p.per_gap, 10.0, "perGap");
    assert_eq!(p.gap_positions(), [2, 3, 4]);
}

/// `:76-83`: leading 字下げ whitespace stays fixed; gaps at 3 and 4, slack 30 →
/// 15.
#[test]
fn c08_leading_indent_whitespace_stays_fixed() {
    let p = plan(
        &[item("  日本語", 100.0)],
        130.0,
        100.0,
        JustifyMode::Just,
        false,
        false,
    );
    close(p.per_gap, 15.0, "perGap");
    assert_eq!(p.gap_positions(), [3, 4]);
}

/// `:85-90`: trailing whitespace at the line end does not stretch (one gap,
/// slack 40).
#[test]
fn c09_trailing_whitespace_does_not_stretch() {
    let p = plan(&[item("日本 ", 60.0)], 100.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 40.0, "perGap");
    assert_eq!(p.gap_positions(), [1]);
}

/// `:92-113`: CJK boundaries are evaluated across a segment boundary —
/// `[日本][語]` opens gaps after 日 and after 本 (slack 60 → 30 each).
#[test]
fn c10_cjk_boundaries_cross_segment_seams() {
    let items = [item("日本", 40.0), item("語", 20.0)];
    let p = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 30.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 2]);
}

/// `:115-125`: gaps open INSIDE the final segment (`[日本][語学]` → gaps after
/// 日, 本, 語), only the boundary after the last glyph is trimmed.
#[test]
fn c11_gaps_open_inside_the_final_segment() {
    let items = [item("日本", 40.0), item("語学", 40.0)];
    let p = plan(&items, 150.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 30.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 2, 3]);
}

/// `:127-146`: an inline object is one unit and can take a gap: gaps after 日
/// and after the object → perGap = 40/2 = 20.
#[test]
fn c12_inline_object_is_one_unit_and_takes_a_gap() {
    let items = [
        item("日", 20.0),
        LineItem::Object { width: 20.0, style: S },
        item("本", 20.0),
    ];
    let p = plan(&items, 100.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 20.0, "perGap");
    // Boundary 1 = before the object, boundary 2 = before 本.
    assert_eq!(p.gap_positions(), [1, 2]);
    // The object atom itself is opaque: it holds no internal boundary.
    assert_eq!(p.gap_after, [false, true, true, false]);
}

/// `:148-162`: an empty-text segment (inline OMML) contributes no unit and no
/// gap; the single remaining gap (日|本) takes the whole slack 60.
#[test]
fn c13_empty_segment_contributes_no_gap() {
    let items = [item("日", 20.0), item("", 0.0), item("本", 20.0)];
    let p = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 60.0, "perGap");
    assert_eq!(p.gap_positions(), [1]);
    assert_eq!(p.gap_count(), 1);
}

/// `:164-176`: Σ jext + Σ(perGap × splits) equals the slack (line reaches
/// `availWidth`): "Hello world foo bar", avail 300, natural 120 → 180 across 3
/// gaps.
#[test]
fn c14_stretch_sums_to_the_slack() {
    let p = plan(
        &[item("Hello world foo bar", 120.0)],
        300.0,
        120.0,
        JustifyMode::Just,
        false,
        false,
    );
    assert_eq!(p.gap_positions(), [6, 12, 16]);
    close(p.gap_count() as f32 * p.per_gap, 180.0, "total stretch");
}

/// `:178-180`: no slack to distribute (60.2 vs 60) → null.
#[test]
fn c15_no_slack_returns_null() {
    assert_eq!(
        justify_line(&[item("日本語", 60.0)], 60.2, 60.0, JustifyMode::Just, false, false),
        None
    );
}

/// `:182-184`: a single Latin word has no inner gap → null.
#[test]
fn c16_single_latin_word_returns_null() {
    assert_eq!(
        justify_line(&[item("Hello", 50.0)], 200.0, 50.0, JustifyMode::Just, false, false),
        None
    );
}

/// `:186-188`: a single CJK glyph is one content unit → null.
#[test]
fn c17_single_cjk_glyph_returns_null() {
    assert_eq!(
        justify_line(&[item("日", 20.0)], 200.0, 20.0, JustifyMode::Just, false, false),
        None
    );
}

/// `:190-193`: every annotated segment keeps its style fields. The Rust kernel
/// returns a gap mask instead of annotated copies, so the contract is that it
/// never rewrites the caller's items (style included).
#[test]
fn c18_items_and_styles_are_never_rewritten() {
    let items = [LineItem::Text {
        text: "日本語".to_string(),
        style: B,
        width: 60.0,
    }];
    let before = items.clone();
    let p = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 30.0, "perGap");
    assert_eq!(p.gap_positions(), [1, 2]);
    assert_eq!(items, before, "justify_line must not touch the items");
    // The unit still belongs to the authored style B.
    match &line_from_items(&items) {
        LineItem::Text { style, text, .. } => {
            assert_eq!(*style, B);
            assert_eq!(text, "日本語");
        }
        other => panic!("expected a text item, got {other:?}"),
    }
}

/// `:195-206`: U+3000 stretches as ONE inter-word gap (whitespace is classified
/// before the CJK predicate); `日　本` slack 60 → 60 at offset 2.
#[test]
fn c19_ideographic_space_is_one_gap() {
    let p = plan(&[item("日　本", 60.0)], 120.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 60.0, "perGap");
    assert_eq!(p.gap_positions(), [2]);
}

/// `:199-206`: U+3000 between CJK glyphs stays a single stretch point even with
/// neighbours: `日　本で` → gaps at 2 and 3, slack 80 → 40.
#[test]
fn c20_ideographic_space_with_neighbours() {
    let p = plan(&[item("日　本で", 60.0)], 140.0, 60.0, JustifyMode::Just, false, false);
    close(p.per_gap, 40.0, "perGap");
    assert_eq!(p.gap_positions(), [2, 3]);
}
