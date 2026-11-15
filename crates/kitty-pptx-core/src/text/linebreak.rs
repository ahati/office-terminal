//! DrawingML soft-wrap phase — 1:1 port of `packages/core/src/drawingml-text/break.ts`
//! (reference revision 1f5c6e1d). The rule kernels it consumes live next to this
//! module: [`rules`] is `text/line-break.ts` (UAX #14 pairs), `text/cjk-ranges.ts`,
//! `text/sea-break.ts` and `text/kinsoku/*`; [`grapheme`] is the UAX #29 cluster
//! segmentation `graphemeClusterOffsets` provides through ICU.
//!
//! Input is the host's authored [`BreakItem`] list; output is logical order. The
//! adapter owns fonts and measurement ([`TextMeasure`]); this phase never changes a
//! run's style and never fabricates one. Alignment, bidi, line metrics and paint
//! are later phases. Atoms are extended grapheme clusters (break.ts:113-121).

pub(crate) mod grapheme;
pub(crate) mod rules;

use super::measure::{StyleId, TabAlign, TabStop, TextMeasure};
use grapheme::grapheme_spans;
use rules::{
    is_cjk_break_char, is_latin, is_uax14_no_break_pair, kinsoku_adjusted_split, lb_class,
    sea_seam, Lb,
};

/// One authored paragraph item, in logical order (ref break.ts:8-13).
#[derive(Clone, Debug, PartialEq)]
pub enum BreakItem {
    /// Authored text run. A `\n` inside it opens a new visual region (break.ts:114).
    Text { text: String, style: StyleId },
    /// Authored `<a:br/>` (break.ts:104-110).
    LineBreak,
    /// Authored `\t` (break.ts:117).
    Tab,
    /// Inline non-text object (math / picture bullet) with a fixed advance (break.ts:111-112).
    Object { width: f32, style: StyleId },
}

/// Wrap inputs. `max_width` is already net of body insets and paragraph margins
/// (break.ts:41-74). This API carries no `kinsokuAcrossRuns`/`nonMonotoneMeasure`
/// evidence callback: a run seam keeps its greedy break (Office C08) and advances
/// are assumed monotone over a prefix.
pub struct BreakOptions<'a> {
    pub max_width: f32,
    /// Signed first-line indent; continuation lines use the full `max_width`.
    pub first_line_indent: f32,
    pub tab_stops: &'a [TabStop],
    /// `a:pPr@defTabSz`; 0 disables the default grid.
    pub default_tab_size: f32,
    /// `a:pPr@eaLnBrk` (ECMA-376 §21.1.2.2.7, default true).
    pub east_asian_line_break: bool,
    pub measurer: &'a dyn TextMeasure,
}

/// One painted segment of a broken line (ref break.ts:15-20).
#[derive(Clone, Debug, PartialEq)]
pub enum LineItem {
    Text {
        text: String,
        style: StyleId,
        width: f32,
    },
    Tab {
        width: f32,
        style: StyleId,
    },
    Object {
        width: f32,
        style: StyleId,
    },
}

/// One visual line. `ends_with_break` mirrors the reference's `endsWithBreak` (closed
/// by an authored break); `starts_after_break` marks a region's first line after one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrokenLine {
    pub items: Vec<LineItem>,
    pub width: f32,
    pub ends_with_break: bool,
    pub starts_after_break: bool,
}

#[derive(Clone, Copy)]
enum AtomKind<'a> {
    Text(&'a str),
    Tab,
    Object(f32),
}

#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: AtomKind<'a>,
    style: StyleId,
    /// Input run that authored the atom; kinsoku never retracts across it (C08).
    run: usize,
}

impl Atom<'_> {
    fn first_char(&self) -> Option<char> {
        match self.kind { AtomKind::Text(text) => text.chars().next(), _ => None }
    }
    fn last_char(&self) -> Option<char> {
        match self.kind { AtomKind::Text(text) => text.chars().last(), _ => None }
    }
}

