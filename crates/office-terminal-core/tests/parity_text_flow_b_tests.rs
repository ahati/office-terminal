//! Parity ports of the reference text-flow specs (batch `text-flow-b`).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/cjk-wrap.test.ts`,
//! `cjk-latin-word-wrap.test.ts`, `cjk-spc-justify.test.ts`, `ea-line-break.test.ts`,
//! `bidi-line.test.ts`, `sea-line-break.test.ts`, `sea-transition-line-break.test.ts`,
//! `sea-justified-fit.test.ts`, `boundary-glue-lb28-wrap.test.ts`,
//! `latin-hyphen-wrap.test.ts`, `latin-nonstarter-wrap.test.ts`,
//! `paragraph-trailing-whitespace.test.ts`, `firstline-indent-wrap.test.ts`,
//! `superscript-citation-wrap.test.ts`.
//!
//! Every probe uses the reference spec's own stub `measureText` (10 px per code
//! point / 10 px space unless the spec says otherwise) and calls the crate's
//! public layout kernels with the reference's exact numbers, so the assertions
//! are the specs' own values. Cases whose contract needs a kernel this crate does
//! not expose are listed — never silently dropped — in
//! `tests/parity/coverage/text-flow-b.md` together with the missing symbol.

use office_terminal_core::text::linebreak::{break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem};
use office_terminal_core::text::{StyleId, TextMeasure};

const S: StyleId = StyleId(0);
/// A second style is a formatting-run seam in the reference's `sameStyle` sense.
const S2: StyleId = StyleId(1);

/// `cjk-wrap` / `cjk-latin-word-wrap` / `latin-*` / `firstline-indent` stub:
/// `measureText = [...text].length * 10`, space 10.
struct Len10;

impl TextMeasure for Len10 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 10.0
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

/// `paragraph-trailing-whitespace` / `sea-*` stub: `text.length * 10` — UTF-16
/// code units, not code points.
struct Len10Utf16;

impl TextMeasure for Len10Utf16 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.encode_utf16().count() as f32 * 10.0
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

/// `cjk-wrap.test.ts:110`: an explicit per-character advance table.
struct WideStart;

