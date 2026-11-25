//! Break-decision rule kernels for [`super::break_paragraph`]: UAX #14
//! no-break pairs (`packages/core/src/text/line-break.ts`), the CJK and Latin
//! classifiers (`text/cjk-ranges.ts`), the Southeast Asian script opportunities
//! (`text/sea-break.ts`) and the Japanese kinsoku engine
//! (`text/kinsoku/{rules,split}.ts`). Data and predicates only; no layout state.

/// CJK / ideographic code points that allow a break against an adjacent
/// character without whitespace (cjk-ranges.ts:57-64). Hangul ends at U+D7A3:
/// U+D7B0–D7FF are conjoining Jamo and are not break units.
pub(crate) fn is_cjk_break_char(cp: u32) -> bool {
    matches!(cp, 0x3000..=0x9fff | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xff00..=0xffef)
}

/// Latin script — approximates the `\p{Script_Extensions=Latin}` test of the
/// HY/HH hyphen rule (break.ts:224-230).
pub(super) fn is_latin(c: char) -> bool {
    matches!(c as u32, 0x41..=0x5a | 0x61..=0x7a | 0xc0..=0x24f | 0x1e00..=0x1eff
        | 0x2c60..=0x2c7f | 0xa720..=0xa7ff | 0xab30..=0xab6f | 0xfb00..=0xfb06
        | 0xff21..=0xff3a | 0xff41..=0xff5a)
}

/// The UAX #14 classes the supported rules discriminate
/// (`line-break-class.generated.ts`, after the LB1 default resolutions).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Lb {
    Other,
    Op,
    Cp,
    Al,
    Nu,
    Pr,
    Po,
    Hy,
    Hh,
    Is,
    Id,
}

/// Effective line-break class. AL and HL are merged (every supported rule treats
/// them alike); the ideographic class is `is_cjk_break_char`; complex-context
/// scripts (Thai/Lao/Khmer/…) are `Other`, as UAX #14 gives them `SA`.
pub(super) fn lb_class(cp: u32) -> Lb {
    match cp {
        0x28 | 0x5b | 0x7b | 0xbf | 0x201a | 0x201e | 0x2329 | 0x3008 | 0x300a | 0x300c | 0x300e | 0x3010 | 0x3014
        | 0x3016 | 0x3018 | 0x301a | 0x301d | 0xff08 | 0xff3b | 0xff5b | 0xff5f | 0xff62 => Lb::Op,
        0x29 | 0x5d | 0x7d | 0xff09 | 0xff3d | 0xff5d | 0xff60 => Lb::Cp,
        0x23 => Lb::Al,
        0x2d => Lb::Hy,
        0x2010 => Lb::Hh,
        0x2c | 0x2e | 0x3a | 0x3b => Lb::Is,
        0x24 | 0x2b | 0x5c | 0xa3 | 0xa5 | 0xff04 | 0xffe1 | 0xffe5..=0xffe6 | 0x20a0..=0x20a6 => Lb::Pr,
        0x25 | 0xa2 | 0xb0 | 0xff05 | 0xffe0 | 0x2030..=0x2035 | 0x20a7 | 0x2103 | 0x2109 => Lb::Po,
        // CJK punctuation is CL/NS/CM in UAX #14, classes no supported rule uses:
        // it proves nothing rather than becoming ideographic (ID).
        0x3001..=0x3002
        | 0x3005
        | 0x3009
        | 0x300b
        | 0x300d
        | 0x300f
        | 0x3011
        | 0x3015
        | 0x3017
        | 0x3019
        | 0x301b..=0x301c
        | 0x301e..=0x301f
        | 0x302a..=0x302f
        | 0x3035
        | 0x303b..=0x303c
        | 0xff01
        | 0xff0c
        | 0xff0e
        | 0xff1a..=0xff1b
        | 0xff1f
        | 0xff61
        | 0xff63..=0xff64 => Lb::Other,
        _ if is_cjk_break_char(cp) => Lb::Id,
        _ => match char::from_u32(cp) {
            Some(c) if !is_sea_script_cp(cp) && c.is_numeric() => Lb::Nu,
            Some(c) if !is_sea_script_cp(cp) && c.is_alphabetic() => Lb::Al,
            _ => Lb::Other,
        },
    }
}

