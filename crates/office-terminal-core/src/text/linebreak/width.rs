//! Kerning-aware width oracle for the soft-wrap fit probe.
//!
//! `fit_prefix` prices each candidate atom prefix against the wrap budget.
//! Its historical probe-by-`make_line` walk re-shaped the whole line for every
//! probe — an exponential probe plus a binary search, ~8–10 full
//! cosmic-text/rustybuzz line shapes per wrapped line. This module prices the
//! same probes through a word-level reconstruction that is bit-identical
//! (f32 `==`) to what `make_line` would have measured, for one full item shape
//! per line plus a tiny truncated-word shape per probe that cuts inside a word.
//!
//! Why word level and not per-cluster sums: cosmic-text shapes a line per
//! UAX #14 word inside each bidi span (see `text::shape`), so kerning and
//! ligatures never cross a word boundary and the layout width is the fold
//! `Σ_span (Σ_word word.width)` in logical order. A prefix ending at a word
//! boundary therefore re-uses the full line's own word shapes: the oracle
//! folds the same numbers in the same association and lands bit-identical
//! widths. A prefix that cuts through a word re-prices only that word's
//! truncated fragment through the same `ShapeWord` path the fresh truncated
//! line takes — exact by construction, not by approximation. A naive
//! per-cluster prefix sum cannot be bit-stable: clusters are folded per word
//! and per span (association), and a truncated word's last cluster loses its
//! successor kern or ligature form; `tests` measures that divergence at legal
//! cuts where this oracle stays exact.
//!
//! Fallbacks keep the baseline behaviour verbatim: a line carrying a tab
//! resolves its stops per probe (`make_line`), and a measurer without the
//! [`crate::text::measure::TextMeasure::wrap_words`] capability — every toy
//! measurer in the unit suites — keeps the probe-by-`make_line` search.

use super::{make_line, Atom, AtomKind, BreakOptions, StyleId};
use crate::text::measure::WrapWords;

/// One coalesced wrap item of the measured atom range (mirrors `make_line`).
struct OracleItem {
    /// Atom range inside `[start, end)`, relative to the region start.
    first_atom: usize,
    /// Exclusive atom range end.
    last_atom: usize,
    kind: ItemKind,
}

enum ItemKind {
    /// Inline object with a fixed advance; never truncated (one atom).
    Object(f32),
    Text {
        text: String,
        table: WrapWords,
        /// The item's full measured advance (fold × 12700 + tracking).
        full_advance: f32,
        /// Byte offset in `text` of each atom boundary (len = atoms + 1).
        bytes: Vec<usize>,
        /// Code-point count at each atom boundary.
        chars: Vec<usize>,
    },
}

impl OracleItem {
    fn full_advance(&self) -> f32 {
        match &self.kind {
            ItemKind::Object(width) => *width,
            ItemKind::Text { full_advance, .. } => *full_advance,
        }
    }
}

/// A text item under construction while atoms coalesce.
struct PendingText {
    first_atom: usize,
    style: StyleId,
    text: String,
    bytes: Vec<usize>,
    chars: Vec<usize>,
    char_count: usize,
}

/// Word-level width data for the atom range `[start, end)` of one region.
pub(super) struct LineOracle {
    start: usize,
    items: Vec<OracleItem>,
}

