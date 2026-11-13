//! Extended grapheme clusters for [`super::break_paragraph`]. The reference
//! segments each text run with ICU through `graphemeClusterOffsets`
//! (`packages/core/src/text/sea-break.ts:552-579`); this module is a dependency
//! free UAX #29 implementation of the same default rules (GB3–GB13) with the
//! `Extend`/`SpacingMark` classes merged (both forbid a break before themselves,
//! so only a rare `ExtPict Extend SpacingMark*` chain could tell them apart).

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gcb {
    Other,
    L,
    V,
    T,
    Lv,
    Lvt,
}

fn hangul(cp: u32) -> Gcb {
    match cp {
        0x1100..=0x115f | 0xa960..=0xa97c => Gcb::L,
        0x1160..=0x11a7 | 0xd7b0..=0xd7c6 => Gcb::V,
        0x11a8..=0x11ff | 0xd7cb..=0xd7fb => Gcb::T,
        0xac00..=0xd7a3 if (cp - 0xac00).is_multiple_of(28) => Gcb::Lv,
        0xac00..=0xd7a3 => Gcb::Lvt,
        _ => Gcb::Other,
    }
}

fn is_gb_control(cp: u32) -> bool {
    (cp < 0x20 && cp != 0x0a)
        || matches!(cp, 0x7f..=0x9f | 0x200b | 0x200e | 0x200f | 0x2028..=0x202e
            | 0x2060..=0x206f | 0xfeff)
}

/// Grapheme_Cluster_Break `Extend` ∪ `SpacingMark` — a UCD subset covering Latin,
/// Hebrew, Arabic, the Indic blocks, Thai/Lao/Khmer/Myanmar/Tibetan, CJK
/// variation selectors and emoji modifiers. Marks outside it start a new atom.
fn is_combining(cp: u32) -> bool {
    matches!(cp, 0x300..=0x36f | 0x483..=0x489 | 0x591..=0x5bd | 0x5bf | 0x5c1..=0x5c2
        | 0x5c4..=0x5c5 | 0x5c7 | 0x610..=0x61a | 0x64b..=0x65f | 0x670 | 0x6d6..=0x6dc
        | 0x6df..=0x6e4 | 0x6e7..=0x6e8 | 0x6ea..=0x6ed | 0x711 | 0x730..=0x74a
        | 0x7a6..=0x7b0 | 0x7eb..=0x7f3 | 0x816..=0x819 | 0x81b..=0x823 | 0x825..=0x827
        | 0x829..=0x82d | 0x859..=0x85b | 0x8d3..=0x8e1 | 0x8e3..=0x902 | 0x93a | 0x93c
        | 0x941..=0x948 | 0x94d | 0x951..=0x957 | 0x962..=0x963 | 0x981 | 0x9bc
        | 0x9be..=0x9c0 | 0x9c7..=0x9c8 | 0x9cb..=0x9cd | 0x9e2..=0x9e3 | 0xa01..=0xa02
        | 0xa3c | 0xa3e..=0xa42 | 0xa47..=0xa48 | 0xa4b..=0xa4d | 0xa51 | 0xa70..=0xa71
        | 0xa75 | 0xa81..=0xa82 | 0xabc | 0xabe..=0xac5 | 0xac7..=0xac9 | 0xacb..=0xacd
        | 0xae2..=0xae3 | 0xb01 | 0xb3c | 0xb3e..=0xb44 | 0xb47..=0xb48 | 0xb4b..=0xb4d
        | 0xb56 | 0xb62..=0xb63 | 0xb82 | 0xbbe..=0xbc2 | 0xbc6..=0xbc8 | 0xbca..=0xbcd
        | 0xc00..=0xc04 | 0xc3e..=0xc44 | 0xc46..=0xc48 | 0xc4a..=0xc4d | 0xc55..=0xc56
        | 0xc62..=0xc63 | 0xc81 | 0xcbc | 0xcbe..=0xcc4 | 0xcc6..=0xcc8 | 0xcca..=0xccd
        | 0xce2..=0xce3 | 0xd00..=0xd01 | 0xd3b..=0xd44 | 0xd46..=0xd48 | 0xd4a..=0xd4d
        | 0xd62..=0xd63 | 0xdca | 0xdcf..=0xdd4 | 0xdd6 | 0xdd8..=0xddf
        | 0xdf2..=0xdf3 | 0xe31 | 0xe33..=0xe3a | 0xe47..=0xe4e | 0xeb1 | 0xeb3..=0xebc
        | 0xec8..=0xecd | 0xf18..=0xf19 | 0xf35 | 0xf37 | 0xf39 | 0xf3e..=0xf3f
        | 0xf71..=0xf84 | 0xf86..=0xf87 | 0xf8d..=0xfbc | 0xfc6 | 0x102b..=0x103e
        | 0x1056..=0x1059 | 0x105e..=0x1060 | 0x1062..=0x1064 | 0x1067..=0x106d
        | 0x1071..=0x1074 | 0x1082..=0x108d | 0x108f | 0x109a..=0x109d
        | 0x135d..=0x135f | 0x1712..=0x1714 | 0x1732..=0x1734 | 0x1752..=0x1753
        | 0x1772..=0x1773 | 0x17b4..=0x17d3 | 0x17dd | 0x180b..=0x180d | 0x18a9
        | 0x1920..=0x1922 | 0x1930..=0x1932 | 0x1a17..=0x1a1a | 0x1a55..=0x1a57
        | 0x1a61..=0x1a64 | 0x1ab0..=0x1aff | 0x1b00..=0x1b03 | 0x1b34..=0x1b41
        | 0x1b6b..=0x1b73 | 0x1dc0..=0x1dff | 0x200c | 0x20d0..=0x20f0 | 0x2cef..=0x2cf1
        | 0x302a..=0x302f | 0x3099..=0x309a | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f
        | 0xff9e..=0xff9f | 0x101fd | 0x1d167..=0x1d169 | 0x1d17b..=0x1d182
        | 0x1e8d0..=0x1e8d6 | 0x1f3fb..=0x1f3ff | 0xe0100..=0xe01ef)
}