/// East_Asian_Width ∈ {F, W, H} — the `$EastAsian` set LB30 excludes
/// (line-break.ts:71-79).
fn is_east_asian_fwh(cp: u32) -> bool {
    matches!(cp, 0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0x303e | 0x3041..=0x33ff | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff | 0xa000..=0xa4cf | 0xa960..=0xa97f | 0xac00..=0xd7a3
        | 0xf900..=0xfaff | 0xfe10..=0xfe19 | 0xfe30..=0xfe6f | 0xff00..=0xff60
        | 0xff61..=0xff9f | 0xffe0..=0xffe6 | 0x1b000..=0x1b001 | 0x1f200..=0x1f251
        | 0x20000..=0x3fffd)
}

/// Conservative one-way UAX #14 no-break proof (line-break.ts:57-100). `true`
/// proves the boundary is prohibited; `false` only means "not proved here".
/// Supported: LB14 `OP ×`; LB23 `(AL|HL) × NU`, `NU × (AL|HL)`; LB23a
/// `PR × (ID|EB|EM)`, `(ID|EB|EM) × PO`; LB24 `(PR|PO) × (AL|HL)` and its
/// reverse; LB25 `NU × (NU|PO|PR)` and `(PO|PR|HY|IS) × NU`; LB28
/// `(AL|HL) × (AL|HL)`; LB30 `(AL|HL|NU) × OP`, `CP × (AL|HL|NU)` gated on the
/// bracket not being East_Asian_Width F/W/H. The `NU (SY|IS)* CL/CP × PO/PR`
/// lines and LB13 need left context and stay deferred, exactly as in the
/// reference. EB/EM are folded into ID.
pub(super) fn is_uax14_no_break_pair(prev_cp: u32, next_cp: u32) -> bool {
    let prev = lb_class(prev_cp);
    if prev == Lb::Op {
        return true; // LB14
    }
    let next = lb_class(next_cp);
    let (prev_alpha, next_alpha) = (prev == Lb::Al, next == Lb::Al);
    if prev_alpha && (next_alpha || matches!(next, Lb::Nu | Lb::Pr | Lb::Po)) {
        return true; // LB28 / LB23 / LB24
    }
    if next_alpha && matches!(prev, Lb::Nu | Lb::Pr | Lb::Po) {
        return true;
    }
    if (prev == Lb::Pr && next == Lb::Id) || (prev == Lb::Id && next == Lb::Po) {
        return true; // LB23a
    }
    if prev == Lb::Nu && matches!(next, Lb::Nu | Lb::Po | Lb::Pr) {
        return true; // LB25
    }
    if next == Lb::Nu && matches!(prev, Lb::Po | Lb::Pr | Lb::Hy | Lb::Is) {
        return true;
    }
    if (prev_alpha || prev == Lb::Nu) && next == Lb::Op && !is_east_asian_fwh(next_cp) {
        return true; // LB30
    }
    prev == Lb::Cp && !is_east_asian_fwh(prev_cp) && (next_alpha || next == Lb::Nu)
}

// ── Southeast Asian scripts (text/sea-break.ts) ─────────────────────────────

/// Thai/Lao/Khmer dictionary scripts plus the Myanmar/Tibetan grapheme-fill
/// scripts (sea-break.ts:56-68).
fn is_sea_script_cp(cp: u32) -> bool {
    matches!(cp, 0x0e00..=0x0e7f | 0x0e80..=0x0eff | 0x1780..=0x17ff | 0x1000..=0x109f
        | 0xaa60..=0xaa7f | 0xa9e0..=0xa9ff | 0x0f00..=0x0fff)
}

