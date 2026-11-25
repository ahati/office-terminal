//! Justified text alignment — 1:1 port of `packages/pptx/src/text-justify.ts`
//! (reference revision 1f5c6e1d) over the shared gap kernel
//! `packages/core/src/text/{line-distribute,line-gaps}.ts`.
//!
//! DrawingML §20.1.10.59 offers four "fill the column" alignments:
//! `just`/`justLow` leave the paragraph's FINAL line natural (and a line closed
//! by an authored `<a:br/>`), while `dist`/`thaiDist` justify every line. All
//! four share one gap selection: an inter-word space or an inter-CJK boundary,
//! evaluated over the line's whole code-point stream so item seams never
//! swallow a gap (`text-justify.ts:58-66`). `thaiDist` additionally opens a gap
//! at every grapheme-CLUSTER boundary of a space-free Thai/Lao/Khmer span —
//! "each character is treated as a word" — keeping combining marks glued
//! (`line-gaps.ts:168-173`).
//!
//! PowerPoint's whitespace predicate is the JavaScript `\s` set
//! (`text-justify.ts:87-91`), wider than WordprocessingML's U+0020/U+3000:
//! TAB, NBSP, U+3000, … each count as ONE inter-word gap. Whitespace is
//! classified before the CJK predicate, so U+3000 is never double-counted as
//! an ideographic boundary. Leading indent whitespace opens no gap and the
//! boundary after the last content unit is trimmed, which is why the adapter
//! passes `lastDrawnSi = segments.length` (exclude no item; `text-justify.ts:118-126`).
//!
//! The reference works per laid-out segment and returns `jext`/`splitBefore`
//! annotations. This kernel is item-based: it flattens the line into UNITS
//! (one per code point for [`LineItem::Text`], one per tab/object) and returns
//! a boundary mask whose true positions are exactly the reference's
//! `splitBefore` code-point offsets.

use super::linebreak::LineItem;

/// Which of the four DrawingML fill alignments is active
/// (`ST_TextAlignType`, ECMA-376 §20.1.10.59).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyMode {
    /// `just` / `justLow`: the last line and a line ended by `<a:br/>` stay
    /// natural. `justLow` differs only in Arabic kashida refinement, which has
    /// no analogue here (`text-justify.ts:1-12`).
    Just,
    /// `dist`: every line is filled, the last included.
    Dist,
    /// `thaiDist`: `dist` plus a gap at every SEA grapheme-cluster boundary.
    ThaiDist,
}

/// The stretch to apply to one line.
///
/// `gap_after` is a BOUNDARY mask over the line's flat unit stream: entry `i`
/// is true when a gap opens immediately BEFORE unit `i` (equivalently after
/// unit `i - 1`). Its length is `units + 1`; entry 0 (line start) and entry
/// `units` (after the final content unit) are always false. The indices of the
/// true entries are the reference's `splitBefore` values, so
/// ``日本語`` yields `[false, true, true, false]` → gaps `[1, 2]`.
///
/// A unit is one code point of a [`LineItem::Text`]; a [`LineItem::Tab`] or
/// [`LineItem::Object`] is one opaque unit. A renderer walks its items in
/// order, tracking the global unit offset, and inserts `per_gap` at each open
/// boundary. `Σ (open boundaries) × per_gap == slack`.
#[derive(Clone, Debug, PartialEq)]
pub struct JustifyPlan {
    /// px added at every open gap.
    pub per_gap: f32,
    /// Boundary mask, see the type documentation.
    pub gap_after: Vec<bool>,
}

impl JustifyPlan {
    /// Number of open gaps, i.e. how many boundaries the slack is split over.
    pub fn gap_count(&self) -> usize {
        self.gap_after.iter().filter(|open| **open).count()
    }

    /// The open boundaries (the reference's `splitBefore` offsets).
    pub fn gap_positions(&self) -> Vec<usize> {
        self.gap_after
            .iter()
            .enumerate()
            .filter_map(|(index, open)| if *open { Some(index) } else { None })
            .collect()
    }
}