fn is_space(atom: &Atom<'_>) -> bool {
    matches!(atom.kind, AtomKind::Text(" "))
}

fn is_cjk_atom(atom: &Atom<'_>) -> bool {
    atom.first_char().is_some_and(|c| is_cjk_break_char(c as u32))
}

/// A tab cell's tail atom: text that is neither breakable space nor CJK
/// (ref break.ts:321-323).
fn is_cell_atom(atom: &Atom<'_>) -> bool {
    matches!(atom.kind, AtomKind::Text(_)) && !is_space(atom) && !is_cjk_atom(atom)
}

/// The explicit-stop cell opener: exactly one Latin or Number grapheme
/// (ref break.ts:314-316, `^[\p{Script_Extensions=Latin}\p{Number}]$`).
fn is_latin_cell(atom: &Atom<'_>) -> bool {
    match atom.kind {
        AtomKind::Text(text) => {
            let mut chars = text.chars();
            matches!((chars.next(), chars.next()), (Some(c), None) if is_latin(c) || c.is_numeric())
        }
        _ => false,
    }
}

/// Split the authored items into visual regions (break.ts:88-123). `<a:br/>` and
/// an in-run `\n` open a region, `\t` becomes a tab atom and `\r` is dropped. A
/// zero-width run contributes no atom; its metrics cannot be carried by this API.
fn build_regions(items: &[BreakItem]) -> Vec<Vec<Atom<'_>>> {
    let mut regions: Vec<Vec<Atom<'_>>> = vec![Vec::new()];
    for (run, item) in items.iter().enumerate() {
        match item {
            BreakItem::LineBreak => regions.push(Vec::new()),
            BreakItem::Tab => push_atom(&mut regions, AtomKind::Tab, StyleId(0), run),
            BreakItem::Object { width, style } => {
                push_atom(&mut regions, AtomKind::Object(*width), *style, run);
            }
            BreakItem::Text { text, style } => {
                for (from, to) in grapheme_spans(text) {
                    match &text[from..to] {
                        "\n" => regions.push(Vec::new()),
                        "\t" => push_atom(&mut regions, AtomKind::Tab, *style, run),
                        "\r" => {}
                        atom => push_atom(&mut regions, AtomKind::Text(atom), *style, run),
                    }
                }
            }
        }
    }
    regions
}

fn push_atom<'a>(regions: &mut [Vec<Atom<'a>>], kind: AtomKind<'a>, style: StyleId, run: usize) {
    if let Some(region) = regions.last_mut() {
        region.push(Atom { kind, style, run });
    }
}

