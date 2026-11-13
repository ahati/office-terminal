//! Text-layout kernel parity: the reference specs' own numbers.
//!
//! Ports, with the reference's concrete values, from `packages/pptx/src/*.test.ts`
//! (read-only @ `1f5c6e1d`): `text-justify`, `tab-layout`, `tab-stop-spc`,
//! `cjk-wrap`, `ea-line-break`, `latin-hyphen-wrap`, `latin-nonstarter-wrap`,
//! `boundary-glue-lb28-wrap`, `powerpoint-line-metrics`, the two percentage
//! spacing specs and `firstline-indent-autofit`. Every wrap probe uses the
//! reference tests' stub measurer (10 px per code point, 10 px space) and calls
//! the production kernels directly, so a regression in the wrap, the
//! justification gap mask, the tab pen or the line box fails here.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_text_tests.rs.txt`; the
//! ledger keeps their rows.

use kitty_pptx_core::text::justify::{justify_line, JustifyMode, JustifyPlan};
use kitty_pptx_core::text::linebreak::{
    break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem,
};
use kitty_pptx_core::text::metrics::{
    natural_line_box, paragraph_spacing, spaced_line_box, FaceMetrics,
};
use kitty_pptx_core::text::tabs::resolve_tab_widths;
use kitty_pptx_core::text::{StyleId, TabAlign, TabStop, TextMeasure};

const S: StyleId = StyleId(0);

/// The reference probes' `measureText`: `chars * 10`, space 10.
static MEASURE: Len10 = Len10;

struct Len10;

impl TextMeasure for Len10 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 10.0
    }

    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

fn text_item(text: &str) -> BreakItem {
    BreakItem::Text {
        text: text.to_string(),
        style: S,
    }
}

fn line_text(line: &BrokenLine) -> String {
    line.items
        .iter()
        .map(|item| match item {
            LineItem::Text { text, .. } => text.as_str(),
            LineItem::Tab { .. } => "\t",
            LineItem::Object { .. } => "[obj]",
        })
        .collect()
}

fn texts(lines: &[BrokenLine]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line_text(line).trim_end().to_string())
        .collect()
}

fn wrap_lines(items: &[BreakItem], width: f32, indent: f32, east_asian: bool) -> Vec<BrokenLine> {
    let options = BreakOptions {
        max_width: width,
        first_line_indent: indent,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: east_asian,
        measurer: &MEASURE,
    };
    break_paragraph(items, &options)
}

fn wrap_one(text: &str, width: f32) -> Vec<String> {
    texts(&wrap_lines(&[text_item(text)], width, 0.0, true))
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected {expected}, got {actual}"
    );
}

fn item(text: &str, width: f32) -> LineItem {
    LineItem::Text {
        text: text.to_string(),
        style: S,
        width,
    }
}

fn plan(
    items: &[LineItem],
    avail: f32,
    natural: f32,
    mode: JustifyMode,
    last_line: bool,
    ends_with_break: bool,
) -> JustifyPlan {
    justify_line(items, avail, natural, mode, last_line, ends_with_break)
        .unwrap_or_else(|| panic!("expected a justification plan for {items:?}"))
}

// ── text-justify.test.ts ────────────────────────────────────────────────────

/// `:35-38`: three CJK glyphs offer two interior gaps; slack 60 → 30 each,
/// even on the final line under `dist`.
#[test]
fn dist_justifies_the_last_cjk_line() {
    let items = [item("日本語", 60.0)];
    let justified = plan(&items, 120.0, 60.0, JustifyMode::Dist, true, false);
    close(justified.per_gap, 30.0);
    assert_eq!(justified.gap_positions(), [1, 2]);
    assert_eq!(justified.gap_after, [false, true, true, false]);
    assert_eq!(justified.gap_count(), 2);
}

