//! Shaping seam for the text layout engine.
//!
//! The reference measures and paints through the browser's resolved face
//! (`renderer.ts:2083-2092`, `buildFont`). We use `cosmic-text` for the same two
//! jobs: measure a run inside the layout pass and rasterise the glyphs at paint
//! time. Keeping both on one `FontSystem` guarantees the advances used for line
//! breaking are the advances that get painted.
//!
//! Reference: `/tmp/oooxml/packages/pptx/src/renderer.ts` (`measureText`,
//! `buildFont`), `packages/pptx/src/powerpoint-line-metrics.ts` (ascent share).

use std::cell::RefCell;
use std::collections::HashMap;

use cosmic_text::{Attrs, AttrsList, Buffer, Family, Metrics, ShapeWord, Shaping, Style, Weight};

use crate::text::measure::WrapFragment;

thread_local! {
    /// `(family, bold, italic) -> ascent share`; the face probe itself runs
    /// against the shared runtime in [`super::fonts`] (one font scan per
    /// thread, not per consumer).
    static ASCENT_CACHE: RefCell<HashMap<(String, bool, bool), f32>> = RefCell::new(HashMap::new());
}

/// One shaped run in element-local EMU.
#[derive(Clone, Debug)]
pub struct ShapedText {
    /// `(glyph id, advance start x in EMU, advance in EMU)` in logical order.
    pub glyphs: Vec<(u16, f32, f32)>,
    /// `fontdb` id of each entry in `glyphs` (parallel array): the compile-time
    /// faces, so a run whose ids all came from one face can be painted
    /// without re-shaping (see `PositionedGlyphRun::single_face`).
    pub font_ids: Vec<fontdb::ID>,
    /// Sum of advances in EMU (reference `measureText().width`).
    pub width: f32,
    /// `usWinAscent / (usWinAscent + usWinDescent)` proxy for the line box split.
    pub ascent_share: f32,
}

impl ShapedText {
    pub fn empty() -> Self {
        Self {
            glyphs: Vec::new(),
            font_ids: Vec::new(),
            width: 0.0,
            ascent_share: 0.8,
        }
    }
}

/// Shape `text` at `size_emu` (EMU per em), returning per-glyph advances in EMU.
///
/// The reference's canvas font is `sizePt * PT_TO_EMU * scale` px; here the caller
/// passes the same number already expressed in EMU, so layout stays resolution
/// independent. `ascent_share` is derived from the face's catalogue share with
/// the same 1.2-em box the reference uses (see `ascent_share_for`).
pub fn shape(text: &str, family: &str, size_emu: f32, bold: bool, italic: bool) -> ShapedText {
    if text.is_empty() {
        return ShapedText::empty();
    }
    super::fonts::with_font_system(|font_system| {
        let size_px = (size_emu / 12700.0).max(0.5);
        let mut buffer = Buffer::new(font_system, Metrics::new(size_px, size_px * 1.2));
        buffer.set_size(font_system, None, None);

        let mut attrs = Attrs::new().family(Family::Name(family));
        if bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        if italic {
            attrs = attrs.style(Style::Italic);
        }
        buffer.set_text(font_system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        let mut glyphs = Vec::new();
        let mut font_ids = Vec::new();
        let mut width = 0.0f32;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            for glyph in run.glyphs {
                glyphs.push((
                    glyph.glyph_id,
                    glyph.x * 12700.0,
                    glyph.w * 12700.0,
                ));
                font_ids.push(glyph.font_id);
            }
        }

        ShapedText {
            glyphs,
            font_ids,
            width: width * 12700.0,
            ascent_share: ascent_share_for(family, bold, italic),
        }
    })
}

/// Width only — used by the word-wrap measurer.
pub fn measure(text: &str, family: &str, size_emu: f32, bold: bool, italic: bool) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    shape(text, family, size_emu, bold, italic).width
}