/// One code point (or opaque atom) of the flattened line.
#[derive(Clone, Copy)]
struct Unit {
    /// `None` for a tab or inline object (an opaque atom, `line-gaps.ts:207-211`).
    cp: Option<char>,
    /// PowerPoint's JS-`\s` inter-word classification.
    ws: bool,
}

/// `justifyLine` (`text-justify.ts:93-141`) with the DrawingML policy.
///
/// `avail` is the content width to fill and `natural` the measured advance of
/// the unexpanded line. Returns `None` when the alignment leaves the line
/// ragged (`just` on the final / `<a:br/>`-terminated line), when the slack is
/// at or below the reference's 0.5 px noise floor, or when the line offers no
/// stretchable boundary.
pub fn justify_line(
    items: &[LineItem],
    avail: f32,
    natural: f32,
    mode: JustifyMode,
    last_line: bool,
    ends_with_break: bool,
) -> Option<JustifyPlan> {
    // `just`/`justLow` leave the final or manually terminated line natural;
    // `dist`/`thaiDist` justify every line (`text-justify.ts:95-96`,
    // `align.ts:24-30`).
    if mode == JustifyMode::Just && (last_line || ends_with_break) {
        return None;
    }
    // `text-justify.ts:99-101`: no room to fill, or overflowing. Negative
    // slack is never compressed by the pptx adapter.
    let slack = avail - natural;
    if slack <= 0.5 {
        return None;
    }
    let gap_after = gap_mask(items, mode == JustifyMode::ThaiDist);
    let gaps = gap_after.iter().filter(|open| **open).count();
    if gaps == 0 {
        return None;
    }
    Some(JustifyPlan {
        per_gap: slack / gaps as f32,
        gap_after,
    })
}

/// Flatten the line into units (`line-gaps.ts:196-225`).
fn units(items: &[LineItem]) -> Vec<Unit> {
    let mut out = Vec::new();
    for item in items {
        match item {
            LineItem::Text { text, .. } => {
                out.extend(text.chars().map(|c| Unit {
                    cp: Some(c),
                    ws: is_js_whitespace(c as u32),
                }));
            }
            // A tab or inline object is one opaque unit: it bears no stretch of
            // its own, but a CJK neighbour can open a gap against its edge.
            LineItem::Tab { .. } | LineItem::Object { .. } => out.push(Unit { cp: None, ws: false }),
        }
    }
    out
}

/// `enumerateGaps` (`line-gaps.ts:118-225`) with the pptx adapter's options:
/// `firstContentSi = 0`, `lastDrawnSi = segments.length` (exclude no item),
/// the JS-`\s` whitespace predicate, `isGapChar = isCjkBreakChar`, and
/// `seaClusterGaps = mode === 'thaiDist'`.
fn gap_mask(items: &[LineItem], thai_clusters: bool) -> Vec<bool> {
    let units = units(items);
    // `mask[i]` = a gap opens before unit `i`.
    let mut mask = vec![false; units.len() + 1];
    // Last non-whitespace unit; whitespace never replaces it (`line-gaps.ts:155-158`).
    let mut last: Option<usize> = None;
    let mut has_content = false;
    // A space run is pending until its right neighbour arrives; then every
    // space in the run is one opportunity (`line-gaps.ts:139-171`).
    let mut pending: Vec<usize> = Vec::new();
    let mut last_was_space = false;
    for (index, unit) in units.iter().enumerate() {
        if unit.ws {
            // Leading whitespace (no content yet) stays fixed, and a trailing
            // run at the line end never gets a right neighbour.
            if has_content {
                pending.push(index);
            }
            last_was_space = true;
            continue;
        }
        if !pending.is_empty() {
            // Each space is one gap, owned by the boundary after itself.
            // Whitespace is classified before the CJK rule, so this never
            // double-counts U+3000 (`cjk-ranges.ts:35-42`).
            for &space in &pending {
                mask[space + 1] = true;
            }
            pending.clear();
        } else if let Some(previous) = last {
            if has_content && !last_was_space && opens_gap(units[previous].cp, unit.cp, thai_clusters) {
                mask[previous + 1] = true;
            }
        }
        has_content = true;
        last = Some(index);
        last_was_space = false;
    }
    mask
}