/// `:24-40`: `just` leaves the final line and a `<a:br/>`-closed line ragged;
/// a middle line still justifies.
#[test]
fn just_leaves_the_last_and_break_terminated_lines_ragged() {
    let items = [item("日本語", 60.0)];
    assert_eq!(
        justify_line(&items, 120.0, 60.0, JustifyMode::Just, true, false),
        None
    );
    assert_eq!(
        justify_line(&items, 120.0, 60.0, JustifyMode::Just, false, true),
        None
    );
    let justified = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
    close(justified.per_gap, 30.0);
    assert_eq!(justified.gap_positions(), [1, 2]);
}

/// `:41-53`: `thaiDist` opens every grapheme-cluster boundary of a space-free
/// Thai span, keeping ◌ื glued to ม (gaps before offsets 1, 3, 4); `dist` is
/// not an alias and leaves the line ragged.
#[test]
fn thai_dist_opens_grapheme_clusters() {
    let items = [item("เมือง", 60.0)];
    let justified = plan(&items, 90.0, 60.0, JustifyMode::ThaiDist, false, false);
    close(justified.per_gap, 10.0);
    assert_eq!(justified.gap_positions(), [1, 3, 4]);
    assert_eq!(
        justify_line(&items, 90.0, 60.0, JustifyMode::Dist, false, false),
        None
    );
}

/// `:62-66`: no gap inside the Latin "EC"; gaps at C|市, 市|場, 場|で.
#[test]
fn mixed_latin_cjk_opens_only_cjk_boundaries() {
    let items = [item("EC市場で", 100.0)];
    let justified = plan(&items, 130.0, 100.0, JustifyMode::Just, false, false);
    close(justified.per_gap, 10.0);
    assert_eq!(justified.gap_positions(), [2, 3, 4]);
}

/// `:68-73`: the leading 字下げ whitespace stays fixed; only the two content
/// gaps stretch (slack 30 → 15 each).
#[test]
fn leading_indent_whitespace_opens_no_gap() {
    let items = [item("  日本語", 100.0)];
    let justified = plan(&items, 130.0, 100.0, JustifyMode::Just, false, false);
    close(justified.per_gap, 15.0);
    assert_eq!(justified.gap_positions(), [3, 4]);
}

/// `:55-60`: pure Latin widens its inter-word spaces; gaps land before the
/// following word (offsets 6 and 12), never at line end.
#[test]
fn latin_words_open_inter_word_gaps() {
    let items = [item("Hello world foo", 100.0)];
    let justified = plan(&items, 200.0, 100.0, JustifyMode::Just, false, false);
    close(justified.per_gap, 50.0);
    assert_eq!(justified.gap_positions(), [6, 12]);
}

// ── tab-layout.test.ts / tab-stop-spc.test.ts ──────────────────────────────

fn cell(width: f32) -> LineItem {
    LineItem::Text {
        text: String::from("cell"),
        style: S,
        width,
    }
}

fn tab_item() -> LineItem {
    LineItem::Tab {
        width: 0.0,
        style: S,
    }
}

fn stop(pos: f32, align: TabAlign) -> TabStop {
    TabStop { pos, align }
}

fn tab_widths(
    items: &mut [LineItem],
    stops: &[TabStop],
    start_pen: f32,
    limit: f32,
    no_stop_gap: f32,
    def_tab_sz: f32,
) -> Vec<f32> {
    resolve_tab_widths(items, stops, start_pen, limit, no_stop_gap, def_tab_sz);
    items
        .iter()
        .map(|item| match item {
            LineItem::Text { width, .. }
            | LineItem::Tab { width, .. }
            | LineItem::Object { width, .. } => *width,
        })
        .collect()
}

/// `tab-layout.test.ts:26-36` (#916 N cells): `[50,T,40,T,40]` with right stops
/// at 200 and 400 leaves gaps 110 and 160.
#[test]
fn n_tab_cells_end_on_their_stops() {
    let out = tab_widths(
        &mut [cell(50.0), tab_item(), cell(40.0), tab_item(), cell(40.0)],
        &[
            stop(200.0, TabAlign::Right),
            stop(400.0, TabAlign::Right),
        ],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [50.0, 110.0, 40.0, 160.0, 40.0]);
    close(50.0 + out[1] + 40.0, 200.0);
    close(50.0 + out[1] + 40.0 + out[3] + 40.0, 400.0);
}