// ── Word-level width oracle for the wrap phase ─────────────────────────────
//
// `cosmic-text` never shapes a whole line at once: `ShapeLine` splits the text
// into bidi spans, each span into UAX #14 words, and each word is shaped on its
// own through `Shaping::Advanced` (shape.rs:537-743 in cosmic-text 0.12.1). The
// laid-out width is the fold `Σ_span (Σ_word word.width(font_size))`
// (add_to_visual_line, shape.rs:1052-1079) with the *logical* word order inside
// every span (the wrap loop walks stored words forward for congruent spans and
// reversed for incongruent ones, shape.rs:1086-1092 — and incongruent spans were
// stored reversed, shape.rs:738-741, so both are logical). Kerning and
// ligatures therefore never cross a word boundary, and a truncated prefix only
// re-shapes the one word it cuts through.
//
// [`shape_words`] exposes exactly that structure so the wrap phase can price
// every atom-prefix cut without a fresh line shape: words fully inside the
// prefix fold bit-identically (the fresh line shapes the same strings as its own
// words), and the cut word is priced by [`ShapedSpan::fragment`] — a `ShapeWord`
// shaped with the span's own direction and the line's glyph fold order, which is
// precisely the shape the fresh truncated line produces for that word.

/// One UAX #14 word of a shaped item: its byte range and its width fold in px.
pub struct ShapedWord {
    /// Byte range in the shaped text (logical; ligature swallows included).
    pub start: usize,
    pub end: usize,
    /// `word.width(font_size)` in px — the same fold the layout pass sums.
    pub width: f32,
    /// A whitespace word (`is_blank`): a trailing run of these re-levels to
    /// the paragraph base in a truncated line (UAX #9 L1).
    pub blank: bool,
}

/// One bidi span's word range, in logical order.
pub struct ShapedSpan {
    /// Half-open word index range into [`ShapedWords::words`].
    pub first: usize,
    pub last: usize,
    /// The span's resolved embedding level (UAX #9): a trailing blank run
    /// inside a span whose level differs from the paragraph base re-levels
    /// in a truncated line, so the fold must split it out (UAX #9 L1).
    pub level: u8,
    /// `(text, word_start, cut, line_rtl) -> px`: the exact in-situ width of
    /// `text[word_start..cut]` — a `ShapeWord` shaped with this span's
    /// direction, its glyphs reversed iff the line is RTL, folded like the
    /// fresh truncated line folds its truncated last word.
    pub fragment: WrapFragment,
}

/// Word-level reconstruction of one item's width fold (see the module notes).
pub struct ShapedWords {
    /// All words of the item, in logical order, spans in logical order.
    pub words: Vec<ShapedWord>,
    pub spans: Vec<ShapedSpan>,
    /// The reproduced layout fold in px: bit-equal to the buffer's `line_w`.
    pub total: f32,
    /// Base paragraph direction of the shaped text.
    pub line_rtl: bool,
}