/// Grapheme-fill scripts take no dictionary: every interior cluster boundary is
/// a break opportunity (sea-break.ts:44-48).
fn is_grapheme_fill_cp(cp: u32) -> bool {
    matches!(cp, 0x1000..=0x109f | 0xaa60..=0xaa7f | 0xa9e0..=0xa9ff | 0x0f00..=0x0fff)
}

/// Whitespace and the non-breaking space family beside which a SEA transition
/// gains no break (sea-break.ts:335-341).
fn is_break_inert_space(c: char) -> bool {
    matches!(
        c as u32,
        0x20 | 0x09 | 0x0a | 0x0d | 0x3000 | 0xa0 | 0x2007 | 0x202f | 0x2060 | 0xfeff
    )
}

/// Seam-level equivalent of `seaMixedBreakOffsets(full, {cjk: true})`
/// (sea-break.ts:392-443) for one adjacent atom pair. Atoms are grapheme
/// clusters, so the reference's grapheme-safety filter is satisfied by
/// construction and its CJK per-character offsets are already covered by the CJK
/// rule. Dictionary word boundaries need ICU's Thai/Lao/Khmer dictionaries — no
/// Rust equivalent is available in this crate — so a dictionary span adds
/// nothing, which is the reference's own graceful fallback when
/// `Intl.Segmenter` is missing.
pub(super) fn sea_seam(before: Option<char>, after: Option<char>) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return false;
    };
    let (before_sea, after_sea) = (is_sea_script_cp(before as u32), is_sea_script_cp(after as u32));
    if !before_sea && !after_sea {
        return false;
    }
    if before_sea != after_sea {
        return !is_break_inert_space(before) && !is_break_inert_space(after);
    }
    is_grapheme_fill_cp(before as u32) && is_grapheme_fill_cp(after as u32)
}

// ── Kinsoku (text/kinsoku/{rules,split}.ts) ─────────────────────────────────

/// §17.15.1.59 行頭禁則 — characters that may NOT begin a line (Word's default
/// Japanese table, rules.ts:44-63).
const KINSOKU_LINE_START: &str = "”’）〕］｝〉》」』】〙〗〟｠»\
、。，．・：；／？！‐ー゠–〜～\
ぁぃぅぇぉっゃゅょゎゕゖ\
ァィゥェォッャュョヮヵヶ\
ㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ\
々〻ゝゞヽヾ゛゜\
％‰℃°′″\
｡｣､･ｰﾞﾟ\
!),.:;?]}｠";

/// §17.15.1.58 行末禁則 — characters that may NOT end a line (rules.ts:67-72).
const KINSOKU_LINE_END: &str = "“‘（〔［｛〈《「『【〘〖〝｟«\
＄￥＃￡￠\
([{｟";

/// `kinsokuAdjustedSplit` (split.ts:26-53): the largest legal split at or before
/// `split_at` whose tail does not start with a line-start-forbidden char and
/// whose head does not end with a line-end-forbidden one. Retraction is bounded
/// by `min_split`; a pathological forbidden run keeps the unrestricted split.
pub(super) fn kinsoku_adjusted_split(chars: &[char], split_at: usize, min_split: usize) -> usize {
    if split_at == 0 || split_at >= chars.len() {
        return split_at;
    }
    let start_forbidden = |i: usize| chars.get(i).is_some_and(|c| KINSOKU_LINE_START.contains(*c));
    let end_forbidden = |i: usize| i < chars.len() && KINSOKU_LINE_END.contains(chars[i]);
    let mut split = split_at;
    while split > min_split && (start_forbidden(split) || end_forbidden(split - 1)) {
        split -= 1;
    }
    if split <= min_split && (start_forbidden(split) || (split > 0 && end_forbidden(split - 1))) {
        return split_at;
    }
    split
}