/// `tab-layout.test.ts:90-106`: with no reachable stop the §21.1.2.1.1
/// `defTabSz` grid applies; a pen already on a grid line advances to the next.
#[test]
fn default_tab_grid_jumps_to_the_next_line() {
    let out = tab_widths(&mut [tab_item(), cell(30.0)], &[], 0.0, 600.0, 0.0, 100.0);
    assert_eq!(out, [100.0, 30.0]);
    let out = tab_widths(&mut [tab_item(), cell(10.0)], &[], 100.0, 600.0, 0.0, 100.0);
    assert_eq!(out, [100.0, 10.0]);
}

/// `tab-layout.test.ts:64-69` (#835): a cell that would cross the trailing text
/// edge is pinned so its far edge sits on the limit.
#[test]
fn overflow_clamps_the_cell_to_the_trailing_edge() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(700.0, TabAlign::Right)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [560.0, 40.0]);
    close(out[0] + out[1], 600.0);
}

// ── cjk-wrap.test.ts (行頭禁則 / 行末禁則) ─────────────────────────────────

/// `:11-15`: a 20 px line fits "あい", but 、 may not begin a line, so
/// 追い出し retracts the split to one character.
#[test]
fn kinsoku_retracts_a_line_start_forbidden_mark() {
    let lines = wrap_lines(&[text_item("あい、う")], 20.0, 0.0, true);
    assert_eq!(texts(&lines), ["あ", "い、", "う"]);
    assert_eq!(lines[0].width, 10.0);
    assert_eq!(lines[1].width, 20.0);
}

/// `:23-36`: 「 may not end a line, so it is pulled down with い; a legal
/// greedy split and a wholly fitting run are untouched.
#[test]
fn kinsoku_retracts_a_line_end_forbidden_bracket() {
    let lines = wrap_lines(&[text_item("あ「い")], 20.0, 0.0, true);
    assert_eq!(texts(&lines), ["あ", "「い"]);
    assert_eq!(lines[0].width, 10.0);
    assert_eq!(wrap_one("あいうえ", 20.0), ["あい", "うえ"]);
    assert_eq!(wrap_one("あい", 100.0), ["あい"]);
}

// ── ea-line-break.test.ts (§21.1.2.2.7) ────────────────────────────────────

/// E01 (`:63-66`): `eaLnBrk="0"` lifts the line-start/line-end rules — 、
/// freely starts a line and 「 may end one — while the default runs kinsoku.
#[test]
fn ealnbrk_false_lets_cjk_punctuation_end_a_line() {
    assert_eq!(wrap_one("あい、う", 20.0), ["あ", "い、", "う"]);
    assert_eq!(
        texts(&wrap_lines(&[text_item("あい、う")], 20.0, 0.0, false)),
        ["あい", "、う"]
    );
    let split = [text_item("日本語「"), text_item("日本語")];
    assert_eq!(
        texts(&wrap_lines(&split, 20.0, 0.0, false)),
        ["日本", "語「", "日本", "語"]
    );
    assert_eq!(
        texts(&wrap_lines(&split, 20.0, 0.0, true)),
        ["日本", "語", "「日", "本語"]
    );
}

/// E08/E09 (`:67-72`): only `eaLnBrk="0"` breaks after a CJK opening bracket
/// before Latin text.
#[test]
fn ealnbrk_false_breaks_after_an_opening_bracket() {
    let bracket = [text_item("「"), text_item("abc")];
    assert_eq!(texts(&wrap_lines(&bracket, 30.0, 0.0, false)), ["「", "abc"]);
    assert_eq!(texts(&wrap_lines(&bracket, 30.0, 0.0, true)), ["「ab", "c"]);
}

