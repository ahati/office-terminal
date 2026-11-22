//! Parity ports of the reference text-flow specs (batch `text-flow-b`, part 2:
//! South-East Asian wrapping and justification-gap structure).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/sea-line-break.test.ts`,
//! `sea-transition-line-break.test.ts`, `sea-justified-fit.test.ts`,
//! `cjk-spc-justify.test.ts` and the astral-grapheme probe of `cjk-wrap.test.ts`.
//! The stub measurer is the specs' own (`10 px per UTF-16 unit`, space 10).
//!
//! The Thai/Lao/Khmer dictionary segmenter (`seaWordBreakOffsets`, ICU
//! `Intl.Segmenter`) has no Rust counterpart in this crate — `text/linebreak/rules.rs`
//! documents that a dictionary span adds nothing and falls back to the
//! reference's own "no segmenter" path. Cases that only make sense WITH the
//! dictionary are listed as BLOCKED in `tests/parity/coverage/text-flow-b.md`;
//! everything the dictionary-free contract can express is asserted here.

use office_terminal_core::text::justify::{justify_line, JustifyMode, JustifyPlan};
use office_terminal_core::text::linebreak::{
    break_paragraph, BreakItem, BreakOptions, BrokenLine, LineItem,
};
use office_terminal_core::text::{StyleId, TextMeasure};

const S: StyleId = StyleId(0);

/// The SEA specs' mock: `measureText = text.length * 10` (UTF-16 code units).
struct Len10Utf16;

impl TextMeasure for Len10Utf16 {
    fn advance(&self, text: &str, _: StyleId) -> f32 {
        text.encode_utf16().count() as f32 * 10.0
    }
    fn space(&self, _: StyleId) -> f32 {
        10.0
    }
}

static LEN10: Len10Utf16 = Len10Utf16;

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
    lines.iter().map(line_text).collect()
}

fn wrap_one(text: &str, width: f32) -> Vec<BrokenLine> {
    let options = BreakOptions {
        max_width: width,
        first_line_indent: 0.0,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer: &LEN10,
    };
    break_paragraph(&[text_item(text)], &options)
}

/// `breakOffsets(texts)` (`sea-line-break.test.ts:64-70`).
fn break_offsets(lines: &[String]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut acc = 0;
    for line in lines.iter().take(lines.len().saturating_sub(1)) {
        acc += line.chars().count();
        out.push(acc);
    }
    out
}

/// `isSeaScriptCodePoint` (`packages/core/src/text/sea-break.ts:69-79`).
fn is_sea(c: char) -> bool {
    matches!(
        c as u32,
        0x0e00..=0x0e7f
            | 0x0e80..=0x0eff
            | 0x1780..=0x17ff
            | 0x1000..=0x109f
            | 0xaa60..=0xaa7f
            | 0xa9e0..=0xa9ff
            | 0x0f00..=0x0fff
    )
}

/// `isCjkBreakChar` (`cjk-ranges.ts:57-64`).
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3000..=0x9fff | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xff00..=0xffef
    )
}

/// `isBreakInertSpaceCp` (`sea-break.ts:334-340`).
fn is_inert_space(c: char) -> bool {
    matches!(
        c as u32,
        0x20 | 0x09 | 0x0a | 0x0d | 0x3000 | 0xa0 | 0x2007 | 0x202f | 0x2060 | 0xfeff
    )
}

/// `seaTransitionOffsets` (`sea-break.ts:299-322`) in code-point offsets.
fn transition_offsets(text: &str) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for i in 1..chars.len() {
        let (prev, cur) = (chars[i - 1], chars[i]);
        if is_sea(prev) != is_sea(cur) && !is_inert_space(prev) && !is_inert_space(cur) {
            out.push(i);
        }
    }
    out
}

