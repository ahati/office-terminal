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

use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, Style, Weight};

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