/// Extract the word structure of `text` through the exact `shape()` pipeline.
///
/// Returns `None` (and the wrap phase falls back to probe-by-`advance`) when the
/// reconstruction cannot be proven: the buffer split into several layout runs,
/// a word shaped to no glyphs, or the reproduced fold differing from the
/// buffer's own `line_w` by a single bit — which also keeps a future
/// cosmic-text version change from silently altering wrap decisions.
pub fn shape_words(
    text: &str,
    family: &str,
    size_emu: f32,
    bold: bool,
    italic: bool,
) -> Option<ShapedWords> {
    if text.is_empty() {
        return None;
    }
    super::fonts::with_font_system(|font_system| {
        // Identical to `shape()`: same metrics, attrs and shaping path, so the
        // extracted numbers are the numbers `measure()` reports.
        let size_px = (size_emu / 12700.0).max(0.5);
        let mut buffer = Buffer::new(font_system, Metrics::new(size_px, size_px * 1.2));
        buffer.set_size(font_system, None, None);

        let mut attrs = Attrs::new().family(Family::Name(family));
        if bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        if italic {
            attrs = attrs.style(Style::Italic);
        }
        buffer.set_text(font_system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        let line = buffer.lines.first()?.shape_opt().as_ref()?;
        let line_rtl = line.rtl;
        let mut words: Vec<ShapedWord> = Vec::new();
        let mut spans: Vec<ShapedSpan> = Vec::new();
        // Words tile the text byte for byte (cosmic-text splits spans into
        // words without gaps); the running cover proves it span by span.
        let mut covered = 0usize;
        for span in &line.spans {
            let first = words.len();
            // The wrap loop folds words in LOGICAL order: undo the visual
            // reversal an incongruent span stored (cosmic-text shape.rs:738-741).
            let logical = if line_rtl != span.level.is_rtl() {
                span.words.iter().rev().collect::<Vec<_>>()
            } else {
                span.words.iter().collect::<Vec<_>>()
            };
            for word in logical {
                // A word with no glyph cannot be ranged; the cut mapping would
                // be guesswork, so the caller falls back to probing.
                let (lo, hi) = word_byte_range(word)?;
                if lo != covered {
                    return None; // holes in the byte cover would misprice cuts
                }
                covered = hi;
                words.push(ShapedWord {
                    start: lo,
                    end: hi,
                    width: word.width(size_px),
                    blank: word.blank,
                });
            }
            if words.len() == first {
                return None; // a span without words cannot be ranged
            }
            // The truncated-word probe captures the span's own direction token
            // by value — `unicode_bidi::Level` stays unnamed, so this closure is
            // the seam that carries cosmic-text's direction without this crate
            // taking `unicode-bidi` as a direct dependency.
            let level = span.level;
            let owned_family = family.to_string();
            let fragment = Box::new(
                move |text: &str, start: usize, cut: usize, line_rtl: bool| {
                    super::fonts::with_font_system(|font_system| {
                        let size_px = (size_emu / 12700.0).max(0.5);
                        let mut attrs =
                            Attrs::new().family(Family::Name(&owned_family));
                        if bold {
                            attrs = attrs.weight(Weight::BOLD);
                        }
                        if italic {
                            attrs = attrs.style(Style::Italic);
                        }
                        let attrs_list = AttrsList::new(attrs);
                        let mut word = ShapeWord::new(
                            font_system,
                            text,
                            &attrs_list,
                            start..cut,
                            level,
                            false,
                            Shaping::Advanced,
                        );
                        // The line folds RTL-line words over reversed glyphs
                        // (cosmic-text shape.rs:733-736); the probe folds the
                        // same order to stay bit-identical.
                        if line_rtl {
                            word.glyphs.reverse();
                        }
                        word.width(size_px)
                    })
                },
            );
            spans.push(ShapedSpan {
                first,
                last: words.len(),
                level: span.level.number(),
                fragment,
            });
        }
        if covered != text.len() {
            return None;
        }
        // The fold itself, association for association: per span from 0.0,
        // spans folded into the running line width (add_to_visual_line).
        let mut total = 0.0f32;
        for span in &spans {
            let mut span_width = 0.0f32;
            for word in &words[span.first..span.last] {
                span_width += word.width;
            }
            total += span_width;
        }
        // Prove the reconstruction against the buffer's own layout width.
        let mut runs = buffer.layout_runs();
        let line_w = runs.next().map(|run| run.line_w);
        let single = runs.next().is_none();
        (single && line_w == Some(total)).then(|| ShapedWords {
            words,
            spans,
            total,
            line_rtl,
        })
    })
}

/// `[min glyph start, max glyph end)` of one shaped word.
fn word_byte_range(word: &ShapeWord) -> Option<(usize, usize)> {
    let mut lo = usize::MAX;
    let mut hi = 0usize;
    for glyph in &word.glyphs {
        lo = lo.min(glyph.start);
        hi = hi.max(glyph.end);
    }
    (!word.glyphs.is_empty()).then_some((lo, hi))
}

/// `usWinAscent / (usWinAscent + usWinDescent)` proxy from the resolved face.
///
/// `ttf-parser` does not expose the OS/2 `usWin*` pair, so we use `hhea`
/// ascender/descender plus half the line gap, which is the same quantity
/// PowerPoint's #1610 model derives (ref `powerpoint-line-metrics.ts:115-134`).
/// Faces that cannot be resolved fall back to the synthetic 0.8/0.2 box
/// (`packages/core/src/text/synthetic-em-box.ts:15`).
///
/// The ascent share of a run's drawn face, in PowerPoint's #1610 model.
///
/// PowerPoint lays a text body out with the **static** catalogue of the faces
/// Office ships (plus the macOS Supplemental copies it embeds), never with the
/// host's `hhea` table: `powerpoint-line-metrics.test.ts` pins Arial at
/// `usWinAscent / (usWinAscent + usWinDescent) = 1854 / 2288`, which no
/// installed-face probe reproduces. The catalogue lives in
/// [`super::faces::power_point_ascent_share`].
///
/// The family reaching this function is the *painted* one, so a
/// metric-compatible substitute is mapped back to the authored Office face
/// ([`catalogue_family`]): layout metrics belong to the PowerPoint face, only
/// the glyph outlines come from the substitute (`renderer.ts:1928-1938` reads
/// `seg.lineMetric`, not the canvas route).
///
/// An uncatalogued face (`Avenir`, `Menlo`, `Hiragino Sans`, a generic family)
/// returns `None` there, and the reference then keeps the ordinary line model
/// for the whole body (`metricOk = false`, `renderer.ts:225`). This function
/// cannot express "unresolved": callers that need the distinction read
/// [`super::faces::power_point_face_metrics`] directly. The probe below is the
/// crate's existing host approximation and is kept as that fallback.
///
/// A math face is deliberately left to the probe: any `seg.math` sets
/// `metricOk = false` in the reference (`renderer.ts:5289`), so the whole body
/// keeps the ordinary line model and a display equation's line box never
/// shrinks to Cambria Math's narrow usWin split. Until that gate is ported the
/// synthetic 0.8 split reproduces the reference's math-line baseline.
pub fn ascent_share_for(family: &str, bold: bool, italic: bool) -> f32 {
    let key = (family.to_string(), bold, italic);
    if let Some(share) = ASCENT_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return share;
    }
    let catalogue_share = if is_math_face(family) {
        None
    } else {
        crate::text::faces::power_point_ascent_share(catalogue_family(family), bold, italic)
    };
    let share = catalogue_share.unwrap_or_else(|| compute_ascent_share(family, bold, italic));
    ASCENT_CACHE.with(|c| {
        c.borrow_mut().insert(key, share);
    });
    share
}

