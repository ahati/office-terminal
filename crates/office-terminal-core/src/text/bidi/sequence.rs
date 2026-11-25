//! UAX#9 weak, neutral and implicit resolution over one isolating run sequence:
//! W1-W7, N0-N2, I1-I2.
//!
//! Split out of [`super::levels`]; 1:1 port of `resolveSequence` in
//! `packages/core/src/text/bidi/uax9/rules.ts` (reference revision 1f5c6e1d).

use super::class::bracket;
use super::explicit::{canon_bracket, Sequence};
use super::levels::{is_ni, Dir};
use super::BidiClass;

/// `strongDirOf` (rules.ts:264): L -> L, R -> R, EN/AN -> R, else none.
fn strong_dir_of(t: BidiClass) -> Option<Dir> {
    match t {
        BidiClass::L => Some(0),
        BidiClass::R | BidiClass::En | BidiClass::An => Some(1),
        _ => None,
    }
}

/// W1-W7, N0-N2, I1-I2 over one isolating run sequence.
pub(super) fn resolve_sequence(
    seq: &Sequence,
    cps: &[u32],
    orig: &[BidiClass],
    wt: &mut [BidiClass],
    levels: &mut [i16],
) {
    let ind = &seq.indices;
    let m = ind.len();
    let (sos, eos, level) = (seq.sos, seq.eos, seq.level);

    // W1: NSM takes the previous type (ON after an isolate initiator/PDI, sos first).
    let mut previous: Option<BidiClass> = None;
    for &idx in ind {
        if wt[idx] == BidiClass::Nsm {
            wt[idx] = match previous {
                None if sos == 0 => BidiClass::L,
                None => BidiClass::R,
                Some(BidiClass::Lri | BidiClass::Rli | BidiClass::Fsi | BidiClass::Pdi) => BidiClass::On,
                Some(other) => other,
            };
        }
        previous = Some(wt[idx]);
    }

    // W2 then W3: keep the pre-W3 strong type so an AL turned R stays visible.
    {
        let mut last_strong = if sos == 0 { BidiClass::L } else { BidiClass::R };
        for &idx in ind {
            let t = wt[idx];
            if matches!(t, BidiClass::R | BidiClass::L | BidiClass::Al) {
                last_strong = t;
            } else if t == BidiClass::En && last_strong == BidiClass::Al {
                wt[idx] = BidiClass::An;
            }
            if wt[idx] == BidiClass::Al {
                wt[idx] = BidiClass::R;
            }
        }
    }

    // W4: a single ES between EN EN, or CS between EN EN / AN AN, joins them.
    for p in 1..m.saturating_sub(1) {
        let t = wt[ind[p]];
        let before = wt[ind[p - 1]];
        let after = wt[ind[p + 1]];
        let en_pair = before == BidiClass::En && after == BidiClass::En && matches!(t, BidiClass::Es | BidiClass::Cs);
        if en_pair {
            wt[ind[p]] = BidiClass::En;
        } else if t == BidiClass::Cs && before == BidiClass::An && after == BidiClass::An {
            wt[ind[p]] = BidiClass::An;
        }
    }

    // W5: a run of ET adjacent to EN becomes EN.
    let mut p = 0;
    while p < m {
        if wt[ind[p]] != BidiClass::Et {
            p += 1;
            continue;
        }
        let mut q = p;
        while q < m && wt[ind[q]] == BidiClass::Et {
            q += 1;
        }
        let before = if p > 0 {
            wt[ind[p - 1]]
        } else if sos == 0 {
            BidiClass::L
        } else {
            BidiClass::R
        };
        let after = if q < m {
            wt[ind[q]]
        } else if eos == 0 {
            BidiClass::L
        } else {
            BidiClass::R
        };
        if before == BidiClass::En || after == BidiClass::En {
            for r in p..q {
                wt[ind[r]] = BidiClass::En;
            }
        }
        p = q;
    }

    // W6 then W7 share one traversal: W6 changes no strong L/R context.
    {
        let mut last_strong = if sos == 0 { BidiClass::L } else { BidiClass::R };
        for &idx in ind {
            let t = wt[idx];
            if matches!(t, BidiClass::Es | BidiClass::Et | BidiClass::Cs) {
                wt[idx] = BidiClass::On;
            }
            let t = wt[idx];
            if t == BidiClass::R || t == BidiClass::L {
                last_strong = t;
            } else if t == BidiClass::En && last_strong == BidiClass::L {
                wt[idx] = BidiClass::L;
            }
        }
    }

    // N0: paired brackets (BD16 pairing, then resolution).
    let e: Dir = level % 2;
    let o: Dir = 1 - e;
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    {
        // BD16 stack of (expected close, position), at most 63 open brackets.
        let mut stack: Vec<(u32, usize)> = Vec::new();
        for (p, &idx) in ind.iter().enumerate() {
            if wt[idx] != BidiClass::On {
                continue;
            }
            let Some((pair, open)) = bracket(cps[idx]) else {
                continue;
            };
            if open {
                if stack.len() == 63 {
                    break;
                }
                stack.push((canon_bracket(pair), p));
            } else {
                let close = canon_bracket(cps[idx]);
                if let Some(slot) = stack.iter().rposition(|(expect, _)| *expect == close) {
                    pairs.push((stack[slot].1, p));
                    stack.truncate(slot);
                }
            }
        }
        pairs.sort_unstable();
    }
    let apply_nsm_after = |wt: &mut [BidiClass], p: usize, dir: Dir| {
        for k in p + 1..m {
            if orig[ind[k]] != BidiClass::Nsm {
                break;
            }
            wt[ind[k]] = if dir == 0 { BidiClass::L } else { BidiClass::R };
        }
    };
    for (open, close) in pairs {
        let mut found_e = false;
        let mut found_o = false;
        for k in open + 1..close {
            match strong_dir_of(wt[ind[k]]) {
                Some(dir) if dir == e => found_e = true,
                Some(dir) if dir == o => found_o = true,
                _ => {}
            }
        }
        let set_dir = if found_e {
            Some(e)
        } else if found_o {
            let mut ctx = sos;
            for k in (0..open).rev() {
                if let Some(dir) = strong_dir_of(wt[ind[k]]) {
                    ctx = dir;
                    break;
                }
            }
            Some(if ctx == o { o } else { e })
        } else {
            None
        };
        if let Some(dir) = set_dir {
            let value = if dir == 0 { BidiClass::L } else { BidiClass::R };
            wt[ind[open]] = value;
            wt[ind[close]] = value;
            apply_nsm_after(wt, open, dir);
            apply_nsm_after(wt, close, dir);
        }
    }

    // N1 then N2: each NI run takes equal surrounding directions, else its
    // embedding direction (EN/AN count as R).
    let mut p = 0;
    while p < m {
        if !is_ni(wt[ind[p]]) {
            p += 1;
            continue;
        }
        let mut q = p;
        while q < m && is_ni(wt[ind[q]]) {
            q += 1;
        }
        let before = if p > 0 {
            strong_dir_of(wt[ind[p - 1]])
        } else {
            Some(sos)
        };
        let after = if q < m { strong_dir_of(wt[ind[q]]) } else { Some(eos) };
        let direction = match (before, after) {
            (Some(a), Some(b)) if a == b => a,
            _ => e,
        };
        let value = if direction == 0 { BidiClass::L } else { BidiClass::R };
        for r in p..q {
            wt[ind[r]] = value;
        }
        p = q;
    }

    // I1 / I2: implicit levels.
    for &idx in ind {
        let t = wt[idx];
        if level % 2 == 0 {
            if t == BidiClass::R {
                levels[idx] += 1;
            } else if t == BidiClass::An || t == BidiClass::En {
                levels[idx] += 2;
            }
        } else if matches!(t, BidiClass::L | BidiClass::En | BidiClass::An) {
            levels[idx] += 1;
        }
    }
}
