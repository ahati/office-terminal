//! Parity ports of the reference text-flow specs (batch `text-flow-a`, part 3:
//! soft-wrap of tab-led lines and multi-cell tab lines).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/pptx-tab-wrap.test.ts`
//! (9 `it()` cases) and `pptx-multi-tab.test.ts` (4 `it()` cases). Both specs
//! render through `renderTextBody`; the wrap and cell placement they pin live in
//! `break_paragraph`, so the probes call it with the specs' 20 px-per-code-point
//! stub and reconstruct the drawn pens from the broken lines' resolved widths.
//!
//! Coverage report: `tests/parity/coverage/text-flow-a.md`.

#[path = "parity_text_flow_a_support/mod.rs"]
mod support;

use office_terminal_core::text::linebreak::LineItem;
use office_terminal_core::text::TabAlign;
use support::{
    close, line_text, pieces, pieces_at, stop, tab_item, tab_widths, text_item, text_item_styled,
    wrap, wrap_indented, B, S,
};

// ── pptx-tab-wrap.test.ts (issue #1006) ────────────────────────────────────

const LATIN: &str = "\tt01 t02 t03 t04 t05 t06 t07 t08 t09 t10 t11 t12";

/// `:96-100`: control C01 — an authored run seam inside a wrapped word is kept.
#[test]
fn c34_run_seam_inside_a_wrapped_word() {
    let lines = wrap(
        &[text_item_styled("abc", S), text_item_styled("def", B)],
        90.0,
        &[],
        0.0,
    );
    assert_eq!(
        lines.iter().map(line_text).collect::<Vec<_>>(),
        ["abcd", "ef"]
    );
}

/// `:101-119`: a Latin leading tab wraps (the jump is charged to the budget)
/// and the continuation re-anchors at text-left. Left stop @100, budget 280 →
/// line 1 holds exactly t01 t02.
#[test]
fn c35_latin_leading_tab_wraps_and_continuation_re_anchors() {
    let lines = wrap(&[text_item(LATIN)], 280.0, &[stop(100.0, TabAlign::Left)], 0.0);
    assert!(lines.len() > 1, "tab-led paragraph must wrap: {lines:?}");
    close(pieces(&lines[0])[0].x, 100.0, "line 1 starts at the stop");
    assert_eq!(line_text(&lines[0]), "t01 t02");
    assert_eq!(line_text(&lines[1]), "t03 t04 t05");
    close(pieces(&lines[1])[0].x, 0.0, "continuation at text-left");
}

/// `:120-134`: a wider box fits five line-1 tokens and still re-anchors the
/// continuation at text-left.
#[test]
fn c36_wide_box_fits_more_tokens_and_re_anchors() {
    let lines = wrap(&[text_item(LATIN)], 520.0, &[stop(100.0, TabAlign::Left)], 0.0);
    assert!(lines.len() > 1);
    close(pieces(&lines[0])[0].x, 100.0, "line 1 starts at the stop");
    assert_eq!(line_text(&lines[0]), "t01 t02 t03 t04 t05");
    close(pieces(&lines[1])[0].x, 0.0, "continuation at text-left");
    assert!(line_text(&lines[1]).starts_with("t06"));
}

/// `:136-151`: a long space-free Thai run after a leading tab reaches the SEA
/// fill, wraps, and every continuation starts at text-left.
#[test]
fn c37_thai_no_space_leading_tab_wraps() {
    let thai = "\tการเขียนภาษาไทยไม่ใช้ช่องว่างระหว่างคำแต่ใช้ช่องว่างเฉพาะเมื่อจบประโยคหรือวลี";
    let lines = wrap(&[text_item(thai)], 280.0, &[stop(100.0, TabAlign::Left)], 0.0);
    assert!(lines.len() > 1, "tab-led Thai must wrap: {lines:?}");
    close(pieces(&lines[0])[0].x, 100.0, "line 1 at the tab stop");
    for (index, line) in lines.iter().enumerate().skip(1) {
        close(
            pieces(line)[0].x,
            0.0,
            &format!("Thai continuation line {index} at text-left"),
        );
    }
}

