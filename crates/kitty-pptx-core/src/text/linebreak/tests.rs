//! Wrap-control tests for [`super::break_paragraph`] with a `len * 10` advance.

use super::*;

static MEASURE: Len10 = Len10;

/// `len * 10`: one advance unit per code point, CJK included (E00-E12 probes).
struct Len10;
impl TextMeasure for Len10 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 10.0
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

const S: StyleId = StyleId(0);

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

fn wrap_full(parts: &[&str], width: f32, tab_size: f32, east_asian: bool) -> Vec<String> {
    let items: Vec<BreakItem> = parts
        .iter()
        .map(|text| BreakItem::Text {
            text: (*text).to_string(),
            style: S,
        })
        .collect();
    let options = BreakOptions {
        max_width: width,
        first_line_indent: 0.0,
        tab_stops: &[],
        default_tab_size: tab_size,
        east_asian_line_break: east_asian,
        measurer: &MEASURE,
    };
    break_paragraph(&items, &options)
        .iter()
        .map(|line| line_text(line).trim_end().to_string())
        .collect()
}

fn wrap(parts: &[&str], width: f32) -> Vec<String> {
    wrap_full(parts, width, 0.0, true)
}

#[test]
fn wraps_at_the_office_control_boundaries() {
    // ref break.test.ts:29-45 (C00/C02/C03/C04/C07) with a len*10 advance.
    assert_eq!(wrap(&["abcdef"], 45.0), ["abcd", "ef"]);
    assert_eq!(wrap(&["abc def"], 50.0), ["abc", "def"]);
    assert_eq!(wrap(&["日本語Power"], 70.0), ["日本語", "Power"]);
    assert_eq!(wrap(&["non-managed"], 70.0), ["non-", "managed"]);
    assert_eq!(wrap(&["abc ", "$", "100"], 55.0), ["abc", "$100"]);
}

#[test]
fn retracts_kinsoku_at_cjk_seams() {
    // 行頭禁則 追い出し: the greedy split "あい" would orphan "、".
    assert_eq!(wrap(&["あい、う"], 20.0), ["あ", "い、", "う"]);
    // 行末禁則: "「" may not end a line, so it is pulled down.
    assert_eq!(wrap(&["あ「い"], 20.0), ["あ", "「い"]);
    // eaLnBrk="0" lifts the rules (E01): the greedy split stands.
    assert_eq!(wrap_full(&["あい、う"], 20.0, 0.0, false), ["あい", "、う"]);
    // An authored run seam keeps its greedy break (C08): kinsoku does not retract
    // across runs without the host's evidence.
    assert_eq!(wrap(&["漢漢", "、漢"], 20.0), ["漢漢", "、漢"]);
    assert_eq!(wrap(&["漢漢、漢"], 20.0), ["漢", "漢、", "漢"]);
}

#[test]
fn carries_an_overflowing_tab_to_the_next_line() {
    // ref break.test.ts:63-66 (C11): the tab seats its cell at the 72 stop.
    assert_eq!(
        wrap_full(&["abc\tdef"], 85.0, 72.0, true),
        ["abc", "\td", "ef"]
    );
}

#[test]
fn breaks_after_a_latin_hyphen_but_never_before_it() {
    assert_eq!(wrap(&["non-managed"], 70.0), ["non-", "managed"]);
    // No break before the hyphen: "non-" stays whole when it does not fit.
    assert_eq!(wrap(&["non-managed"], 40.0), ["non-", "mana", "ged"]);
}

#[test]
fn nbsp_binds_the_adjacent_words() {
    // ref nbsp-fit.test.ts T00-T03 and break.test.ts:56 (C10).
    assert_eq!(wrap(&["abc\u{a0}def"], 200.0), ["abc\u{a0}def"]);
    assert_eq!(wrap(&["abc\u{a0}def"], 50.0), ["abc\u{a0}d", "ef"]);
    assert_eq!(wrap(&["abc def"], 50.0), ["abc", "def"]);
}

#[test]
fn breaks_an_overwide_word_at_grapheme_boundaries() {
    // ref break.test.ts:59-61 (C09).
    assert_eq!(
        wrap(&["supercalifragilistic"], 80.0),
        ["supercal", "ifragili", "stic"]
    );
    // A base + combining mark is one atom, so "a◌́" is never torn apart.
    assert_eq!(
        wrap(&["a\u{301}ba\u{301}b"], 25.0),
        ["a\u{301}", "b", "a\u{301}", "b"]
    );
}

#[test]
fn u200b_is_always_an_opportunity() {
    assert_eq!(wrap(&["ab\u{200b}cd"], 30.0), ["ab\u{200b}", "cd"]);
}

#[test]
fn authored_breaks_and_zero_width_runs() {
    let items = vec![
        BreakItem::Text {
            text: String::new(),
            style: S,
        },
        BreakItem::Text {
            text: "abc".to_string(),
            style: S,
        },
        BreakItem::LineBreak,
        BreakItem::Text {
            text: String::new(),
            style: S,
        },
        BreakItem::Text {
            text: "de f".to_string(),
            style: S,
        },
    ];
    let options = BreakOptions {
        max_width: 200.0,
        first_line_indent: 0.0,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer: &MEASURE,
    };
    let lines = break_paragraph(&items, &options);
    let texts: Vec<String> = lines
        .iter()
        .map(|line| line_text(line).trim_end().to_string())
        .collect();
    assert_eq!(texts, ["abc", "de f"]);
    assert!(lines[0].ends_with_break && !lines[0].starts_after_break);
    assert!(!lines[1].ends_with_break && lines[1].starts_after_break);
}

#[test]
fn ealnbrk_lifts_the_east_asian_rules() {
    // E00/E01/E08/E09 (ref ea-line-break.test.ts): eaLnBrk="0" still breaks
    // ideographs per character but lets a bracket end a line.
    assert_eq!(wrap_full(&["日本語"], 10.0, 0.0, false), ["日", "本", "語"]);
    assert_eq!(
        wrap_full(&["日本語「", "日本語"], 20.0, 0.0, false),
        ["日本", "語「", "日本", "語"]
    );
    assert_eq!(
        wrap(&["日本語「", "日本語"], 20.0),
        ["日本", "語", "「日", "本語"]
    );
    assert_eq!(wrap_full(&["「abc"], 30.0, 0.0, false), ["「", "abc"]);
    assert_eq!(wrap(&["「abc"], 30.0), ["「ab", "c"]);
}

#[test]
fn seats_a_tab_on_an_explicit_stop() {
    // drawingml-text/tab.ts: a left stop at 50 puts the cell head at 50.
    let stops = [TabStop {
        pos: 50.0,
        align: TabAlign::Left,
    }];
    let items = [BreakItem::Text {
        text: "ab\tcd".to_string(),
        style: S,
    }];
    let options = BreakOptions {
        max_width: 200.0,
        first_line_indent: 0.0,
        tab_stops: &stops,
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer: &MEASURE,
    };
    let lines = break_paragraph(&items, &options);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].width, 70.0);
}

#[test]
fn bounds_work_for_a_long_unbreakable_word() {
    // ref break.test.ts:72-84 (#2039): the exponential fit probe keeps a
    // one-grapheme box linear instead of measuring every prefix.
    let text = "a".repeat(3200);
    assert_eq!(wrap(&[text.as_str()], 1.0).len(), 3200);
}

#[test]
fn terminal_spaces_do_not_open_a_tail_line() {
    // ref break.test.ts:38-42 (C05/C06).
    assert_eq!(wrap(&["abc  "], 35.0), ["abc"]);
    assert_eq!(wrap(&["abc", "  "], 35.0), ["abc"]);
}
