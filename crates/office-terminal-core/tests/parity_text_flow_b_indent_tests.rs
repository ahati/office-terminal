//! Parity ports of the reference text-flow specs (batch `text-flow-b`, part 3:
//! first-line indent budgets, superscript run sizing and the astral-grapheme
//! kinsoku probe).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/firstline-indent-wrap.test.ts`,
//! `superscript-citation-wrap.test.ts` and `cjk-wrap.test.ts`'s astral case.
//! The stubs are the specs' own: `measureText = [...text].length * 10` (code
//! points), and the citation spec's size-scaled mock (20 px per code point, 13 px
//! for the 65% baseline run).

use office_terminal_core::text::linebreak::{
    break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem,
};
use office_terminal_core::text::{StyleId, TextMeasure};

const S: StyleId = StyleId(0);
/// The superscript / full-size citation run.
const S2: StyleId = StyleId(1);

/// `[...text].length * 10`, space 10.
struct Len10;

impl TextMeasure for Len10 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 10.0
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

/// `superscript-citation-wrap.test.ts:9-14`: the mock scales with the size in
/// `ctx.font`, so a `baseline="30000"` run advances 13 px per code point while
/// its authored size stays 20.
struct Citations;

impl TextMeasure for Citations {
    fn advance(&self, text: &str, style: StyleId) -> f32 {
        let px = if style == S2 { 13.0 } else { 20.0 };
        text.chars().count() as f32 * px
    }
    fn space(&self, style: StyleId) -> f32 {
        self.advance(" ", style)
    }
}

/// The full-size control's `measureText`: 20 px per code point everywhere.
struct Len20;

impl TextMeasure for Len20 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.chars().count() as f32 * 20.0
    }
    fn space(&self, _: StyleId) -> f32 {
        20.0
    }
}

static LEN10: Len10 = Len10;
static CITATIONS: Citations = Citations;
static LEN20: Len20 = Len20;

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

fn wrap_indent(
    items: &[BreakItem],
    measurer: &dyn TextMeasure,
    width: f32,
    indent: f32,
) -> Vec<BrokenLine> {
    let options = BreakOptions {
        max_width: width,
        first_line_indent: indent,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer,
    };
    break_paragraph(items, &options)
}

fn wrap(items: &[BreakItem], measurer: &dyn TextMeasure, width: f32) -> Vec<BrokenLine> {
    wrap_indent(items, measurer, width, 0.0)
}

/// `firstline-indent-wrap.test.ts:98-101` `firstLineWordCount`.
fn word_count(line: &BrokenLine) -> usize {
    line_text(line)
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .count()
}

/// `cjk-wrap.test.ts:55-59`: `fitCjkLine(mc('𠮷𠮷、'), 0, 20)` is 1 — an astral
/// ideograph is ONE grapheme (10 px here) and kinsoku pulls the second one down
/// so the line never begins with 、.
#[test]
fn cjk_astral_ideographs_are_single_graphemes_with_kinsoku() {
    let lines = wrap(&[text_item("𠮷𠮷、")], &LEN10, 20.0);
    assert_eq!(texts(&lines), ["𠮷", "𠮷、"]);
    assert_eq!(lines[0].width, 10.0);
}

// ── firstline-indent-wrap.test.ts ──────────────────────────────────────────

/// `:104-112` baseline: "aaaa bbbb cccc dddd" = 190 fits; adding " eeee" (240)
/// does not, so four tokens start line 1. The authored space stays on the
/// closed line (ref `break.ts` "Keep authored spaces on the closed line"), so
/// the spec's own `lineWidth` is the 200 px the assertion bounds.
#[test]
fn first_line_indent_baseline_packs_four_tokens() {
    let lines = wrap(&[text_item("aaaa bbbb cccc dddd eeee")], &LEN10, 200.0);
    assert_eq!(line_text(&lines[0]), "aaaa bbbb cccc dddd ");
    assert_eq!(lines[0].width, 200.0);
    assert_eq!(word_count(&lines[0]), 4);
}

/// `:114-134`: an 80 px indent narrows ONLY the first line (budget 120): two
/// tokens fit; continuation lines keep the full 200 px.
#[test]
fn first_line_indent_narrows_the_first_wrap_line() {
    let lines = wrap_indent(
        &[text_item("aaaa bbbb cccc dddd eeee")],
        &LEN10,
        200.0,
        80.0,
    );
    assert_eq!(line_text(&lines[0]), "aaaa bbbb ");
    assert_eq!(lines[0].width, 100.0);
    assert_eq!(word_count(&lines[0]), 2);
    assert!(lines[0].width <= 120.0);
    let continuation = lines[1..].iter().map(|line| line.width).fold(0.0, f32::max);
    assert!(
        continuation > 120.0,
        "continuation {continuation} used the narrow budget"
    );
    assert!(continuation <= 200.0);
}

/// `:136-150`: an explicit zero indent is byte-identical to an omitted one.
#[test]
fn first_line_indent_zero_matches_the_default() {
    let items = [text_item("aaaa bbbb cccc dddd eeee")];
    let omitted = texts(&wrap(&items, &LEN10, 200.0));
    let explicit_zero = texts(&wrap_indent(&items, &LEN10, 200.0, 0.0));
    assert_eq!(explicit_zero, omitted);
    assert_eq!(omitted, ["aaaa bbbb cccc dddd ", "eeee"]);
}

// ── superscript-citation-wrap.test.ts ──────────────────────────────────────

/// `:95-112`: a `baseline="30000"` citation measures at 65% (the stub's 13 px),
/// so "aaaa bbbb citation 1" (386 px) fits the 390 px box on one line.
#[test]
fn superscript_citation_measures_at_65_percent() {
    let items = [text_item("aaaa bbbb citation"), styled_item(" 1", S2)];
    let lines = wrap(&items, &CITATIONS, 390.0);
    assert_eq!(texts(&lines), ["aaaa bbbb citation 1"]);
    assert_eq!(lines[0].width, 386.0);
}

/// `:114-127` control: at full size the same text is 400 px, so PowerPoint
/// orphans "1" onto the next line (the space before it stays on line 1).
#[test]
fn superscript_full_size_control_orphans_the_citation() {
    let items = [text_item("aaaa bbbb citation"), styled_item(" 1", S2)];
    let lines = wrap(&items, &LEN20, 390.0);
    assert_eq!(texts(&lines), ["aaaa bbbb citation ", "1"]);
    assert_eq!(lines[0].width, 380.0);
}