/// A math face keeps the ordinary line model (see [`ascent_share_for`]).
fn is_math_face(family: &str) -> bool {
    family.to_lowercase().contains("math")
}

/// Map a metric-compatible substitute back to the Office face it replaces.
///
/// `compiler::text::resolve_font_family` bundles Liberation / Carlito / Caladea
/// for the glyph outlines; PowerPoint's line box, however, belongs to the
/// authored face (`Calibri` = 1950 / 2500 usWin, not Carlito's host `hhea`).
/// Keeping the two apart is what lets the bundled substitutes paint while the
/// catalogue keeps the baselines.
fn catalogue_family(family: &str) -> &str {
    let lower = family.to_lowercase();
    if lower.starts_with("carlito") {
        "Calibri"
    } else if lower.starts_with("caladea") {
        "Cambria"
    } else if lower.starts_with("liberation sans") {
        "Arial"
    } else if lower.starts_with("liberation serif") {
        "Times New Roman"
    } else if lower.starts_with("liberation mono") {
        "Courier New"
    } else {
        family
    }
}

fn compute_ascent_share(family: &str, bold: bool, italic: bool) -> f32 {
    super::fonts::with_face_db(|db| face_ascent_share(db, family, bold, italic))
}

fn face_ascent_share(db: &fontdb::Database, family: &str, bold: bool, italic: bool) -> f32 {
    let weight = if bold {
        fontdb::Weight::BOLD
    } else {
        fontdb::Weight::NORMAL
    };
    let style = if italic {
        fontdb::Style::Italic
    } else {
        fontdb::Style::Normal
    };
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight,
        stretch: fontdb::Stretch::Normal,
        style,
    };
    let Some(id) = db.query(&query) else {
        return 0.8;
    };
    let mut share = 0.8f32;
    let _ = db.with_face_data(id, |data, index| {
        if let Ok(face) = rustybuzz::ttf_parser::Face::parse(data, index) {
            let asc = face.ascender() as f32;
            let desc = face.descender().abs() as f32;
            let gap = face.line_gap().max(0) as f32;
            let total = asc + desc + gap;
            if total > 0.0 {
                share = ((asc + gap * 0.5) / total).clamp(0.5, 0.98);
            }
        }
    });
    share
}

/// Run a closure over the raw face data of `family` at the given weight/style.
pub(crate) fn with_face_data<R>(
    family: &str,
    bold: bool,
    italic: bool,
    f: impl FnOnce(&[u8], u32) -> R,
) -> Option<R> {
    super::fonts::with_face_db(|db| {
        let weight = if bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        };
        let style = if italic {
            fontdb::Style::Italic
        } else {
            fontdb::Style::Normal
        };
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight,
            stretch: fontdb::Stretch::Normal,
            style,
        };
        let id = db.query(&query)?;
        db.with_face_data(id, f)
    })
}
