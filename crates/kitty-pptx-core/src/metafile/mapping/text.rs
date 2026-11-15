//! Text runs → shaped `PositionedGlyphRun`s.
//!
//! Skeleton scope: position, family, size, weight/style and simple decoration
//! lines. Escapement rotation and `dx` overrides land in M3 (PRD 13 §4.5).

use crate::contracts::{
    AffineTransform, DecorationLine, PositionedGlyph, PositionedGlyphRun, TextDecorations,
};

use crate::metafile::MetafileTextRun;

/// Shapes one engine text run into element-local EMU.
pub fn shape_text_run(
    run: &MetafileTextRun,
    unit_to_emu: &AffineTransform,
    emu_per_unit: f32,
) -> Option<PositionedGlyphRun> {
    if run.text.is_empty() {
        return None;
    }
    let size_emu = (run.size * emu_per_unit).max(1.0);
    let shaped = crate::text::shape::shape(&run.text, &run.family, size_emu, run.bold, run.italic);
    let (origin_x, origin_y) = unit_to_emu.transform_point(run.x, run.y);
    let glyphs = shaped
        .glyphs
        .iter()
        .map(|(glyph_id, glyph_x, advance)| PositionedGlyph {
            glyph_id: *glyph_id,
            x: origin_x + *glyph_x,
            y: origin_y,
            advance: *advance,
            warp_u: None,
        })
        .collect();

    let decorations = TextDecorations {
        underline: run.underline.then(|| DecorationLine {
            color: run.color,
            top_emu: size_emu * 0.12,
            thickness_emu: (size_emu * 0.06).max(1.0),
            gap_emu: size_emu * 0.04,
            style: Default::default(),
        }),
        strike: run.strike_out.then(|| DecorationLine {
            color: run.color,
            top_emu: -size_emu * 0.28,
            thickness_emu: (size_emu * 0.06).max(1.0),
            gap_emu: 0.0,
            style: Default::default(),
        }),
    };

    Some(PositionedGlyphRun {
        text: run.text.clone(),
        font_family: run.family.clone(),
        font_size_pt: size_emu / 12700.0,
        color: run.color,
        bold: run.bold,
        italic: run.italic,
        glyphs,
        decorations,
        warp: None,
    })
}
