//! Measurement seam shared by the layout kernels (placeholder).

use crate::text::shape::ShapedWords;

/// The truncated-word price probe carried by a [`WrapSpan`]:
/// `(item_text, word_start, cut, line_rtl) -> px`.
pub type WrapFragment = Box<dyn Fn(&str, usize, usize, bool) -> f32>;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StyleId(pub usize);

pub trait TextMeasure {
    fn advance(&self, text: &str, style: StyleId) -> f32;
    fn space(&self, style: StyleId) -> f32;

    /// Host evidence that kinsoku may retract across a run seam. Defaults to
    /// false (C08); the real measurer enables it for matching East Asian runs.
    fn kinsoku_across_runs(&self, _left: StyleId, _right: StyleId) -> bool {
        false
    }

    /// Word-level width data for one coalesced wrap item, extracted from ONE
    /// full shaping ([`crate::text::shape::shape_words`]). The wrap phase
    /// ([`crate::text::linebreak`]) prices every atom-prefix cut through it
    /// instead of re-shaping each probe prefix; `None` — the default — makes
    /// the caller fall back to probe-by-`advance` measurement, so toy
    /// measurers keep their exact baseline behaviour.
    fn wrap_words(&self, _text: &str, _style: StyleId) -> Option<WrapWords> {
        None
    }
}

/// One UAX #14 word of a wrap item (see [`crate::text::shape::shape_words`]).
#[derive(Clone)]
pub struct WrapWord {
    /// Byte range in the item text, logical order.
    pub start: usize,
    pub end: usize,
    /// The word's width fold in px, pre-`12700` scaling.
    pub width: f32,
    /// A whitespace word: a trailing run of these re-levels to the base in
    /// a truncated line (UAX #9 L1).
    pub blank: bool,
}

/// One bidi span's word range plus its truncated-word price probe.
pub struct WrapSpan {
    /// Half-open word index range into [`WrapWords::words`], logical order.
    pub first: usize,
    pub last: usize,
    /// The span's resolved embedding level: a trailing blank run inside a
    /// non-base span re-levels in a truncated line and folds separately.
    pub level: u8,
    /// `(item_text, word_start, cut, line_rtl) -> px`: the exact width of
    /// `item_text[word_start..cut]` as the fresh truncated line would shape it
    /// (same span direction, same glyph fold order).
    pub fragment: WrapFragment,
}

/// Word-level width data for one wrap item.
pub struct WrapWords {
    /// All words, logical order; spans in logical order.
    pub words: Vec<WrapWord>,
    pub spans: Vec<WrapSpan>,
    /// The reproduced layout fold in px, bit-equal to the measured width
    /// before the `12700` scaling.
    pub total: f32,
    /// The item's base paragraph direction.
    pub line_rtl: bool,
    /// `rPr@spc` tracking in EMU per inter-code-point boundary (`0` = normal).
    pub spacing_emu: f32,
}

impl WrapWords {
    /// Adopt the shaped word table, with the item's `rPr@spc` tracking.
    pub fn from_shaped(shaped: ShapedWords, spacing_emu: f32) -> Self {
        let ShapedWords {
            words,
            spans,
            total,
            line_rtl,
        } = shaped;
        Self {
            words: words
                .into_iter()
                .map(|word| WrapWord {
                    start: word.start,
                    end: word.end,
                    width: word.width,
                    blank: word.blank,
                })
                .collect(),
            spans: spans
                .into_iter()
                .map(|span| WrapSpan {
                    first: span.first,
                    last: span.last,
                    level: span.level,
                    fragment: span.fragment,
                })
                .collect(),
            total,
            line_rtl,
            spacing_emu,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabAlign {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Copy, Debug)]
pub struct TabStop {
    pub pos: f32,
    pub align: TabAlign,
}
