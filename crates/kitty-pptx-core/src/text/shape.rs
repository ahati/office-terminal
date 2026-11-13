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

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight};

thread_local! {
    static FONT_SYSTEM: RefCell<FontSystem> = RefCell::new({
        let mut fs = FontSystem::new();
        super::fonts::install_into_font_system(&mut fs);
        fs
    });
    /// `(family, bold, italic) -> ascent share`, one font-db scan per process/thread.
    static ASCENT_CACHE: RefCell<HashMap<(String, bool, bool), f32>> = RefCell::new(HashMap::new());
    static FACE_DB: RefCell<Option<fontdb::Database>> = const { RefCell::new(None) };
}

/// One shaped run in element-local EMU.
#[derive(Clone, Debug)]
pub struct ShapedText {
    /// `(glyph id, advance start x in EMU, advance in EMU)` in logical order.
    pub glyphs: Vec<(u16, f32, f32)>,
    /// Sum of advances in EMU (reference `measureText().width`).
    pub width: f32,
    /// `usWinAscent / (usWinAscent + usWinDescent)` proxy for the line box split.
    pub ascent_share: f32,
}

impl ShapedText {
    pub fn empty() -> Self {
        Self {
            glyphs: Vec::new(),
            width: 0.0,
            ascent_share: 0.8,
        }
    }
}

/// Shape `text` at `size_emu` (EMU per em), returning per-glyph advances in EMU.
///
/// The reference's canvas font is `sizePt * PT_TO_EMU * scale` px; here the caller
/// passes the same number already expressed in EMU, so layout stays resolution
/// independent. `ascent_share` is derived from the face's `hhea` metrics with the
/// same 1.2-em box the reference uses (see `ascent_share_for`).
pub fn shape(text: &str, family: &str, size_emu: f32, bold: bool, italic: bool) -> ShapedText {
    if text.is_empty() {
        return ShapedText::empty();
    }
    FONT_SYSTEM.with(|cell| {
        let font_system = &mut *cell.borrow_mut();
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
        let mut width = 0.0f32;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            for glyph in run.glyphs {
                glyphs.push((
                    glyph.glyph_id,
                    glyph.x * 12700.0,
                    glyph.w * 12700.0,
                ));
            }
        }

        ShapedText {
            glyphs,
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
pub fn ascent_share_for(family: &str, bold: bool, italic: bool) -> f32 {
    let key = (family.to_string(), bold, italic);
    if let Some(share) = ASCENT_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return share;
    }
    let share = compute_ascent_share(family, bold, italic);
    ASCENT_CACHE.with(|c| {
        c.borrow_mut().insert(key, share);
    });
    share
}

fn compute_ascent_share(family: &str, bold: bool, italic: bool) -> f32 {
    FACE_DB.with(|cell| {
        let mut slot = cell.borrow_mut();
        let db = slot.get_or_insert_with(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            super::fonts::install_into_db(&mut db);
            db
        });
        face_ascent_share(db, family, bold, italic)
    })
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