// ── latin-hyphen-wrap.test.ts ──────────────────────────────────────────────

/// `:41-58`: UAX #14 HY/HH breaks after an authored compound hyphen. A 140 px
/// line fits "aaaa bbbb non-" exactly, so PowerPoint needs two lines, not three.
#[test]
fn latin_compound_hyphen_is_a_soft_break() {
    assert_eq!(
        wrap_one("aaaa bbbb non-managed bbbb", 140.0),
        ["aaaa bbbb non-", "managed bbbb"]
    );
    assert_eq!(
        wrap_one("aaaa bbbb non‐managed bbbb", 140.0),
        ["aaaa bbbb non‐", "managed bbbb"]
    );
}

/// `:59-75`: the opportunity survives a formatting-run seam and an option
/// prefix ("--list-").
#[test]
fn hyphen_break_spans_a_run_seam() {
    let seam = [text_item("aaaaaaaa-"), text_item("bbbb")];
    assert_eq!(
        texts(&wrap_lines(&seam, 100.0, 0.0, true)),
        ["aaaaaaaa-", "bbbb"]
    );
    assert_eq!(
        wrap_one("aaaa --list-masters bbbb", 120.0),
        ["aaaa --list-", "masters bbbb"]
    );
}

/// `:77-93`: a numeric range and a word-initial hyphen never break — only a
/// hyphen flanked by Latin on both sides does.
#[test]
fn numeric_range_and_word_initial_hyphen_never_break() {
    assert_eq!(
        wrap_one("aaaa bbbb 2024-2025 bbbb", 140.0),
        ["aaaa bbbb", "2024-2025 bbbb"]
    );
    assert_eq!(
        wrap_one("aaaa bbbb -managed bbbb", 140.0),
        ["aaaa bbbb", "-managed bbbb"]
    );
}

// ── latin-nonstarter-wrap.test.ts (UAX #14 LB13) ───────────────────────────

/// `:46-58`: a closing/mid-punctuation mark has no break opportunity before
/// it, so no line may begin with one (165 px fits "aaaa bbbb system").
#[test]
fn closing_punctuation_never_starts_a_latin_line() {
    for mark in ['!', ')', ',', '.', ':', ';', '?', ']', '}'] {
        let source = format!("aaaa bbbb system{mark} cc");
        let out = wrap_one(&source, 165.0);
        assert_eq!(out.len(), 2, "mark {mark:?}: {out:?}");
        assert_eq!(out[0], "aaaa bbbb", "mark {mark:?}");
        assert_eq!(out[1], format!("system{mark} cc"), "mark {mark:?}");
    }
}

/// `:60-76`: the word may split at the formatting seam ("sys" / "tem" /
/// ", cc"), but the comma still never leads a line.
#[test]
fn non_starters_stay_with_their_word_across_run_seams() {
    let items = [
        text_item("aaaa bbbb sys"),
        text_item("tem"),
        text_item(", cc"),
    ];
    let out = texts(&wrap_lines(&items, 165.0, 0.0, true));
    assert_eq!(out, ["aaaa bbbb", "system, cc"]);
    for line in &out {
        assert!(!line.starts_with(','), "line {line:?} starts with a comma");
    }
}

// ── boundary-glue-lb28-wrap.test.ts ────────────────────────────────────────

/// `:44-60` (LB25 PR × NU): "wwww " + "$" fits 70 px, "100" does not, and the
/// $|1 seam is unbreakable, so "$100" wraps as one unit; the authored space
/// keeps its trailing cell on line 1.
#[test]
fn lb25_glues_a_currency_prefix_to_its_digits() {
    let items = [text_item("wwww "), text_item("$"), text_item("100")];
    let lines = wrap_lines(&items, 70.0, 0.0, true);
    assert_eq!(lines.len(), 2);
    assert_eq!(line_text(&lines[0]), "wwww ");
    assert_eq!(line_text(&lines[1]), "$100");
    assert_eq!(lines[1].width, 40.0);
}