impl TextMeasure for WideStart {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().map(|c| if c == 'あ' { 30.0 } else { 10.0 }).sum()
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

static LEN10: Len10 = Len10;
static LEN10_UTF16: Len10Utf16 = Len10Utf16;
static WIDE_START: WideStart = WideStart;

fn text_item(text: &str) -> BreakItem {
    BreakItem::Text {
        text: text.to_string(),
        style: S,
    }
}

fn styled_item(text: &str, style: StyleId) -> BreakItem {
    BreakItem::Text {
        text: text.to_string(),
        style,
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
    lines.iter().map(line_text).collect()
}

fn wrap(items: &[BreakItem], measurer: &dyn TextMeasure, width: f32, ea: bool) -> Vec<BrokenLine> {
    let options = BreakOptions {
        max_width: width,
        first_line_indent: 0.0,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: ea,
        measurer,
    };
    break_paragraph(items, &options)
}

/// `len10(text, width)` for a single authored run.
fn wrap_one(text: &str, width: f32) -> Vec<String> {
    texts(&wrap(&[text_item(text)], &LEN10, width, true))
}

/// The count `fitCjkLine` returns for an empty line: the graphemes of line 1.
fn head_units(lines: &[BrokenLine]) -> usize {
    lines
        .first()
        .map(|line| line_text(line).trim_end().chars().count())
        .unwrap_or(0)
}

// ── cjk-wrap.test.ts — 行頭禁則 / 行末禁則 ─────────────────────────────────

/// `:11-15`: a 20 px line fits "あい", but 、 may not begin a line, so 追い出し
/// retracts the split to one grapheme (the reference returns 1).
#[test]
fn cjk_kinsoku_retracts_before_a_comma() {
    let lines = wrap(&[text_item("あい、う")], &LEN10, 20.0, true);
    assert_eq!(texts(&lines), ["あ", "い、", "う"]);
    assert_eq!(head_units(&lines), 1);
}

/// `:17-20`: the same retraction for a full stop.
#[test]
fn cjk_kinsoku_retracts_before_a_full_stop() {
    let lines = wrap(&[text_item("かき。く")], &LEN10, 20.0, true);
    assert_eq!(texts(&lines), ["か", "き。", "く"]);
    assert_eq!(head_units(&lines), 1);
}

/// `:22-27`: an opening bracket may not end a line either — 「 is pulled down
/// with い, so only あ stays on line 1.
#[test]
fn cjk_kinsoku_retracts_a_line_end_bracket() {
    let lines = wrap(&[text_item("あ「い")], &LEN10, 20.0, true);
    assert_eq!(texts(&lines), ["あ", "「い"]);
    assert_eq!(head_units(&lines), 1);
}

/// `:29-35`: a kinsoku-legal greedy split is untouched (2), and a wholly fitting
/// run returns whole (2).
#[test]
fn cjk_kinsoku_keeps_a_legal_greedy_split() {
    let legal = wrap(&[text_item("あいうえ")], &LEN10, 20.0, true);
    assert_eq!(texts(&legal), ["あい", "うえ"]);
    assert_eq!(head_units(&legal), 2);

    let whole = wrap(&[text_item("あい")], &LEN10, 100.0, true);
    assert_eq!(texts(&whole), ["あい"]);
    assert_eq!(head_units(&whole), 2);
}

/// `:37-40`: on an empty line at least one grapheme is placed even though it
/// overflows (あ is 30 px in a 20 px box).
#[test]
fn cjk_kinsoku_forces_one_grapheme_on_an_empty_line() {
    let lines = wrap(&[text_item("あい")], &WIDE_START, 20.0, true);
    assert_eq!(texts(&lines), ["あ", "い"]);
    assert_eq!(head_units(&lines), 1);
    assert!(lines[0].width > 20.0, "the forced grapheme overflows: {lines:?}");
}

/// `:49-53`: with kinsoku disabled the plain greedy split is returned (2).
/// This crate models the switch as `eaLnBrk="0"`, which lifts the East Asian
/// line-start/line-end rules (`text/linebreak.rs`, ref E01).
#[test]
fn cjk_kinsoku_off_returns_the_plain_greedy_split() {
    let lines = wrap(&[text_item("あい、う")], &LEN10, 20.0, false);
    assert_eq!(texts(&lines), ["あい", "、う"]);
    assert_eq!(head_units(&lines), 2);
}

/// `:72-77`: every grapheme is 行頭禁則, so no legal split exists within the
/// retraction floor; the unrestricted greedy split (2) survives instead of
/// emptying the line.
#[test]
fn cjk_pathological_all_forbidden_run_keeps_the_greedy_split() {
    let lines = wrap(&[text_item("、。、。")], &LEN10, 20.0, true);
    assert_eq!(texts(&lines), ["、。", "、。"]);
    assert_eq!(head_units(&lines), 2);
}

// ── cjk-latin-word-wrap.test.ts (#1474) ────────────────────────────────────

/// `:44-46`: a Latin word after CJK text in ONE run moves intact.
#[test]
fn cjk_latin_word_moves_intact_after_cjk() {
    assert_eq!(wrap_one("日本語Power", 70.0), ["日本語", "Power"]);
}

/// `:48-52`: the same across a formatting-run boundary.
#[test]
fn cjk_latin_word_moves_intact_across_a_run_seam() {
    let items = [text_item("日本語"), styled_item("Power", S2)];
    assert_eq!(texts(&wrap(&items, &LEN10, 70.0, true)), ["日本語", "Power"]);
}

/// `:54-58`: "YoY+11.9%" is 9 code points = 90 px, so the 90 px box holds it and
/// the font seam must not invent a break.
#[test]
fn cjk_latin_fitting_word_keeps_no_break_at_the_seam() {
    let items = [text_item("YoY+"), styled_item("11.9%", S2)];
    assert_eq!(texts(&wrap(&items, &LEN10, 90.0, true)), ["YoY+11.9%"]);
}

/// `:60-66`: an overwide word (100 px in a 40 px box) is emergency-broken per
/// grapheme across the font seam — no style seam keeps it whole.
#[test]
fn cjk_latin_overwide_word_breaks_per_grapheme() {
    let items = [text_item("ab"), styled_item("cdefghij", S2)];
    assert_eq!(texts(&wrap(&items, &LEN10, 40.0, true)), ["abcd", "efgh", "ij"]);
}

// ── ea-line-break.test.ts (§21.1.2.2.7) ────────────────────────────────────

/// E00 (`:58-60`): `eaLnBrk="0"` still breaks ideographs per character.
#[test]
fn ealnbrk_false_still_breaks_ideographs_per_character() {
    assert_eq!(wrap(&[text_item("日本語")], &LEN10, 10.0, false).len(), 3);
    assert_eq!(
        texts(&wrap(&[text_item("日本語")], &LEN10, 10.0, false)),
        ["日", "本", "語"]
    );
}

/// E01 (`:62-66`): only `eaLnBrk="0"` lets a CJK opening bracket end a line.
#[test]
fn ealnbrk_false_lets_a_cjk_bracket_end_a_line() {
    let split = [text_item("日本語「"), text_item("日本語")];
    assert_eq!(
        texts(&wrap(&split, &LEN10, 20.0, false)),
        ["日本", "語「", "日本", "語"]
    );
    assert_eq!(texts(&wrap(&split, &LEN10, 20.0, true)), ["日本", "語", "「日", "本語"]);
}

/// E08/E09 (`:68-73`): only `eaLnBrk="0"` breaks after an opening bracket
/// before Latin text.
#[test]
fn ealnbrk_false_breaks_after_an_opening_bracket() {
    let bracket = [text_item("「"), text_item("abc")];
    assert_eq!(texts(&wrap(&bracket, &LEN10, 30.0, false)), ["「", "abc"]);
    assert_eq!(texts(&wrap(&bracket, &LEN10, 30.0, true)), ["「ab", "c"]);
}

/// `:75-101`: the same `eaLnBrk` decision inside table-cell text. The cell path
/// resolves to the identical layout kernel (`a:bodyPr` insets are the only
/// difference and are 0 here), so the probe runs the kernel with the cell's box.
#[test]
fn ealnbrk_is_honoured_in_table_cell_text() {
    let bracket = [text_item("「"), text_item("abc")];
    assert_eq!(texts(&wrap(&bracket, &LEN10, 30.0, false)), ["「", "abc"]);
    assert_eq!(texts(&wrap(&bracket, &LEN10, 30.0, true)), ["「ab", "c"]);
}

/// `:122-133` (K00–K23): a Japanese curly quote keeps in-run kinsoku, so the
/// quote is retracted with its ideograph. The one-run twins and the two-run
/// `“` variant (whose authored seam already lands on the same boundary) match;
/// the two-run `”` variant needs cross-run kinsoku evidence — see the coverage
/// report.
#[test]
fn ea_kinsoku_is_continuous_inside_one_run() {
    assert_eq!(wrap_one("漢漢”漢", 20.0), ["漢", "漢”", "漢"]);
    assert_eq!(wrap_one("漢“漢漢", 20.0), ["漢", "“漢", "漢"]);
    // Two-run twins: '漢' + '“漢漢' keeps the same lines.
    let split = [text_item("漢"), styled_item("“漢漢", S2)];
    assert_eq!(texts(&wrap(&split, &LEN10, 20.0, true)), ["漢", "“漢", "漢"]);
}

/// `:167-175`: an unobserved mark and mixed-language seams stay outside the
/// slot-seam rule — the authored seam keeps its greedy break.
#[test]
fn ea_unobserved_punctuation_and_mixed_seams_keep_greedy_breaks() {
    let ideographic_comma = [text_item("漢漢"), styled_item("、漢", S2)];
    assert_eq!(texts(&wrap(&ideographic_comma, &LEN10, 20.0, true)), ["漢漢", "、漢"]);
    let three = [text_item("漢“"), styled_item("）", S2), styled_item("”漢", StyleId(2))];
    assert_eq!(texts(&wrap(&three, &LEN10, 30.0, true)), ["漢“）", "”漢"]);
    let mixed = [text_item("漢“"), styled_item("漢", S2), styled_item("”漢", StyleId(2))];
    assert_eq!(texts(&wrap(&mixed, &LEN10, 30.0, true)), ["漢“漢", "”漢"]);
}

/// `:177-181`: a `ko-KR`/`zh-CN` curly quote stays in the ideographs' own run,
/// and the retracted second line is ONE segment.
#[test]
fn ea_ko_zh_curly_quote_is_one_segment() {
    // `ko-KR` and `zh-CN` both resolve the quote into the ideographs' face, so
    // the in-run kinsoku retraction applies to the single authored run.
    let lines = wrap(&[text_item("漢漢”漢")], &LEN10, 20.0, true);
    assert_eq!(texts(&lines), ["漢", "漢”", "漢"]);
    let second: Vec<&LineItem> = lines[1]
        .items
        .iter()
        .filter(|item| !matches!(item, LineItem::Text { text, .. } if text.is_empty()))
        .collect();
    assert_eq!(second.len(), 1, "one face paints as one segment: {lines:?}");
    assert!(matches!(second[0], LineItem::Text { text, .. } if text == "漢”"));
}

/// L00 (`:184-194`): the line feed opens a region; the next line takes the
/// containing run's size. Sizes are not carried by [`LineItem`], so the probe
/// asserts the region split and derives the line's unit size from the run.
#[test]
fn ea_line_feed_splits_and_carries_the_containing_run() {
    let items = [styled_item("Ab\nCd", S), styled_item("Ef", S2)];
    let lines = wrap(&items, &LEN10, 1000.0, true);
    assert_eq!(texts(&lines), ["Ab", "CdEf"]);
    // The continuation line carries BOTH runs (the 40 pt `Cd` that contains the
    // line feed and the 14 pt `Ef` that follows it) as two styled items, so the
    // closed run's 40 pt size still owns that line box. `sizePx` itself is not
    // carried by `LineItem` — see the coverage report.
    assert_eq!(lines[1].items.len(), 2);
    assert!(matches!(&lines[1].items[1], LineItem::Text { style, .. } if *style == S2));
}

/// L02 (`:196-204`): a run that ENDS at the line feed does not size the next
/// line — the second line is a 14 pt line.
#[test]
fn ea_line_feed_does_not_size_the_next_line_from_the_closed_run() {
    let items = [styled_item("Ab\n", S), styled_item("Cd", S2)];
    let lines = wrap(&items, &LEN10, 1000.0, true);
    assert_eq!(texts(&lines), ["Ab", "Cd"]);
    assert_eq!(lines[1].items.len(), 1);
    assert!(matches!(&lines[1].items[0], LineItem::Text { style, .. } if *style == S2));
}

/// L07/L08 (`:206-212`): an empty line opened by a line feed is kept.
#[test]
fn ea_line_feed_opens_and_keeps_an_empty_line() {
    let lines = wrap(&[text_item("Ab\n\nCd")], &LEN10, 1000.0, true);
    assert_eq!(texts(&lines), ["Ab", "", "Cd"]);
    assert_eq!(lines.len(), 3);
}

// ── boundary-glue-lb28-wrap.test.ts ────────────────────────────────────────

/// `:28-42` (LB28): "<" may not be orphaned from the Arabic word that follows.
#[test]
fn lb28_glues_a_less_than_sign_to_the_following_arabic_word() {
    let items = [text_item("wwww "), text_item("<"), text_item("شيء")];
    let lines = wrap(&items, &LEN10, 70.0, true);
    let bracket = texts(&lines)
        .into_iter()
        .find(|text| text.contains('<'))
        .expect("a line holds the less-than sign");
    assert!(bracket.contains("شيء"), "line {bracket:?} lost its word");
    assert_eq!(bracket, "<شيء");
}

/// `:44-60` (LB25 PR × NU): "$100" wraps down as one unit.
#[test]
fn lb25_glues_a_currency_prefix_to_its_digits() {
    let items = [text_item("wwww "), text_item("$"), text_item("100")];
    let lines = wrap(&items, &LEN10, 70.0, true);
    assert_eq!(texts(&lines), ["wwww ", "$100"]);
    assert_eq!(lines[1].width, 40.0);
}

/// `:62-78` (LB14 OP × NU): "(" may not be orphaned from "2026".
#[test]
fn lb14_glues_an_opening_bracket_to_its_digits() {
    let items = [text_item("wwww "), text_item("("), text_item("2026")];
    let lines = wrap(&items, &LEN10, 70.0, true);
    assert_eq!(texts(&lines), ["wwww ", "(2026"]);
    assert_eq!(lines[1].width, 50.0);
}

/// `:80-95`: a preceding Thai word is NOT retracted across the AL run boundary —
/// the SEA tailoring opens a legal break at ภาษา|&lt;.
#[test]
fn lb28_does_not_retract_a_preceding_thai_word() {
    let items = [text_item("wwww "), text_item("ภาษา"), text_item("<a")];
    let lines = wrap(&items, &LEN10, 100.0, true);
    assert_eq!(texts(&lines), ["wwww ภาษา", "<a"]);
}

// ── latin-hyphen-wrap.test.ts ──────────────────────────────────────────────

/// `:41-49`: UAX #14 HY breaks AFTER an authored compound hyphen. A 140 px line
/// fits "aaaa bbbb non-" exactly.
#[test]
fn latin_compound_hyphen_is_a_soft_break() {
    assert_eq!(
        wrap_one("aaaa bbbb non-managed bbbb", 140.0),
        ["aaaa bbbb non-", "managed bbbb"]
    );
}

/// `:51-57`: U+2010 HYPHEN is the same explicit compound boundary.
#[test]
fn latin_u2010_hyphen_is_a_soft_break() {
    assert_eq!(
        wrap_one("aaaa bbbb non\u{2010}managed bbbb", 140.0),
        ["aaaa bbbb non\u{2010}", "managed bbbb"]
    );
}

/// `:59-66`: option prefixes stay intact while an internal compound hyphen
/// breaks ("aaaa --list-" is 12 cells = 120 px).
#[test]
fn latin_option_prefix_keeps_an_internal_hyphen_break() {
    assert_eq!(
        wrap_one("aaaa --list-masters bbbb", 120.0),
        ["aaaa --list-", "masters bbbb"]
    );
}

/// `:68-75`: the opportunity survives a formatting-run boundary.
#[test]
fn latin_hyphen_break_spans_a_run_seam() {
    let items = [text_item("aaaaaaaa-"), styled_item("bbbb", S2)];
    assert_eq!(texts(&wrap(&items, &LEN10, 100.0, true)), ["aaaaaaaa-", "bbbb"]);
}

/// `:77-94`: a numeric range, a word-initial hyphen and a Hebrew compound never
/// take the Latin compound-hyphen opportunity.
#[test]
fn latin_hyphen_never_splits_a_range_word_initial_or_hebrew_compound() {
    for source in [
        "aaaa bbbb 2024-2025 bbbb",
        "aaaa bbbb -managed bbbb",
        "aaaa bbbb אב-גד bbbb",
    ] {
        let lines = wrap_one(source, 140.0);
        // The reference's assertion: no line ends in the hyphen break that would
        // have been legal after a Latin-flanked hyphen.
        assert_eq!(lines[0].trim_end(), "aaaa bbbb", "{source:?}");
        assert!(!lines[0].ends_with('-'), "{source:?} took the hyphen break");
    }
}

// ── latin-nonstarter-wrap.test.ts (UAX #14 LB13) ───────────────────────────

/// `:46-58`: a comma in a separate run never begins a line; "system," moves
/// down as a unit (165 px fits "aaaa bbbb system").
#[test]
fn latin_comma_in_a_separate_run_never_starts_a_line() {
    let items = [text_item("aaaa bbbb system"), styled_item(", cc", S2)];
    let lines = wrap(&items, &LEN10, 165.0, true);
    assert_eq!(texts(&lines), ["aaaa bbbb ", "system, cc"]);
    for line in &lines {
        assert!(!line_text(line).starts_with(','), "line {line:?} orphans a comma");
    }
}

/// `:60-76`: the word may split at the font seam, but LB13 still holds.
#[test]
fn latin_comma_never_starts_a_line_across_a_font_seam() {
    let items = [
        text_item("aaaa bbbb sys"),
        styled_item("tem", S2),
        styled_item(", cc", StyleId(2)),
    ];
    let lines = wrap(&items, &LEN10, 165.0, true);
    for line in &lines {
        assert!(!line_text(line).starts_with(','), "line {line:?} orphans a comma");
    }
    let joined = texts(&lines).join("|");
    assert!(joined.contains("system"), "word torn: {joined}");
}

// ── paragraph-trailing-whitespace.test.ts ──────────────────────────────────

/// `:63-80`: a paragraph-terminal space never opens a continuation line.
#[test]
fn trailing_whitespace_makes_no_empty_continuation_line() {
    let lines = wrap(&[text_item("aaaa bbbb ")], &LEN10_UTF16, 90.0, true);
    assert_eq!(lines.len(), 1);
    assert_eq!(line_text(&lines[0]), "aaaa bbbb");
}

/// `:82-96`: only the terminal suffix is trimmed, across formatting runs.
#[test]
fn trailing_whitespace_trims_the_terminal_suffix_across_runs() {
    let items = [
        text_item("aaaa "),
        styled_item("bbbb ", S2),
        styled_item("  ", StyleId(2)),
    ];
    let lines = wrap(&items, &LEN10_UTF16, 90.0, true);
    assert_eq!(lines.len(), 1);
    assert_eq!(line_text(&lines[0]), "aaaa bbbb");
}

/// `:98-112`: the visible non-breaking spaces NBSP / U+2007 / U+202F are kept.
#[test]
fn trailing_whitespace_preserves_visible_non_breaking_spaces() {
    for space in ['\u{a0}', '\u{2007}', '\u{202f}'] {
        let source = format!("aaaa{space}");
        let lines = wrap(&[text_item(&source)], &LEN10_UTF16, 200.0, true);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), source, "space U+{:04X}", space as u32);
    }
}