/// `seaMixedBreakOffsets(text, {cjk:true})` restricted to the sources this crate
/// implements (dictionary spans add nothing without ICU, which is the
/// reference's own graceful fallback).
fn legal_offsets(text: &str) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = transition_offsets(text);
    for i in 1..chars.len() {
        if is_cjk(chars[i - 1]) || is_cjk(chars[i]) {
            out.push(i);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Combining marks a grapheme cluster must never start a line with: Myanmar
/// medials/vowels (U+102B-U+103E) and Tibetan vowel signs/subjoined letters
/// (U+0F71-U+0F84, U+0F8D-U+0FBC).
fn is_stack_mark(c: char) -> bool {
    matches!(c as u32, 0x102b..=0x103e | 0x0f71..=0x0f84 | 0x0f8d..=0x0fbc)
}

// ── sea-line-break.test.ts (#797) ──────────────────────────────────────────

/// The spec's Thai sample (`:72`).
const THAI: &str = "ภาษาไทยเป็นภาษาที่สวยงามมาก";

/// `:76-83`: every wrapped line's SEA text is ONE segment — measure==paint, so
/// the per-push widths sum to the line width.
#[test]
fn sea_thai_line_is_a_single_segment() {
    let lines = wrap_one(THAI, 100.0);
    for line in &lines {
        let non_empty = line
            .items
            .iter()
            .filter(|item| matches!(item, LineItem::Text { text, .. } if !text.is_empty()))
            .count();
        assert!(non_empty <= 1, "line {line:?} split its SEA text");
    }
}

/// `:85-92`: narrower than one word still makes progress and preserves the text.
#[test]
fn sea_thai_narrow_box_makes_progress_and_preserves_text() {
    let lines = wrap_one(THAI, 15.0);
    let out = texts(&lines);
    assert_eq!(out.concat(), THAI);
    assert!(out.iter().all(|line| !line.is_empty()), "{out:?}");
}

/// `:94-99`: a Thai run that fits stays on one line without fragmenting.
#[test]
fn sea_thai_fitting_run_is_one_line() {
    let lines = wrap_one(THAI, 400.0);
    assert_eq!(lines.len(), 1);
    let segments: Vec<&LineItem> = lines[0]
        .items
        .iter()
        .filter(|item| matches!(item, LineItem::Text { text, .. } if !text.is_empty()))
        .collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(line_text(&lines[0]), THAI);
}

/// `:104-124` (#961): Myanmar and Tibetan break at grapheme-cluster boundaries
/// with maximal fill — a line never begins with a stacked mark, so a base and
/// its marks are never torn, and the text is preserved.
#[test]
fn sea_myanmar_tibetan_break_at_grapheme_boundaries() {
    for text in [
        "မြန်မာဘာသာစကားကိုစာလုံးများအကြားတွင်ကွက်လပ်မထား",
        "བོད་ཡིག་ནི་ཚིག་གྲུབ་སོ་སོའི་བར་དུ་ཚེག",
    ] {
        let lines = wrap_one(text, 120.0);
        let out = texts(&lines);
        assert!(out.len() > 1, "no wrap for {text:?}: {out:?}");
        assert_eq!(out.concat(), text);
        for line in &out {
            let head = line.chars().next().expect("a non-empty line");
            assert!(
                !is_stack_mark(head),
                "line {line:?} begins with a stacked mark",
            );
            assert!(line.chars().count() * 10 <= 120 || line.chars().count() <= 2);
        }
    }
}

// ── sea-transition-line-break.test.ts (#960) ───────────────────────────────

/// `:59-72` (a2): Thai↔digit seams are breaks; a number is never torn. The
/// reference's `legal()` set is `seaMixedBreakOffsets`, whose dictionary spans
/// need ICU; without a dictionary the only breaks this crate may add beyond the
/// transition/CJK sets are Thai-interior ones (its documented fallback), which
/// the loop below pins explicitly.
#[test]
fn sea_thai_digit_transitions_wrap_but_never_split_a_number() {
    const A2: &str = "ราคาสินค้า1250บาทลดเหลือ990บาทประหยัด260บาทหรือ21เปอร์เซ็นต์ต่อชิ้น";
    let lines = wrap_one(A2, 130.0);
    let out = texts(&lines);
    assert_eq!(out.concat(), A2);
    assert!(out.len() > 1, "no wrap: {out:?}");
    let legal = legal_offsets(A2);
    let chars: Vec<char> = A2.chars().collect();
    let breaks = break_offsets(&out);
    for &b in &breaks {
        if legal.contains(&b) {
            continue;
        }
        // Dictionary-only boundary (no ICU segmenter): Thai on both sides.
        assert!(
            is_sea(chars[b - 1]) && is_sea(chars[b]),
            "break {b} is neither a transition, a CJK edge nor a Thai boundary: {out:?}",
        );
        let is_digit = |c: char| c.is_ascii_digit();
        assert!(
            !(is_digit(chars[b - 1]) && is_digit(chars[b])),
            "break {b} tore a number: {out:?}",
        );
    }
    let transitions = transition_offsets(A2);
    assert!(
        breaks.iter().any(|b| transitions.contains(b)),
        "no script transition was used: {out:?}",
    );
}

/// `:74-84` (b): in one mixed CJK+Thai run each side keeps its own rule — the
/// contiguous Thai span stays intact on one line.
#[test]
fn sea_mixed_cjk_thai_keeps_the_thai_span_intact() {
    const B: &str = "日本語のテキストとภาษาไทยが同じ";
    let lines = wrap_one(B, 100.0);
    let out = texts(&lines);
    assert_eq!(out.concat(), B);
    assert!(out.len() > 1, "no wrap: {out:?}");
    let legal = legal_offsets(B);
    for b in break_offsets(&out) {
        assert!(legal.contains(&b), "illegal break {b} in {out:?}");
    }
    assert!(
        out.iter().any(|line| line.contains("ภาษาไทย")),
        "the Thai span was torn: {out:?}",
    );
}

// ── sea-justified-fit.test.ts (#991) ───────────────────────────────────────

/// `:110-117` control: a chunk that fits the remaining width stays on the line.
#[test]
fn sea_justified_chunk_fitting_the_remainder_stays() {
    let lines = wrap_one("มาก น้อยน้อย", 170.0);
    assert_eq!(lines.len(), 1);
}

/// `:139-146` Rule 3: zero trailing-space shrink — a final word overflowing by
/// 1 px wraps, leaving the leading words on line 1 (180 px natural, 179 px box).
#[test]
fn sea_justified_zero_shrink_wraps_a_one_pixel_overflow() {
    let lines = wrap_one("มาก มาก มาก ดังนี้", 179.0);
    assert_eq!(lines.len(), 2);
    let out: Vec<String> = texts(&lines)
        .iter()
        .map(|line| line.split_whitespace().collect::<String>())
        .collect();
    assert_eq!(out, ["มากมากมาก", "ดังนี้"]);
}

/// `:148-152` Rule 3: the identical wrap holds for left alignment. The wrap
/// kernel has no alignment input, so alignment independence is structural; the
/// probe pins the same numbers as the `thaiDist` case.
#[test]
fn sea_justified_wrap_is_alignment_independent() {
    let lines = wrap_one("มาก มาก มาก ดังนี้", 179.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(texts(&lines)[1].trim(), "ดังนี้");
}

/// `:154-158` Rule 3 control: X = −3 (183 px) admits the final word.
#[test]
fn sea_justified_three_pixel_slack_stays_on_one_line() {
    let lines = wrap_one("มาก มาก มาก ดังนี้", 183.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(line_text(&lines[0]).split_whitespace().collect::<String>(), "มากมากมากดังนี้");
}

// ── cjk-spc-justify.test.ts — justification gap structure ──────────────────

fn text(text: &str, width: f32) -> LineItem {
    LineItem::Text {
        text: text.to_string(),
        style: S,
        width,
    }
}

fn plan(items: &[LineItem], avail: f32, natural: f32) -> JustifyPlan {
    justify_line(items, avail, natural, JustifyMode::Dist, true, false)
        .unwrap_or_else(|| panic!("expected a justification plan for {items:?}"))
}

/// `:216-233` (4a): "あ［AB］い" severs into single-glyph pieces at every CJK
/// boundary, but the embedded Latin pair "AB" stays ONE piece — no gap opens
/// between A and B, so a later contiguous draw keeps the pair whole.
#[test]
fn spc_justify_opens_no_gap_inside_an_embedded_latin_pair() {
    let items = [text("あ［AB］い", 60.0)];
    let plan = plan(&items, 200.0, 60.0);
    assert_eq!(plan.gap_positions(), [1, 2, 4, 5]);
    assert!(!plan.gap_after[3], "a gap opened between A and B");
    assert_eq!(plan.gap_count(), 4);
    assert!((plan.per_gap - 35.0).abs() < 1e-4, "140 px over 4 gaps");
}

/// `:235-258` (4b): with ls == 0 a pure-CJK `dist` line is FULLY distributed —
/// a gap at every one of the four inter-glyph boundaries, carrying the stretch
/// as one positive pitch (`segPerGap`), never per-glyph isolated draws.
#[test]
fn spc_fully_distributed_cjk_line_opens_every_gap() {
    let items = [text("あいうえお", 50.0)];
    let plan = plan(&items, 200.0, 50.0);
    assert_eq!(plan.gap_positions(), [1, 2, 3, 4]);
    assert_eq!(plan.gap_count(), 4);
    assert!(plan.per_gap > 0.0, "segPerGap must be a positive expansion");
    assert!((plan.per_gap - 37.5).abs() < 1e-4, "150 px over 4 gaps");
}
