//! UAX#9 explicit levels and isolating run sequences: BD9, X1-X8, X10/BD13.
//!
//! Split out of [`super::levels`]; 1:1 port of the same-named section of
//! `packages/core/src/text/bidi/uax9/rules.ts` (reference revision 1f5c6e1d).

use super::levels::{first_strong_level, is_isolate_initiator, Dir, MAX_DEPTH};
use super::BidiClass;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Override {
    Neutral,
    L,
    R,
}

#[derive(Clone, Copy)]
struct Entry {
    level: u8,
    override_: Override,
    isolate: bool,
}

fn least_greater_odd(x: u8) -> u8 {
    if x % 2 == 1 {
        x + 2
    } else {
        x + 1
    }
}

fn least_greater_even(x: u8) -> u8 {
    if x % 2 == 1 {
        x + 1
    } else {
        x + 2
    }
}

/// BD9: pair isolate initiators with their matching PDI.
pub(super) fn compute_matching(types: &[BidiClass]) -> (Vec<usize>, Vec<i32>) {
    let n = types.len();
    let mut pdi_of = vec![n; n];
    let mut init_of = vec![-1i32; n];
    let mut stack: Vec<usize> = Vec::new();
    for (i, &t) in types.iter().enumerate() {
        if is_isolate_initiator(t) {
            stack.push(i);
        } else if t == BidiClass::Pdi {
            if let Some(j) = stack.pop() {
                pdi_of[j] = i;
                init_of[i] = j as i32;
            }
        }
    }
    (pdi_of, init_of)
}

/// Rules X1-X8: explicit embedding levels plus override application.
pub(super) fn determine_explicit_levels(
    orig: &[BidiClass],
    paragraph_level: u8,
    pdi_of: &[usize],
) -> (Vec<i16>, Vec<BidiClass>) {
    let n = orig.len();
    let root = Entry {
        level: paragraph_level,
        override_: Override::Neutral,
        isolate: false,
    };
    let mut levels = vec![paragraph_level as i16; n];
    let mut types = orig.to_vec();
    let mut stack: Vec<Entry> = vec![root];
    let mut overflow_isolate = 0usize;
    let mut overflow_embedding = 0usize;
    let mut valid_isolate = 0usize;
    for i in 0..n {
        let t = orig[i];
        let top = stack.last().copied().unwrap_or(root);
        match t {
            BidiClass::Rle | BidiClass::Lre | BidiClass::Rlo | BidiClass::Lro => {
                levels[i] = top.level as i16; // removed by X9
                let rtl = t == BidiClass::Rle || t == BidiClass::Rlo;
                let new_level = if rtl {
                    least_greater_odd(top.level)
                } else {
                    least_greater_even(top.level)
                };
                if new_level <= MAX_DEPTH && overflow_isolate == 0 && overflow_embedding == 0 {
                    stack.push(Entry {
                        level: new_level,
                        override_: match t {
                            BidiClass::Rlo => Override::R,
                            BidiClass::Lro => Override::L,
                            _ => Override::Neutral,
                        },
                        isolate: false,
                    });
                } else if overflow_isolate == 0 {
                    overflow_embedding += 1;
                }
            }
            BidiClass::Rli | BidiClass::Lri | BidiClass::Fsi => {
                levels[i] = top.level as i16;
                match top.override_ {
                    Override::L => types[i] = BidiClass::L,
                    Override::R => types[i] = BidiClass::R,
                    Override::Neutral => {}
                }
                let dir = match t {
                    BidiClass::Rli => 1,
                    BidiClass::Lri => 0,
                    _ => first_strong_level(orig, i + 1, pdi_of[i]),
                };
                let new_level = if dir == 1 {
                    least_greater_odd(top.level)
                } else {
                    least_greater_even(top.level)
                };
                if new_level <= MAX_DEPTH && overflow_isolate == 0 && overflow_embedding == 0 {
                    valid_isolate += 1;
                    stack.push(Entry {
                        level: new_level,
                        override_: Override::Neutral,
                        isolate: true,
                    });
                } else {
                    overflow_isolate += 1;
                }
            }
            BidiClass::Pdi => {
                if overflow_isolate > 0 {
                    overflow_isolate -= 1;
                } else if valid_isolate > 0 {
                    overflow_embedding = 0;
                    while stack.last().is_some_and(|entry| !entry.isolate) {
                        stack.pop();
                    }
                    stack.pop();
                    valid_isolate -= 1;
                }
                let top = stack.last().copied().unwrap_or(root);
                levels[i] = top.level as i16;
                match top.override_ {
                    Override::L => types[i] = BidiClass::L,
                    Override::R => types[i] = BidiClass::R,
                    Override::Neutral => {}
                }
            }
            BidiClass::Pdf => {
                levels[i] = top.level as i16; // removed by X9
                if overflow_isolate > 0 {
                    // no-op
                } else if overflow_embedding > 0 {
                    overflow_embedding -= 1;
                } else if !top.isolate && stack.len() >= 2 {
                    stack.pop();
                }
            }
            BidiClass::B => {
                // X8: a paragraph separator terminates every embedding.
                stack.truncate(1);
                overflow_isolate = 0;
                overflow_embedding = 0;
                valid_isolate = 0;
                levels[i] = paragraph_level as i16;
            }
            BidiClass::Bn => levels[i] = top.level as i16, // removed by X9
            _ => {
                // X6: everything else.
                levels[i] = top.level as i16;
                match top.override_ {
                    Override::L => types[i] = BidiClass::L,
                    Override::R => types[i] = BidiClass::R,
                    Override::Neutral => {}
                }
            }
        }
    }
    (levels, types)
}

