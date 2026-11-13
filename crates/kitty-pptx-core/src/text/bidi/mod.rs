//! UAX#9 line ordering for the DrawingML paint pass.
//!
//! Port of `packages/pptx/src/bidi-line.ts` (reference revision 1f5c6e1d), which
//! is a thin wrapper over `@silurus/ooxml-core/internal/bidi-line`
//! (`packages/core/src/text/bidi/segment-line.ts`) and the shared building
//! blocks in `line-order.ts`. The pptx wrapper passes exactly one host hook:
//! `isTab`, so a DrawingML tab is a Segment Separator (Bidi_Class `S`), not a
//! neutral object. Everything else is plain UAX#9.
//!
//! Segment granularity is the same approximation the reference documents: the
//! order is a permutation of the laid-out segments and each carries its resolved
//! direction, never a per-glyph split inside one styled run.

mod class;
mod explicit;
mod levels;
mod sequence;

pub use class::bidi_class;

/// UAX#9 bidirectional character classes (`Bidi_Class` property values), in the
/// reference's canonical index order (`char-data.generated.ts:9-13`).
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BidiClass {
    L,
    R,
    Al,
    En,
    Es,
    Et,
    An,
    Cs,
    Nsm,
    Bn,
    B,
    S,
    Ws,
    On,
    Lre,
    Lro,
    Rle,
    Rlo,
    Pdf,
    Lri,
    Rli,
    Fsi,
    Pdi,
}

/// `RTL_GATE` (`line-order.ts:16-24`): Hebrew/Arabic/Syriac/Thaana/NKo/Samaritan
/// and their presentation forms, the RTL-implicating controls (RLM/RLE/RLO/RLI)
/// and the Plane-1 near-eastern blocks. Only a cheap gate — never used for
/// ordering itself.
pub fn has_strong_rtl(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch as u32,
            0x0590..=0x08FF
                | 0xFB1D..=0xFDFF
                | 0xFE70..=0xFEFF
                | 0x200F
                | 0x202B
                | 0x202E
                | 0x2067
                | 0x10800..=0x10FFF
                | 0x1E800..=0x1EFFF
        )
    })
}

/// OBJECT REPLACEMENT CHARACTER (U+FFFC, Bidi_Class ON): every non-text segment
/// contributes exactly one so it takes the surrounding direction
/// (`line-order.ts:27`).
const OBJECT_PLACEHOLDER: char = '\u{fffc}';

/// One laid-out line segment as the bidi pass sees it.
///
/// The reference uses `{ text?: string; isTab?: boolean }`; a segment without
/// text (inline math, picture marker) is the neutral object placeholder and a
/// tab carries `isTab` so it becomes Bidi_Class `S`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BidiSegment<'a> {
    /// Styled text segment, in logical order.
    Text(&'a str),
    /// Non-text segment (inline equation, picture marker): U+FFFC.
    Object,
    /// Inline tab: Bidi_Class `S` (§17.3.1.37, UAX#9 L1).
    Tab,
}

/// UAX#9 L2 result: `order` is the logical index drawn at each visual position
/// (left to right) and `rtl[i]` is segment `i`'s resolved direction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineVisualOrder {
    pub order: Vec<usize>,
    pub rtl: Vec<bool>,
}

/// `segmentLineHasRtl` (`segment-line.ts:280`): does any segment carry a strong
/// RTL character? Objects have no text, so they never trigger the pass.
pub fn segments_have_rtl(segments: &[BidiSegment<'_>]) -> bool {
    segments.iter().any(|segment| match segment {
        BidiSegment::Text(text) => has_strong_rtl(text),
        BidiSegment::Object | BidiSegment::Tab => false,
    })
}

/// `computeSegmentLineVisualOrder` (`segment-line.ts:293`) with the DrawingML
/// `isTab` hook: concatenate the logical segments, resolve UAX#9 levels over the
/// line, then return the L2 visual order and each segment's direction.
pub fn compute_line_visual_order(
    segments: &[BidiSegment<'_>],
    base_rtl: bool,
) -> LineVisualOrder {
    let count = segments.len();
    if count == 0 {
        return LineVisualOrder::default();
    }
    let mut text: Vec<char> = Vec::new();
    let mut start = vec![0usize; count];
    // Sparse HL1 class overrides, one slot per logical code point.
    let mut overrides: Vec<Option<BidiClass>> = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        start[index] = text.len();
        let content = match segment {
            BidiSegment::Text(value) => Some(*value),
            BidiSegment::Object | BidiSegment::Tab => None,
        };
        match content {
            Some(value) if !value.is_empty() => {
                for ch in value.chars() {
                    text.push(ch);
                    overrides.push(None);
                }
            }
            _ => {
                text.push(OBJECT_PLACEHOLDER);
                overrides.push(None);
            }
        }
        if *segment == BidiSegment::Tab {
            if let Some(slot) = overrides.last_mut() {
                *slot = Some(BidiClass::S);
            }
        }
    }
    let cps: Vec<u32> = text.iter().map(|ch| *ch as u32).collect();
    let resolved = levels::resolve_levels(&cps, Some(&overrides), base_rtl);
    // `buildVisualOrder` (line-order.ts:52-66): a segment takes the level of its
    // first code point, an X9-removed one falls back to the paragraph level.
    let mut segment_levels = vec![0u8; count];
    for index in 0..count {
        let level = resolved.levels.get(start[index]).copied().unwrap_or(0);
        segment_levels[index] = if level == levels::REMOVED {
            resolved.paragraph_level
        } else {
            level as u8
        };
    }
    let order = levels::reorder_visual(&segment_levels);
    let rtl = segment_levels.iter().map(|level| level % 2 == 1).collect();
    LineVisualOrder { order, rtl }
}