/// Build one line: equal-style neighbours coalesce, so shaping survives an
/// authored run seam exactly like `makeSegments` (break.ts:389-414).
fn make_line(
    atoms: &[Atom<'_>],
    start: usize,
    stop: usize,
    line_index: usize,
    o: &BreakOptions<'_>,
) -> BrokenLine {
    let mut items: Vec<LineItem> = Vec::new();
    for atom in &atoms[start..stop] {
        match atom.kind {
            AtomKind::Text(text) => {
                if let Some(LineItem::Text { text: last, style, .. }) = items.last_mut() {
                    if *style == atom.style {
                        last.push_str(text);
                        continue;
                    }
                }
                items.push(LineItem::Text { text: text.to_string(), style: atom.style, width: 0.0 });
            }
            AtomKind::Tab => items.push(LineItem::Tab { width: 0.0, style: atom.style }),
            AtomKind::Object(width) => items.push(LineItem::Object { width, style: atom.style }),
        }
    }
    let width = measure_items(&mut items, line_index, o);
    BrokenLine {
        items,
        width,
        ends_with_break: false,
        starts_after_break: false,
    }
}

fn item_width(item: &LineItem) -> f32 {
    match item {
        LineItem::Text { width, .. }
        | LineItem::Tab { width, .. }
        | LineItem::Object { width, .. } => *width,
    }
}

/// `measureSegments` (break.ts:353-387): tabs resolve against the paragraph's
/// stops with an open (infinite) limit during wrapping.
fn measure_items(items: &mut [LineItem], line_index: usize, o: &BreakOptions<'_>) -> f32 {
    let mut no_stop_gap = 0.0;
    let mut has_tab = false;
    for item in items.iter_mut() {
        match item {
            LineItem::Text { text, style, width } => *width = o.measurer.advance(text, *style),
            LineItem::Tab { width, style } => {
                *width = 0.0;
                no_stop_gap = o.measurer.space(*style);
                has_tab = true;
            }
            LineItem::Object { .. } => {}
        }
    }
    if has_tab {
        let start_pen = if line_index == 0 {
            o.first_line_indent
        } else {
            0.0
        };
        resolve_tab_widths(
            items,
            o.tab_stops,
            start_pen,
            f32::INFINITY,
            no_stop_gap,
            o.default_tab_size,
        );
    }
    items.iter().map(item_width).sum()
}

/// `resolveDrawingMlTabWidths` (drawingml-text/tab.ts:20-59): seat each tab on the
/// next explicit stop, else the `defTabSz` grid, else the no-stop gap.
fn resolve_tab_widths(
    items: &mut [LineItem],
    stops: &[TabStop],
    start_pen: f32,
    limit: f32,
    no_stop_gap: f32,
    def_tab_sz: f32,
) {
    let mut pen = start_pen;
    for i in 0..items.len() {
        if !matches!(items[i], LineItem::Tab { .. }) {
            pen += item_width(&items[i]);
            continue;
        }
        let mut stop: Option<TabStop> = None;
        for candidate in stops {
            if candidate.pos > pen && stop.is_none_or(|s| candidate.pos < s.pos) {
                stop = Some(*candidate);
            }
        }
        let (stop_pos, align) = match stop {
            Some(s) => (s.pos, s.align),
            None if def_tab_sz > 0.0 => (
                ((pen / def_tab_sz).floor() + 1.0) * def_tab_sz,
                TabAlign::Left,
            ),
            None => {
                if let LineItem::Tab { width, .. } = &mut items[i] {
                    *width = no_stop_gap;
                }
                pen += no_stop_gap;
                continue;
            }
        };
        let following: f32 = items[i + 1..]
            .iter()
            .take_while(|item| !matches!(item, LineItem::Tab { .. }))
            .map(item_width)
            .sum();
        let fraction = match align {
            TabAlign::Center => 0.5,
            TabAlign::Right | TabAlign::Decimal => 1.0,
            TabAlign::Left => 0.0,
        };
        let mut target = stop_pos - following * fraction;
        if target + following > limit {
            target = limit - following;
        }
        if target < pen {
            target = pen;
        }
        if let LineItem::Tab { width, .. } = &mut items[i] {
            *width = target - pen;
        }
        pen = target;
    }
}

/// Soft-wrap a paragraph (ref break.ts:88-490). A break is permitted on either
/// side of an ordinary space; before a tab, never after it (C11); at an object
/// edge; after U+200B; between CJK atoms unless UAX #14 proves the pair
/// unbreakable; at a CJK/Latin seam; after an HY/HH hyphen flanked by Latin; at a
/// SEA script transition and between grapheme-fill SEA clusters; and — only under
/// `eaLnBrk="0"` — at any CJK adjacency. `eaLnBrk="0"` also lifts kinsoku.
pub fn break_paragraph(items: &[BreakItem], o: &BreakOptions<'_>) -> Vec<BrokenLine> {
    let regions = build_regions(items);
    let region_count = regions.len();
    let mut lines: Vec<BrokenLine> = Vec::new();
    for (region_index, mut atoms) in regions.into_iter().enumerate() {
        // C05/C06: paragraph-terminal ordinary spaces never open a continuation
        // line; a space before an authored break is kept (break.ts:129-137).
        if region_index + 1 == region_count {
            while atoms.last().is_some_and(is_space) {
                atoms.pop();
            }
        }
        let first_line = lines.len();
        let starts_after_break = region_index > 0;
        let end = atoms.len();
        if end == 0 {
            lines.push(BrokenLine {
                starts_after_break,
                ..BrokenLine::default()
            });
        } else {
            let east_asian = o.east_asian_line_break;
            let break_at: Vec<bool> = (0..=end)
                .map(|i| may_break(&atoms, i, east_asian))
                .collect();
            let mut cell_tail = vec![false; end + 1];
            cell_tail[end] = true;
            for (i, atom) in atoms.iter().enumerate().rev() {
                cell_tail[i] = cell_tail[i + 1] && is_cell_atom(atom);
            }
            let single_tab_cell = |index: usize| -> bool {
                index < end
                    && matches!(atoms[index].kind, AtomKind::Tab)
                    && !o.tab_stops.is_empty()
                    && index + 1 < end
                    && is_latin_cell(&atoms[index + 1])
                    && cell_tail[index + 1]
            };
            // Region code points for the kinsoku engine plus the atom ->
            // code-point offset map that lands retractions on atom boundaries.
            let (chars, char_offsets) = region_code_points(&atoms);
            let mut start = 0usize;
            while start < end {
                let line_index = lines.len();
                let indent = if line_index == 0 {
                    o.first_line_indent
                } else {
                    0.0
                };
                let budget = o.max_width - indent;
                let fit = if budget.is_finite() {
                    fit_prefix(&atoms, start, budget, line_index, o)
                } else {
                    end
                };
                if fit >= end {
                    let first = starts_after_break && line_index == first_line;
                    lines.push(finish_line(&atoms, start, end, line_index, o, first));
                    break;
                }
                let mut split = break_at
                    .iter()
                    .enumerate()
                    .skip(start + 1)
                    .take(fit - start)
                    .filter(|(_, breakable)| **breakable)
                    .map(|(i, _)| i)
                    .next_back()
                    .unwrap_or(0);
                if split > start && single_tab_cell(split) {
                    // An explicit stop keeps the tab and its Latin cell on the
                    // authored line past the box edge (break.ts:295-300).
                    split = end;
                }
                if single_tab_cell(start) {
                    split = start + 2;
                    while split < end
                        && !is_space(&atoms[split])
                        && !matches!(atoms[split].kind, AtomKind::Tab)
                    {
                        split += 1;
                    }
                }
                if split == 0 {
                    if matches!(atoms[start].kind, AtomKind::Tab) && fit <= start + 1 {
                        // A stop beyond the box still carries its next cell.
                        split = start + 1;
                        if split < end && matches!(atoms[split].kind, AtomKind::Text(_)) {
                            split += 1;
                            while split < end && !break_at[split] {
                                split += 1;
                            }
                        }
                    } else {
                        // Overwide word: grapheme-safe emergency break (C09/E06).
                        split = (start + 1).max(fit);
                    }
                }
                // Kinsoku (§17.15.1.58–.60) adjusts an in-run CJK boundary; a run
                // seam keeps its greedy break (C08, break.ts:467-489).
                if split < end && east_asian {
                    let (prev, next) = (&atoms[split - 1], &atoms[split]);
                    let seam_evidence = prev.run == next.run
                        || o.measurer.kinsoku_across_runs(prev.style, next.style);
                    if (is_cjk_atom(prev) || is_cjk_atom(next)) && seam_evidence {
                        let (code_start, code_split) = (char_offsets[start], char_offsets[split]);
                        if code_split - code_start > 1 && code_split < chars.len() {
                            let adjusted =
                                kinsoku_adjusted_split(&chars, code_split, code_start + 1);
                            if adjusted < code_split {
                                let mut floor = split;
                                while floor > start + 1 && char_offsets[floor] > adjusted {
                                    floor -= 1;
                                }
                                split = floor;
                            }
                        }
                    }
                }
                let first = starts_after_break && line_index == first_line;
                lines.push(finish_line(&atoms, start, split, line_index, o, first));
                start = split;
                // Drop ordinary spaces at the next line head; the ones already on
                // the closed line stay (break.ts:497-503).
                while start < end && is_space(&atoms[start]) {
                    start += 1;
                }
            }
        }
        if region_index + 1 < region_count {
            if let Some(last) = lines.last_mut() {
                last.ends_with_break = true;
            }
        }
    }
    lines
}

fn finish_line(
    atoms: &[Atom<'_>],
    start: usize,
    stop: usize,
    line_index: usize,
    o: &BreakOptions<'_>,
    first_line_of_region: bool,
) -> BrokenLine {
    let mut line = make_line(atoms, start, stop, line_index, o);
    line.starts_after_break = first_line_of_region;
    line
}

/// Greatest fitting prefix: an exponential probe then a binary search, measuring
/// only prefixes near the fit boundary (break.ts:297-330).
fn fit_prefix(
    atoms: &[Atom<'_>],
    start: usize,
    budget: f32,
    line_index: usize,
    o: &BreakOptions<'_>,
) -> usize {
    let end = atoms.len();
    let width_at = |stop: usize| make_line(atoms, start, stop, line_index, o).width;
    let (mut lo, mut hi) = (start, end);
    let mut step = 1usize;
    while lo < end {
        let probe = (start + step).min(end);
        if width_at(probe) <= budget {
            lo = probe;
            if probe == end {
                break;
            }
            step *= 2;
        } else {
            hi = probe - 1;
            break;
        }
    }
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if width_at(mid) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// `mayBreakAt` (break.ts:191-233): true when a break is permitted before `index`.
fn may_break(atoms: &[Atom<'_>], index: usize, east_asian: bool) -> bool {
    if index == 0 || index >= atoms.len() {
        return false;
    }
    let (prev, next) = (&atoms[index - 1], &atoms[index]);
    // A tab is break-before, never break-after (C11).
    if matches!(next.kind, AtomKind::Tab) {
        return true;
    }
    if matches!(prev.kind, AtomKind::Tab) {
        return false;
    }
    if is_space(prev) || is_space(next) {
        return true;
    }
    if sea_seam(prev.last_char(), next.first_char()) {
        return true;
    }
    if matches!(prev.kind, AtomKind::Object(_)) || matches!(next.kind, AtomKind::Object(_)) {
        return true;
    }
    let (Some(prev_char), Some(next_char)) = (prev.last_char(), next.first_char()) else {
        return false;
    };
    let (prev_cp, next_cp) = (prev_char as u32, next_char as u32);
    if prev_cp == 0x200b {
        return true;
    }
    // NBSP/narrow-NBSP/word-joiner/BOM bind their neighbours (T00-T08).
    let glued = |cp: u32| matches!(cp, 0xa0 | 0x202f | 0x2060 | 0xfeff);
    let adjacent_cjk = is_cjk_atom(prev) || is_cjk_atom(next);
    // eaLnBrk="0" lifts the East Asian rules but keeps the non-CJK prohibited
    // adjacency (break.ts:207-219).
    if !east_asian && adjacent_cjk && !glued(prev_cp) && !glued(next_cp) {
        return true;
    }
    if is_uax14_no_break_pair(prev_cp, next_cp) {
        return false;
    }
    if adjacent_cjk {
        return true;
    }
    // A Latin compound hyphen breaks after itself, never before (C04).
    if index > 1
        && matches!(lb_class(prev_cp), Lb::Hy | Lb::Hh)
        && atoms[index - 2].last_char().is_some_and(is_latin)
        && is_latin(next_char)
    {
        return true;
    }
    false
}

/// Code points of one region plus the atom -> code-point offset map.
fn region_code_points(atoms: &[Atom<'_>]) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut offsets = Vec::with_capacity(atoms.len() + 1);
    offsets.push(0);
    for atom in atoms {
        if let AtomKind::Text(text) = atom.kind {
            chars.extend(text.chars());
        }
        offsets.push(chars.len());
    }
    (chars, offsets)
}

#[cfg(test)]
mod tests;