/// `:153-172`: multi-tab cells sit on their stops; the overflowing tail
/// re-anchors at text-left. Stops l@100, l@200; budget 260.
#[test]
fn c38_multi_tab_cells_sit_on_stops_and_tail_re_anchors() {
    let text = "A1\tB2\tt01 t02 t03 t04 t05 t06 t07 t08 t09 t10";
    let lines = wrap(
        &[text_item(text)],
        260.0,
        &[stop(100.0, TabAlign::Left), stop(200.0, TabAlign::Left)],
        0.0,
    );
    assert!(lines.len() > 1);
    let first = pieces(&lines[0]);
    let a1 = first.iter().find(|p| p.text == "A1").expect("A1 drawn");
    let b2 = first.iter().find(|p| p.text == "B2").expect("B2 drawn");
    let t01 = first.iter().find(|p| p.text == "t01").expect("t01 drawn");
    close(a1.x, 0.0, "A1 at the leading edge");
    close(b2.x, 100.0, "B2 on stop 1");
    close(t01.x, 200.0, "t01 on stop 2");
    close(pieces(&lines[1])[0].x, 0.0, "tail at text-left");
    assert!(line_text(&lines[1]).starts_with("t02"));
}

/// `:174-188`: `defTabSz` (no `a:tabLst`) — the leading tab lands on the grid
/// stop 100 and the continuation re-anchors.
#[test]
fn c39_default_grid_leading_tab() {
    let lines = wrap(&[text_item(LATIN)], 280.0, &[], 100.0);
    assert!(lines.len() > 1);
    close(pieces(&lines[0])[0].x, 100.0, "grid stop");
    assert_eq!(line_text(&lines[0]), "t01 t02");
    close(pieces(&lines[1])[0].x, 0.0, "continuation at text-left");
}

/// `:190-200`: a right-aligned tab cell that FITS never wraps — the cell ends
/// AT 200, so a tight 220 budget holds it (a tab-blind budget would wrap).
#[test]
fn c40_fitting_right_tab_cell_never_wraps() {
    let lines = wrap(&[text_item("ab\tvalue")], 220.0, &[stop(200.0, TabAlign::Right)], 0.0);
    assert_eq!(lines.len(), 1, "the fitting right-tab cell stays on one line");
    let value = pieces(&lines[0])
        .into_iter()
        .find(|p| p.text == "value")
        .expect("value drawn");
    close(value.x + value.width, 200.0, "value ends on the stop");
}

/// `:202-213`: a right-tab CJK cell that fits by shrinking its gap does not
/// wrap early — 9 glyphs (180 px) end AT 200 inside a 220 box.
#[test]
fn c41_fitting_right_tab_cjk_cell_does_not_wrap() {
    let lines = wrap(
        &[text_item("\t日本語日本語日本語")],
        220.0,
        &[stop(200.0, TabAlign::Right)],
        0.0,
    );
    assert_eq!(lines.len(), 1, "fitting right-tab CJK cell stays on one line");
    let all = pieces(&lines[0]);
    let last = all.last().expect("a drawn cell");
    close(last.x + last.width, 200.0, "cell right-aligned to the stop");
}

/// `:215-226`: a leading tab whose jump exceeds the box does not emit a
/// tab-only CJK line — line 1 carries a glyph and the rest wraps to text-left.
#[test]
fn c42_overwide_leading_tab_keeps_a_glyph_and_wraps() {
    let lines = wrap(
        &[text_item("\t日本語日本語日本語日本語")],
        260.0,
        &[stop(300.0, TabAlign::Left)],
        0.0,
    );
    assert!(lines.len() > 1, "the overwide tab must still wrap: {lines:?}");
    assert!(
        !line_text(&lines[0]).is_empty(),
        "the tab-led line carries a glyph, not tab-only"
    );
    close(pieces(&lines[1])[0].x, 0.0, "continuation at text-left");
}

// ── pptx-multi-tab.test.ts (issue #916) ────────────────────────────────────