/// The inter-CJK / SEA-cluster boundary predicate (`line-gaps.ts:168-173`):
/// either side ideographic, or — under `thaiDist` — both sides in a SEA block
/// with the right side starting a new grapheme cluster.
fn opens_gap(previous: Option<char>, next: Option<char>, thai_clusters: bool) -> bool {
    let previous_cp = previous.map(|c| c as u32);
    let next_cp = next.map(|c| c as u32);
    if previous_cp.is_some_and(is_cjk_break_char) || next_cp.is_some_and(is_cjk_break_char) {
        return true;
    }
    thai_clusters
        && previous_cp.is_some_and(is_sea_script_code_point)
        && next_cp.is_some_and(is_sea_script_code_point)
        && !next_cp.is_some_and(is_sea_grapheme_extend)
}

/// JavaScript `\s` (ECMAScript WhiteSpace + LineTerminator). PowerPoint counts
/// every one of these as an inter-word space (`text-justify.ts:84-91`):
/// TAB, LF, VT, FF, CR, SP, NBSP, OGHAM SPACE MARK, U+2000–U+200A, LS, PS,
/// NARROW NBSP, MEDIUM MATHEMATICAL SPACE, IDEOGRAPHIC SPACE and ZWNBSP.
fn is_js_whitespace(cp: u32) -> bool {
    matches!(
        cp,
        0x0009 | 0x000a | 0x000b | 0x000c | 0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000
            ..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

/// `isCjkBreakChar` (`cjk-ranges.ts:57-64`):
/// U+3000–U+9FFF (CJK symbols, kana, unified ideographs),
/// U+AC00–U+D7A3 (Hangul syllables — NOT the conjoining Jamo above it),
/// U+F900–U+FAFF (compatibility ideographs) and U+FF00–U+FFEF (half/fullwidth
/// forms). Duplicated from `text/linebreak/rules.rs` because that module is
/// private to the breaker; the ranges are identical.
fn is_cjk_break_char(cp: u32) -> bool {
    matches!(cp, 0x3000..=0x9fff | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xff00..=0xffef)
}

/// `isSeaScriptCodePoint` (`sea-break.ts:69-79`): Thai, Lao, Khmer, plus the
/// grapheme-fill Myanmar/Tibetan blocks.
fn is_sea_script_code_point(cp: u32) -> bool {
    matches!(
        cp,
        0x0e00..=0x0e7f
            | 0x0e80..=0x0eff
            | 0x1780..=0x17ff
            | 0x1000..=0x109f
            | 0xaa60..=0xaa7f
            | 0xa9e0..=0xa9ff
            | 0x0f00..=0x0fff
    )
}

/// `isSeaGraphemeExtend` (`sea-break.ts:98-112`): Thai/Lao/Khmer combining
/// marks whose Grapheme_Cluster_Break is Extend/SpacingMark. A `thaiDist` gap
/// never opens before them, so a base keeps its vowels and tone marks glued.
fn is_sea_grapheme_extend(cp: u32) -> bool {
    matches!(
        cp,
        0x0e31 | 0x0e33..=0x0e3a | 0x0e47..=0x0e4e | 0x0eb1 | 0x0eb3..=0x0ebc
            | 0x0ec8..=0x0ece
            | 0x17b4..=0x17d3
            | 0x17dd
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::measure::StyleId;

    fn text(value: &str, width: f32) -> LineItem {
        LineItem::Text {
            text: value.to_string(),
            style: StyleId(0),
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

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-4, "expected {expected}, got {actual}");
    }

    /// `text-justify.test.ts:28`: 3 CJK glyphs, 2 interior gaps, slack 60.
    #[test]
    fn dist_justifies_the_last_cjk_line() {
        let items = [text("日本語", 60.0)];
        let p = plan(&items, 120.0, 60.0, JustifyMode::Dist, true, false);
        close(p.per_gap, 30.0);
        assert_eq!(p.gap_positions(), [1, 2]);
        // Every gap is inside the line: no gap at the start or after the tail.
        assert_eq!(p.gap_after, [false, true, true, false]);
        assert_eq!(p.gap_count(), 2);
    }

    /// `text-justify.test.ts:24`: `just` leaves the final line natural; the
    /// same line under `just` before an authored break also stays ragged.
    #[test]
    fn just_leaves_the_last_and_break_terminated_lines_natural() {
        let items = [text("日本語", 60.0)];
        assert_eq!(justify_line(&items, 120.0, 60.0, JustifyMode::Just, true, false), None);
        assert_eq!(justify_line(&items, 120.0, 60.0, JustifyMode::Just, false, true), None);
        // A middle line of the same paragraph still justifies.
        let p = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 30.0);
        assert_eq!(p.gap_positions(), [1, 2]);
    }

    /// `text-justify.test.ts:35-46`: `thaiDist` distributes space-free Thai at
    /// grapheme-cluster boundaries; `dist` is NOT an alias and leaves it ragged.
    #[test]
    fn thai_dist_opens_sea_cluster_boundaries() {
        let items = [text("เมือง", 60.0)];
        let p = plan(&items, 90.0, 60.0, JustifyMode::ThaiDist, false, false);
        close(p.per_gap, 10.0);
        // เ|ม, ◌ื|อ (the mark stays glued to ม) and อ|ง.
        assert_eq!(p.gap_positions(), [1, 3, 4]);
        assert_eq!(justify_line(&items, 90.0, 60.0, JustifyMode::Dist, false, false), None);
        // `dist` never opens CJK gaps on Thai text either.
        assert_eq!(justify_line(&items, 90.0, 60.0, JustifyMode::Just, false, false), None);
    }

    /// `text-justify.test.ts:62`: no gap inside the Latin "EC"; gaps at C|市,
    /// 市|場 and 場|で.
    #[test]
    fn mixed_latin_and_cjk_opens_only_cjk_boundaries() {
        let items = [text("EC市場で", 100.0)];
        let p = plan(&items, 130.0, 100.0, JustifyMode::Just, false, false);
        close(p.per_gap, 10.0);
        assert_eq!(p.gap_positions(), [2, 3, 4]);
    }

    /// `text-justify.test.ts:68`: leading 字下げ whitespace stays fixed.
    #[test]
    fn leading_indent_whitespace_opens_no_gap() {
        let items = [text("  日本語", 100.0)];
        let p = plan(&items, 130.0, 100.0, JustifyMode::Just, false, false);
        close(p.per_gap, 15.0);
        assert_eq!(p.gap_positions(), [3, 4]);
    }

    /// `text-justify.test.ts:81`: trailing whitespace at the line end does not
    /// stretch — the space run never gets a right neighbour.
    #[test]
    fn trailing_whitespace_opens_no_gap() {
        let items = [text("日本 ", 60.0)];
        let p = plan(&items, 100.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 40.0);
        assert_eq!(p.gap_positions(), [1]);
    }

    /// `text-justify.test.ts:100-125`: CJK boundaries are evaluated across item
    /// seams, and gaps open inside the final item too (only the boundary after
    /// the last content unit is trimmed).
    #[test]
    fn cjk_boundaries_cross_item_seams() {
        let split = [text("日本", 40.0), text("語", 20.0)];
        let p = plan(&split, 120.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 30.0);
        assert_eq!(p.gap_positions(), [1, 2]);
        assert_eq!(p.gap_after, [false, true, true, false]);

        let last_item = [text("日本", 40.0), text("語学", 40.0)];
        let p = plan(&last_item, 150.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 30.0);
        assert_eq!(p.gap_positions(), [1, 2, 3]);
    }

    /// `text-justify.test.ts:127-146`: an inline object is one opaque unit; a
    /// CJK neighbour opens a gap against its edge and the object may carry a
    /// trailing gap of its own.
    #[test]
    fn inline_objects_are_single_units() {
        let items = [
            text("日", 20.0),
            LineItem::Object {
                width: 20.0,
                style: StyleId(0),
            },
            text("本", 20.0),
        ];
        let p = plan(&items, 100.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 20.0);
        assert_eq!(p.gap_positions(), [1, 2]);

        // A tab is the same kind of opaque unit.
        let tabbed = [
            text("日", 20.0),
            LineItem::Tab {
                width: 40.0,
                style: StyleId(0),
            },
            text("本", 20.0),
        ];
        let p = plan(&tabbed, 100.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 20.0);
        assert_eq!(p.gap_positions(), [1, 2]);
    }

    /// `text-justify.test.ts:167-178`: U+3000 is whitespace first, so it is one
    /// inter-word gap and never double-counted as an inter-CJK boundary.
    #[test]
    fn ideographic_space_is_one_inter_word_gap() {
        let items = [text("日　本", 60.0)];
        let p = plan(&items, 120.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 60.0);
        assert_eq!(p.gap_positions(), [2]);

        let tail = [text("日　本で", 60.0)];
        let p = plan(&tail, 140.0, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 40.0);
        assert_eq!(p.gap_positions(), [2, 3]);
    }

    /// `text-justify.test.ts:182-194`: no gap to distribute → no plan.
    #[test]
    fn no_stretchable_boundary_returns_none() {
        assert_eq!(
            justify_line(&[text("Hello", 50.0)], 200.0, 50.0, JustifyMode::Just, false, false),
            None
        );
        assert_eq!(
            justify_line(&[text("日", 20.0)], 200.0, 20.0, JustifyMode::Dist, false, false),
            None
        );
        assert_eq!(justify_line(&[], 200.0, 0.0, JustifyMode::Dist, false, false), None);
    }

    /// `text-justify.test.ts:196-198`: the 0.5 px noise floor.
    #[test]
    fn slack_at_or_below_the_noise_floor_is_ignored() {
        let items = [text("日本語", 60.0)];
        assert_eq!(justify_line(&items, 60.2, 60.0, JustifyMode::Just, false, false), None);
        assert_eq!(justify_line(&items, 60.5, 60.0, JustifyMode::Just, false, false), None);
        assert_eq!(justify_line(&items, 59.0, 60.0, JustifyMode::Just, false, false), None);
        let p = plan(&items, 60.6, 60.0, JustifyMode::Just, false, false);
        close(p.per_gap, 0.3);
    }

    /// `line-distribute.test.ts:441-466`: under PowerPoint's predicate a TAB is
    /// an inter-word gap, while a Latin|Latin seam opens nothing.
    #[test]
    fn js_whitespace_set_is_the_inter_word_predicate() {
        let items = [text("a\tb", 10.0), text("c", 2.0)];
        let p = plan(&items, 22.0, 10.0, JustifyMode::Just, false, false);
        close(p.per_gap, 12.0);
        assert_eq!(p.gap_positions(), [2]);

        // NBSP is in the JS `\s` set even though the breaker glues around it.
        let nbsp = [text("a\u{a0}b", 10.0)];
        assert_eq!(
            plan(&nbsp, 20.0, 10.0, JustifyMode::Just, false, false).gap_positions(),
            [2]
        );
    }

    /// `line-distribute.test.ts:352-374`: `thaiDist` clusters are evaluated over
    /// the whole line, so a style split mid-phrase keeps its boundary.
    #[test]
    fn sea_cluster_gaps_cross_item_seams() {
        let items = [text("กา", 20.0), text("งาน", 20.0)];
        let p = plan(&items, 40.0, 0.0, JustifyMode::ThaiDist, false, false);
        close(p.per_gap, 10.0);
        assert_eq!(p.gap_positions(), [1, 2, 3, 4]);
    }

    /// Σ gaps × perGap == slack, so the painted line reaches `avail`.
    #[test]
    fn stretch_sums_to_the_slack() {
        let items = [text("Hello world foo bar", 120.0)];
        let p = plan(&items, 300.0, 120.0, JustifyMode::Just, false, false);
        close(p.gap_count() as f32 * p.per_gap, 180.0);
        assert_eq!(p.gap_positions(), [6, 12, 16]);
    }
}