impl LineOracle {
    /// Build the oracle for the fit probe over `atoms[start..]`. `None` when
    /// the range carries a tab (its stops resolve per probe) or the measurer
    /// cannot model an item's word structure.
    pub(super) fn build(atoms: &[Atom<'_>], start: usize, o: &BreakOptions<'_>) -> Option<Self> {
        let mut items: Vec<OracleItem> = Vec::new();
        let mut pending: Option<PendingText> = None;
        for (offset, atom) in atoms[start..].iter().enumerate() {
            match atom.kind {
                // A tab seats its cell through `measure_items`' stop grid,
                // which re-resolves against the probe's own item widths.
                AtomKind::Tab => return None,
                AtomKind::Object(width) => {
                    freeze(&mut items, pending.take(), o)?;
                    items.push(OracleItem {
                        first_atom: offset,
                        last_atom: offset + 1,
                        kind: ItemKind::Object(width),
                    });
                }
                AtomKind::Text(text) => match pending.as_mut() {
                    Some(item) if item.style == atom.style => {
                        item.char_count += text.chars().count();
                        item.text.push_str(text);
                        item.bytes.push(item.text.len());
                        item.chars.push(item.char_count);
                    }
                    _ => {
                        freeze(&mut items, pending.take(), o)?;
                        pending = Some(PendingText {
                            first_atom: offset,
                            style: atom.style,
                            text: text.to_string(),
                            bytes: vec![0, text.len()],
                            chars: vec![0, text.chars().count()],
                            char_count: text.chars().count(),
                        });
                    }
                },
            }
        }
        freeze(&mut items, pending.take(), o)?;
        Some(Self { start, items })
    }

    /// The exact `make_line(atoms, start, stop)` width for the probe prefix.
    pub(super) fn width_at(&self, stop: usize) -> f32 {
        let rel = stop - self.start;
        // `measure_items` folds item advances sequentially from zero; the
        // oracle folds full items then the truncated one, association for
        // association with `items.iter().map(item_width).sum()`.
        let mut acc = 0.0f32;
        for item in &self.items {
            if rel <= item.first_atom {
                break;
            }
            if rel < item.last_atom {
                if let ItemKind::Text {
                    text,
                    table,
                    bytes,
                    chars,
                    ..
                } = &item.kind
                {
                    let j = rel - item.first_atom;
                    let mut advance = text_prefix_px(table, text, bytes[j]) * 12700.0;
                    if table.spacing_emu != 0.0 {
                        advance += table.spacing_emu * (chars[j].saturating_sub(1)) as f32;
                    }
                    acc += advance;
                }
                break;
            }
            acc += item.full_advance();
        }
        acc
    }
}

/// Freeze the pending text run into an oracle item through the measurer's
/// word-level capability; `None` makes the caller fall back to probing.
fn freeze(items: &mut Vec<OracleItem>, pending: Option<PendingText>, o: &BreakOptions<'_>) -> Option<()> {
    let Some(item) = pending else {
        return Some(());
    };
    let table = o.measurer.wrap_words(&item.text, item.style)?;
    let mut full_advance = table.total * 12700.0;
    if table.spacing_emu != 0.0 {
        full_advance += table.spacing_emu * (item.char_count.saturating_sub(1)) as f32;
    }
    let last_atom = item.first_atom + item.bytes.len() - 1;
    items.push(OracleItem {
        first_atom: item.first_atom,
        last_atom,
        kind: ItemKind::Text {
            text: item.text,
            table,
            full_advance,
            bytes: item.bytes,
            chars: item.chars,
        },
    });
    Some(())
}

/// The truncated item's shape fold in px: full spans folded span by span, the
/// cut span folded word by word, the cut word whole or as its fragment probe.
///
/// One truncation effect needs explicit care (UAX #9 L1): a run of whitespace
/// at the END of the truncated line re-levels to the paragraph base. When the
/// full line kept that run inside a span of another level — the space between
/// two Arabic runs in an LTR line, say — the fresh line folds it as its own
/// final span, and the oracle must split it out of the span fold to keep the
/// summation association (hence the bits) identical.
fn text_prefix_px(table: &WrapWords, text: &str, cut: usize) -> f32 {
    let base = base_level(table.line_rtl);
    let mut acc = 0.0f32;
    for span in &table.spans {
        let words = &table.words[span.first..span.last];
        let Some(last) = words.last() else {
            continue;
        };
        if last.end <= cut {
            // Whole span: the fresh line folds it from zero, then adds it to
            // the running line width (cosmic-text `add_to_visual_line`).
            // A cut at the span's end that leaves a trailing blank run inside
            // a non-base span re-levels that run: fold it as its own span.
            if last.end == cut && span.level != base {
                let (content, blanks) = split_trailing_blanks(words, words.len() - 1);
                if let Some(blanks) = blanks {
                    acc += fold_words(content);
                    acc += fold_words(blanks);
                    return acc;
                }
            }
            acc += fold_words(words);
            continue;
        }
        if cut <= words[0].start {
            break; // the cut is at or before this span: nothing of it fits
        }
        // Partial span, logical word order: find the last word contributing
        // to the prefix — whole when the cut lands at its end, else the
        // truncated-word fragment.
        let mut last = words.len() - 1;
        while words[last].start >= cut {
            last -= 1;
        }
        if words[last].end > cut {
            // The cut is inside this word: only its truncated fragment
            // remains, shaped exactly as the fresh line shapes that word.
            let mut span_width = fold_words(&words[..last]);
            span_width += (span.fragment)(text, words[last].start, cut, table.line_rtl);
            acc += span_width;
        } else if span.level != base {
            // Word-boundary cut: a trailing blank run re-levels to the base
            // and the fresh line folds it as its own final span.
            let (content, blanks) = split_trailing_blanks(words, last);
            acc += fold_words(content);
            if let Some(blanks) = blanks {
                acc += fold_words(blanks);
            }
        } else {
            acc += fold_words(&words[..=last]);
        }
        return acc;
    }
    acc
}

/// The paragraph's base embedding level: 0 for LTR, 1 for RTL.
fn base_level(line_rtl: bool) -> u8 {
    u8::from(line_rtl)
}

/// Fold word widths from zero, in order — the association `word.width` uses.
fn fold_words(words: &[crate::text::measure::WrapWord]) -> f32 {
    let mut width = 0.0f32;
    for word in words {
        width += word.width;
    }
    width
}

/// Split a trailing blank-word run off `[..=last]`: `(content, Some(blanks))`
/// when `[..=last]` ends in blanks, else `(words, None)`.
fn split_trailing_blanks(
    words: &[crate::text::measure::WrapWord],
    last: usize,
) -> (
    &[crate::text::measure::WrapWord],
    Option<&[crate::text::measure::WrapWord]>,
) {
    if !words[last].blank {
        return (&words[..=last], None);
    }
    let mut start = last;
    while start > 0 && words[start - 1].blank {
        start -= 1;
    }
    (&words[..start], Some(&words[start..=last]))
}

/// Greatest fitting prefix: an exponential probe then a binary search,
/// measuring only prefixes near the fit boundary (break.ts:297-330). Widths
/// come from the word-level oracle when the line has no tabs, else from
/// `make_line` probes; either way the comparison is the budget's own
/// `<=` on the exact same f32 width `measure_items` would report.
pub(super) fn fit_prefix(
    atoms: &[Atom<'_>],
    start: usize,
    budget: f32,
    line_index: usize,
    o: &BreakOptions<'_>,
) -> usize {
    let end = atoms.len();
    let oracle = LineOracle::build(atoms, start, o);
    let width_at = |stop: usize| match &oracle {
        Some(w) => w.width_at(stop),
        None => make_line(atoms, start, stop, line_index, o).width,
    };
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

#[cfg(test)]
mod tests;