/// `:100-118`: LTR renders ALL THREE cells, each ending on its stop — right
/// stops at 300 and 560 in a 600 px box.
#[test]
fn c43_ltr_renders_all_three_cells_on_their_stops() {
    let lines = wrap(
        &[text_item("title\tvalue\tpage")],
        600.0,
        &[
            stop(300.0, TabAlign::Right),
            stop(560.0, TabAlign::Right),
        ],
        0.0,
    );
    assert_eq!(lines.len(), 1);
    let all = pieces(&lines[0]);
    let find = |name: &str| {
        all.iter()
            .find(|p| p.text == name)
            .unwrap_or_else(|| panic!("{name} cell drawn: {all:?}"))
    };
    let (title, value, page) = (find("title"), find("value"), find("page"));
    close(title.x, 0.0, "title at the leading edge");
    close(title.x + title.width, 100.0, "title advance");
    close(value.x + value.width, 300.0, "value ends on stop 300");
    close(page.x + page.width, 560.0, "page ends on stop 560");
}

/// `:137-147`: the START-tab gap is materialised — 'B' sits at the l@100 stop
/// instead of running on at 20.
#[test]
fn c44_start_tab_gap_is_materialised() {
    let lines = wrap(&[text_item("A\tB")], 600.0, &[stop(100.0, TabAlign::Left)], 0.0);
    assert_eq!(lines.len(), 1);
    let all = pieces(&lines[0]);
    let a = all.iter().find(|p| p.text == "A").expect("A drawn");
    let b = all.iter().find(|p| p.text == "B").expect("B drawn");
    close(a.x, 0.0, "A at the leading edge");
    close(b.x, 100.0, "B on the stop, not contiguous at 20");
}

/// `:149-166`: a cell stays ON its absolute stop when the first line carries a
/// positive indent (the resolver's start pen includes the indent).
#[test]
fn c45_cell_stays_on_its_stop_with_a_first_line_indent() {
    let lines = wrap_indented(
        &[text_item("title\tvalue")],
        600.0,
        50.0,
        &[stop(300.0, TabAlign::Right)],
    );
    assert_eq!(lines.len(), 1);
    // The reference reports inShapeX from the text-inset edge, so the first-line
    // indent is part of the line origin (`textXOffset`, `renderer.ts:5890`).
    let all = pieces_at(&lines[0], 50.0);
    let title = all.iter().find(|p| p.text == "title").expect("title drawn");
    let value = all.iter().find(|p| p.text == "value").expect("value drawn");
    close(title.x, 50.0, "first line starts at the indent");
    close(value.x + value.width, 300.0, "cell still ENDS on the stop");
}

// ── rtl-tab-stops.test.ts (issue #831) — reading-frame resolution ──────────

/// `:196-219` case (5), reading frame only: start@100 then A(20) leaves the
/// pen at 120; end@140 with a 40-wide cell cannot reach ⇒ gap 0. This is the
/// frame-independent half of the case; the mirrored canvas x is BLOCKED (no
/// bidi cell mirroring in the paint pass).
#[test]
fn c46_rtl_reading_frame_start_tab_advances_and_end_tab_collapses() {
    let stops = [
        stop(100.0, TabAlign::Left),
        stop(140.0, TabAlign::Right),
    ];
    let mut items = vec![
        tab_item(),
        LineItem::Text {
            text: "A".to_string(),
            style: S,
            width: 20.0,
        },
        tab_item(),
        LineItem::Text {
            text: "12".to_string(),
            style: S,
            width: 40.0,
        },
    ];
    let out = tab_widths(&mut items, &stops, 50.0, 600.0, 0.0, 0.0);
    assert_eq!(out, [50.0, 20.0, 0.0, 40.0]);
    // Reading-frame content width = 50 + 20 + 0 + 40 = 110 (the reference's
    // 600 − 50 − 110 = 440 mirror anchor is a paint-pass concern).
    close(out.iter().sum::<f32>(), 110.0, "reading-frame content width");
}
