//! UAX#9 rule engine ("isolates" revision) over a paragraph's code points:
//! P2-P3, X1-X10, W1-W7, N0-N2, I1-I2 and the L1 reset.
//!
//! 1:1 port of `packages/core/src/text/bidi/uax9/rules.ts` (reference revision
//! 1f5c6e1d), which is conformance-tested against Unicode's `BidiTest.txt` /
//! `BidiCharacterTest.txt`. Rule names below are UAX#9 rule ids.

use super::class::bidi_class;
use super::explicit::{build_sequences, compute_matching, determine_explicit_levels};
use super::sequence::resolve_sequence;
use super::BidiClass;

/// X1 depth cap (UAX#9 §3.3.2, "explicit levels and directions").
pub(super) const MAX_DEPTH: u8 = 125;
/// Code point removed by X9 (no glyph), the reference's `REMOVED`.
pub(super) const REMOVED: i16 = -1;

/// Resolved per-code-point levels plus the paragraph level.
pub(super) struct Resolved {
    pub(super) levels: Vec<i16>,
    pub(super) paragraph_level: u8,
}

/// UAX#9 direction of a resolved type (`strongDirOf`, rules.ts:264).
pub(super) type Dir = u8; // 0 = L, 1 = R

pub(super) fn is_removed(t: BidiClass) -> bool {
    matches!(
        t,
        BidiClass::Rle
            | BidiClass::Lre
            | BidiClass::Rlo
            | BidiClass::Lro
            | BidiClass::Pdf
            | BidiClass::Bn
    )
}

pub(super) fn is_isolate_initiator(t: BidiClass) -> bool {
    matches!(t, BidiClass::Lri | BidiClass::Rli | BidiClass::Fsi)
}

/// NI = neutral or isolate formatting (`isNI`, rules.ts:32).
pub(super) fn is_ni(t: BidiClass) -> bool {
    matches!(
        t,
        BidiClass::B
            | BidiClass::S
            | BidiClass::Ws
            | BidiClass::On
            | BidiClass::Fsi
            | BidiClass::Lri
            | BidiClass::Rli
            | BidiClass::Pdi
    )
}

/// P2-P3 over `[start, end)`: first strong (L/AL/R), skipping isolated runs.
pub(super) fn first_strong_level(types: &[BidiClass], start: usize, end: usize) -> u8 {
    let mut depth = 0usize;
    for &t in types.iter().take(end).skip(start) {
        if is_isolate_initiator(t) {
            depth += 1;
        } else if t == BidiClass::Pdi {
            depth = depth.saturating_sub(1);
        } else if depth == 0 {
            if t == BidiClass::L {
                return 0;
            }
            if t == BidiClass::R || t == BidiClass::Al {
                return 1;
            }
        }
    }
    0
}

/// Resolve per-code-point embedding levels for one paragraph (P2-I2 + L1).
///
/// `class_override` is the UAX#9 §4.3 HL1 hook: a per-code-point class that
/// replaces the assigned one before the algorithm runs (the DrawingML tab
/// separator). `Removed` code points carry [`REMOVED`].
pub(super) fn resolve_levels(
    cps: &[u32],
    class_override: Option<&[Option<BidiClass>]>,
    base_rtl: bool,
) -> Resolved {
    let n = cps.len();
    let mut orig: Vec<BidiClass> = Vec::with_capacity(n);
    for (i, &cp) in cps.iter().enumerate() {
        let over = class_override.and_then(|list| list.get(i).copied().flatten());
        orig.push(over.unwrap_or_else(|| bidi_class(cp)));
    }
    let paragraph_level = if base_rtl {
        1
    } else {
        first_strong_level(&orig, 0, n)
    };
    let (pdi_of, init_of) = compute_matching(&orig);
    let (mut levels, mut types) = determine_explicit_levels(&orig, paragraph_level, &pdi_of);
    let removed: Vec<bool> = orig.iter().map(|t| is_removed(*t)).collect();
    let sequences = build_sequences(&orig, &levels, &removed, paragraph_level, &pdi_of, &init_of);
    for sequence in &sequences {
        resolve_sequence(sequence, cps, &orig, &mut types, &mut levels);
    }

    // L1: segment/paragraph separators and the whitespace next to them (plus
    // isolate formatting and X9-removed code points) reset to the paragraph level.
    let resettable = |i: usize| {
        let t = orig[i];
        t == BidiClass::Ws || is_isolate_initiator(t) || t == BidiClass::Pdi || removed[i]
    };
    for i in 0..n {
        let t = orig[i];
        if t == BidiClass::B || t == BidiClass::S {
            levels[i] = paragraph_level as i16;
            let mut j = i;
            while j > 0 {
                j -= 1;
                if !resettable(j) {
                    break;
                }
                if !removed[j] {
                    levels[j] = paragraph_level as i16;
                }
            }
        }
    }
    let mut j = n;
    while j > 0 && resettable(j - 1) {
        j -= 1;
        if !removed[j] {
            levels[j] = paragraph_level as i16;
        }
    }
    for i in 0..n {
        if removed[i] {
            levels[i] = REMOVED;
        }
    }
    Resolved {
        levels: levels.to_vec(),
        paragraph_level,
    }
}

/// Rule L2: the visual order of `levels` as a permutation of logical indices.
///
/// Removed entries (any sentinel above `MAX_DEPTH + 1`) are skipped; otherwise
/// each level from the highest down to the lowest odd level reverses its
/// contiguous run of at least that level.
pub(super) fn reorder_visual(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len())
        .filter(|i| levels[*i] <= MAX_DEPTH + 1)
        .collect();
    if order.is_empty() {
        return order;
    }
    let mut highest = 0u8;
    let mut lowest_odd = MAX_DEPTH + 2;
    for &index in &order {
        let level = levels[index];
        highest = highest.max(level);
        if level % 2 == 1 && level < lowest_odd {
            lowest_odd = level;
        }
    }
    let mut lvl = highest;
    while lvl >= lowest_odd {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] < lvl {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < order.len() && levels[order[j]] >= lvl {
                j += 1;
            }
            order[i..j].reverse();
            i = j;
        }
        if lvl == 0 {
            break;
        }
        lvl -= 1;
    }
    order
}