fn is_prepend(cp: u32) -> bool {
    matches!(cp, 0x600..=0x605 | 0x6dd | 0x70f | 0x890..=0x891 | 0x8e2 | 0xd4e
        | 0x110bd | 0x110cd | 0x111c2..=0x111c3 | 0x1193f | 0x11941 | 0x11a3a
        | 0x11a84..=0x11a89 | 0x11d46 | 0x11f02)
}

fn is_regional_indicator(cp: u32) -> bool {
    matches!(cp, 0x1f1e6..=0x1f1ff)
}

/// Extended_Pictographic, coarse ranges: only GB11 (emoji ZWJ sequences) uses it.
fn is_ext_pict(cp: u32) -> bool {
    matches!(cp, 0xa9 | 0xae | 0x203c | 0x2049 | 0x2122 | 0x2139 | 0x2194..=0x21aa
        | 0x231a..=0x23ff | 0x24c2 | 0x25aa..=0x27bf | 0x2934..=0x2935 | 0x2b05..=0x2b55
        | 0x3030 | 0x303d | 0x3297 | 0x3299 | 0x1f000..=0x1faff | 0x1fc00..=0x1fffd)
}

/// UAX #29 boundary before `cur`; `ri_run` counts the regional indicators ending
/// at `prev`, `zwj_ready` records `ExtPict Extend* ZWJ` (GB11).
fn gb_break(prev: char, cur: char, ri_run: usize, zwj_ready: bool) -> bool {
    let (p, c) = (prev as u32, cur as u32);
    if p == 0x0d && c == 0x0a {
        return false; // GB3
    }
    if is_gb_control(p) || is_gb_control(c) {
        return true; // GB4 / GB5
    }
    let (hp, hc) = (hangul(p), hangul(c));
    if hp == Gcb::L && matches!(hc, Gcb::L | Gcb::V | Gcb::Lv | Gcb::Lvt) {
        return false; // GB6
    }
    if matches!(hp, Gcb::Lv | Gcb::V) && matches!(hc, Gcb::V | Gcb::T) {
        return false; // GB7
    }
    if matches!(hp, Gcb::Lvt | Gcb::T) && hc == Gcb::T {
        return false; // GB8
    }
    if is_combining(c) || c == 0x200d {
        return false; // GB9
    }
    if is_prepend(p) {
        return false; // GB9b
    }
    if p == 0x200d && zwj_ready && is_ext_pict(c) {
        return false; // GB11
    }
    if is_regional_indicator(p) && is_regional_indicator(c) && ri_run % 2 == 1 {
        return false; // GB12 / GB13
    }
    true
}

/// Byte ranges of the extended grapheme clusters of `text`; empty text yields no
/// span, so a zero-width run contributes no atom.
pub(crate) fn grapheme_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut prev: Option<char> = None;
    let mut ri_run = 0usize;
    let mut pict_base = false;
    let mut zwj_ready = false;
    for (index, c) in text.char_indices() {
        if let Some(previous) = prev {
            if gb_break(previous, c, ri_run, zwj_ready) {
                spans.push((start, index));
                start = index;
            }
        }
        let cp = c as u32;
        if is_ext_pict(cp) {
            pict_base = true;
            zwj_ready = false;
        } else if cp == 0x200d {
            zwj_ready = pict_base;
            pict_base = false;
        } else if is_combining(cp) {
            zwj_ready = false;
        } else {
            pict_base = false;
            zwj_ready = false;
        }
        ri_run = if is_regional_indicator(cp) {
            ri_run + 1
        } else {
            0
        };
        prev = Some(c);
    }
    if prev.is_some() {
        spans.push((start, text.len()));
    }
    spans
}