/// `:62-78` (LB14 OP SP* ×): "(" keeps its following digits on the same line.
#[test]
fn lb14_glues_an_opening_bracket_to_its_digits() {
    let items = [text_item("wwww "), text_item("("), text_item("2026")];
    let lines = wrap_lines(&items, 70.0, 0.0, true);
    assert_eq!(line_text(&lines[0]), "wwww ");
    assert_eq!(line_text(&lines[1]), "(2026");
}

// ── powerpoint-line-metrics.test.ts (#1610) ────────────────────────────────

/// Arial: usWinAscent 1854 / usWinDescent 434 → 1854 / 2288.
const ARIAL: FaceMetrics = FaceMetrics {
    ascent_share: 1854.0 / 2288.0,
};

/// Calibri / Calibri Light: usWinAscent 1950 / usWinDescent 550 → 1950 / 2500.
const CALIBRI: FaceMetrics = FaceMetrics {
    ascent_share: 1950.0 / 2500.0,
};

/// The export snaps a continuous baseline to the nearest 1/100 in, i.e. 0.72
/// document units at the specs' 1 pt = 1 px scale (`:14-16, 60-67`).
const EXPORT_UNIT: f32 = 0.72;

fn snap(continuous: f32) -> i32 {
    (continuous / EXPORT_UNIT + 0.5).floor() as i32
}

fn baseline_units(size: f32, count: usize, face: FaceMetrics) -> Vec<i32> {
    let (ascent, descent) = natural_line_box(&[(size, face)]);
    (0..count)
        .map(|i| snap(ascent + i as f32 * (ascent + descent)))
        .collect()
}

/// `:70-73, 102-109`: each run claims 1.2 × its size split by the face's ascent
/// share; the first two baselines are 97 / 217 (72 pt Arial), 135 / 302 (100 pt
/// Arial) and 94 / 214 (72 pt Calibri).
#[test]
fn powerpoint_natural_box_baselines() {
    assert_eq!(baseline_units(72.0, 2, ARIAL), [97, 217]);
    assert_eq!(baseline_units(100.0, 2, ARIAL), [135, 302]);
    assert_eq!(baseline_units(72.0, 2, CALIBRI), [94, 214]);

    let (ascent, descent) = natural_line_box(&[(72.0, ARIAL)]);
    close(ascent + descent, 86.4);
    close(ascent, 86.4 * 1854.0 / 2288.0);
    let (ascent, descent) = natural_line_box(&[(72.0, CALIBRI)]);
    close(ascent + descent, 86.4);
    close(ascent, 86.4 * 1950.0 / 2500.0);
}

// ── percentage-line-spacing.test.ts ────────────────────────────────────────

/// `:105-110` + `powerpoint-line-metrics.test.ts:267`: `lnSpc` percentages
/// multiply the natural 1.2 × size box, never the font design height: 80 % of a
/// 20 px line is 19.2, not 24.
#[test]
fn percentage_line_spacing_scales_the_natural_box() {
    let (ascent, descent) = natural_line_box(&[(20.0, ARIAL)]);
    close(ascent + descent, 24.0);
    let (ascent, descent) = spaced_line_box(ascent, descent, Some((true, 80_000.0)), 1.0, 0.0);
    close(ascent + descent, 19.2);
}