/// One isolating run sequence (X10 / BD13).
pub(super) struct Sequence {
    pub(super) indices: Vec<usize>,
    pub(super) level: u8,
    pub(super) sos: Dir,
    pub(super) eos: Dir,
}

fn dir_of(level: u8) -> Dir {
    level % 2
}

/// BD16 canonical-equivalence fold for bracket matching (only two pairs).
pub(super) fn canon_bracket(cp: u32) -> u32 {
    match cp {
        0x2329 => 0x3008,
        0x232a => 0x3009,
        other => other,
    }
}

/// X10 + BD13: build the isolating run sequences with their sos/eos directions.
pub(super) fn build_sequences(
    orig: &[BidiClass],
    levels: &[i16],
    removed: &[bool],
    paragraph_level: u8,
    pdi_of: &[usize],
    init_of: &[i32],
) -> Vec<Sequence> {
    let n = orig.len();
    let kept: Vec<usize> = (0..n).filter(|i| !removed[*i]).collect();
    // Level runs: maximal same-level spans of the reduced (X9) text.
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (k, &idx) in kept.iter().enumerate() {
        if k == 0 || levels[idx] != levels[kept[k - 1]] {
            runs.push(vec![idx]);
        } else if let Some(last) = runs.last_mut() {
            last.push(idx);
        }
    }
    let mut sequences: Vec<Sequence> = Vec::new();
    for run in &runs {
        let first_idx = run[0];
        // A run beginning with a matching PDI is appended to its initiator's
        // sequence, not a sequence start of its own.
        if orig[first_idx] == BidiClass::Pdi && init_of[first_idx] != -1 {
            continue;
        }
        let mut indices = run.clone();
        let mut cursor = run.clone();
        loop {
            let last_idx = cursor[cursor.len() - 1];
            if is_isolate_initiator(orig[last_idx]) && pdi_of[last_idx] != n {
                let next = runs.iter().find(|r| r[0] == pdi_of[last_idx]);
                if let Some(next) = next {
                    indices.extend(next.iter().copied());
                    cursor = next.clone();
                    continue;
                }
            }
            break;
        }
        let seq_level = levels[indices[0]] as u8;
        let mut prev_level = paragraph_level;
        for j in (0..indices[0]).rev() {
            if !removed[j] {
                prev_level = levels[j] as u8;
                break;
            }
        }
        let sos = dir_of(seq_level.max(prev_level));
        let last_idx = indices[indices.len() - 1];
        let mut next_level = paragraph_level;
        if !(is_isolate_initiator(orig[last_idx]) && pdi_of[last_idx] == n) {
            for j in last_idx + 1..n {
                if !removed[j] {
                    next_level = levels[j] as u8;
                    break;
                }
            }
        }
        let eos = dir_of(seq_level.max(next_level));
        sequences.push(Sequence {
            indices,
            level: seq_level,
            sos,
            eos,
        });
    }
    sequences
}