/// `powerpoint-line-metrics.test.ts:135-136`: `spcPts` rounds half up to whole
/// points — 40.5 pt lays out as 41 pt and the eight 40 pt Arial baselines step
/// by 57 export units; `spcPts` 18 stays 18 whatever the font box.
#[test]
fn spc_pts_rounds_to_whole_points() {
    let (ascent, descent) = natural_line_box(&[(40.0, ARIAL)]);
    let boxed = spaced_line_box(ascent, descent, Some((false, 40.5)), 1.0, 0.0);
    close(boxed.0 + boxed.1, 41.0);
    assert_eq!(
        boxed,
        spaced_line_box(ascent, descent, Some((false, 41.0)), 1.0, 0.0)
    );
    let ys: Vec<i32> = (0..8)
        .map(|i| snap(boxed.0 + i as f32 * (boxed.0 + boxed.1)))
        .collect();
    assert_eq!(ys, [44, 101, 158, 215, 272, 329, 386, 443]);

    let (ascent, descent) = natural_line_box(&[(20.0, ARIAL)]);
    let (ascent, descent) = spaced_line_box(ascent, descent, Some((false, 18.0)), 1.0, 0.0);
    close(ascent + descent, 18.0);
}

// ── percentage-paragraph-spacing.test.ts ───────────────────────────────────

/// `:139-152, 173-181, 212-218`: 50 % `spcBef` before a 20 pt line adds 12 to
/// the 24 px pitch; before a 40 pt line it adds 24 — the *following* line's
/// natural height, even when that paragraph's own `lnSpc` is 80 % (38.4 px box).
/// `spcPts` is absolute and an omitted `spcBef` adds nothing.
#[test]
fn paragraph_spacing_follows_the_adjacent_line() {
    let pitch = 1.2 * 20.0;
    close(pitch, 24.0);
    let gap = paragraph_spacing(Some((true, 50_000.0)), pitch, 1.0);
    close(gap, 12.0);
    close(pitch + gap, 36.0); // the reference's pitch * 1.5
    close(
        paragraph_spacing(Some((true, 50_000.0)), 1.2 * 40.0, 1.0),
        24.0,
    );
    let (ascent, descent) = natural_line_box(&[(40.0, ARIAL)]);
    let (ascent, descent) = spaced_line_box(ascent, descent, Some((true, 80_000.0)), 1.0, 0.0);
    close(ascent + descent, 38.4);
    close(
        paragraph_spacing(Some((true, 100_000.0)), 1.2 * 40.0, 1.0),
        48.0,
    );
    close(paragraph_spacing(Some((false, 12.0)), 1.2 * 20.0, 1.0), 12.0);
    close(paragraph_spacing(Some((false, 12.0)), 1.2 * 40.0, 1.0), 12.0);
    close(paragraph_spacing(None, 1.2 * 20.0, 1.0), 0.0);
}

// ── firstline-indent-autofit.test.ts (§21.1.2.2.7) ─────────────────────────

/// `:158-167`: a negative indent extends only the first wrap line. `marL` 50 +
/// `indent` −50 in a 200 px box is `max_width` 150 with `first_line_indent`
/// −50, so 40 kana break 20 / 15 / 5 at 10 px each.
#[test]
fn negative_first_line_indent_expands_only_the_first_line() {
    let kana = "あ".repeat(40);
    let lines = wrap_lines(&[text_item(&kana)], 150.0, -50.0, true);
    let lengths: Vec<usize> = lines
        .iter()
        .map(|line| line_text(line).chars().count())
        .collect();
    assert_eq!(lengths, [20, 15, 5]);
    assert_eq!(
        lines.iter().map(|line| line.width).collect::<Vec<f32>>(),
        [200.0, 150.0, 50.0]
    );
}

/// `tab.ts:43-52`: a reachable explicit stop always wins over `defTabSz` — the
/// right stop at 250 ends the 40 px cell there instead of on the 300 px grid
/// line, and a left stop at 80 beats the 100 px grid.
#[test]
fn explicit_stop_wins_over_the_default_grid() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(250.0, TabAlign::Right)],
        0.0,
        600.0,
        0.0,
        100.0,
    );
    assert_eq!(out, [210.0, 40.0]);
    close(out[0] + out[1], 250.0);
    let out = tab_widths(
        &mut [tab_item(), cell(30.0)],
        &[stop(80.0, TabAlign::Left)],
        0.0,
        600.0,
        0.0,
        100.0,
    );
    assert_eq!(out, [80.0, 30.0]);
}
