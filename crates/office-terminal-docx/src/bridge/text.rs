//! Glyph run bridging with exact-face handles (PRD 15 §5.2, issue #40 §7.7.3).
//!
//! Positions accumulate in f64 point space and convert to EMU once at the
//! leaf. Monolingual runs advance along x; multilingual runs carry two-axis
//! advances plus per-glyph offsets. Underline/strike arrive as separate line
//! elements, so IR decorations stay empty. The run's glyph IDs are
//! authoritative: the registered `DocumentFaceId` rides along and both
//! backends paint from the exact face bytes.

use office_terminal_core::contracts::{
    AffineTransform, PositionedGlyph, PositionedGlyphRun, VectorCommand, VectorSlide,
};
use oxml_layout::{GlyphRun, MultilingualGlyphRun};

use super::paint::rgba_scaled;
use super::{pt_to_emu, BridgeContext};

/// Map one monolingual glyph run to a `DrawText` command.
pub(crate) fn map_glyph_run(
    run: &GlyphRun,
    acc: &AffineTransform,
    opacity: f32,
    slide: &mut VectorSlide,
    ctx: &BridgeContext<'_>,
) {
    let count = run.glyph_ids.len();
    if count == 0 || run.text.is_empty() || run.font_size <= 0.0 {
        return;
    }
    // Defensive length agreement: a hostile or buggy engine emitting
    // mismatched vectors degrades to the common prefix, never a panic.
    let advance_count = run.advances.len().min(count);
    if advance_count < count {
        ctx.warn_once("docx-run-length-mismatch", "text-run".to_string());
    }

    let mut pen_x = run.origin.x;
    let mut glyphs = Vec::with_capacity(count);
    for (index, glyph_id) in run.glyph_ids.iter().enumerate() {
        let advance = run.advances.get(index).copied().unwrap_or(0.0);
        glyphs.push(PositionedGlyph {
            glyph_id: *glyph_id,
            x: pt_to_emu(pen_x),
            y: pt_to_emu(run.origin.y),
            advance: pt_to_emu(advance),
            warp_u: None,
        });
        pen_x += advance;
    }

    let Some(font) = ctx.engine.resolve_font(run.font_id) else {
        // Without the engine's font identity the glyph IDs cannot be painted
        // from any face: an honest warning beats garbage glyphs.
        ctx.warn_once("docx-font-unresolved", "text-run".to_string());
        return;
    };

    push_text_command(
        slide,
        *acc,
        PositionedGlyphRun {
            text: run.text.clone(),
            font_family: font.family.clone(),
            font_size_pt: run.font_size as f32,
            color: rgba_scaled(&run.color, opacity),
            bold: run.bold,
            italic: run.italic,
            glyphs,
            decorations: Default::default(),
            single_face: false,
            face: font.face,
            warp: None,
            reflection: None,
        },
    );
}

/// Map one multilingual run: two-axis positioning, per-glyph offsets and
/// logical text for search/selectable SVG.
pub(crate) fn map_multilingual_run(
    run: &MultilingualGlyphRun,
    acc: &AffineTransform,
    opacity: f32,
    slide: &mut VectorSlide,
    ctx: &BridgeContext<'_>,
) {
    let count = run.glyph_ids.len();
    if count == 0 || run.logical_text.is_empty() || run.font_size <= 0.0 {
        return;
    }
    let axes_agree = run.x_advances.len() == count
        && run.y_advances.len() == count
        && run.x_offsets.len() == count
        && run.y_offsets.len() == count;
    if !axes_agree {
        ctx.warn_once("docx-run-length-mismatch", "multilingual-run".to_string());
        return;
    }

    let mut pen_x = run.origin.x;
    let mut pen_y = run.origin.y;
    let mut glyphs = Vec::with_capacity(count);
    for index in 0..count {
        let x = pen_x + run.x_offsets[index];
        let y = pen_y + run.y_offsets[index];
        glyphs.push(PositionedGlyph {
            glyph_id: run.glyph_ids[index],
            x: pt_to_emu(x),
            y: pt_to_emu(y),
            advance: pt_to_emu(run.x_advances[index]),
            warp_u: None,
        });
        pen_x += run.x_advances[index];
        pen_y += run.y_advances[index];
    }

    let Some(font) = ctx.engine.resolve_font(run.font_id) else {
        ctx.warn_once("docx-font-unresolved", "multilingual-run".to_string());
        return;
    };

    push_text_command(
        slide,
        *acc,
        PositionedGlyphRun {
            text: run.logical_text.clone(),
            font_family: font.family.clone(),
            font_size_pt: run.font_size as f32,
            color: rgba_scaled(&run.color, opacity),
            bold: run.bold,
            italic: run.italic,
            glyphs,
            decorations: Default::default(),
            single_face: false,
            face: font.face,
            warp: None,
            reflection: None,
        },
    );
}

fn push_text_command(slide: &mut VectorSlide, transform: AffineTransform, run: PositionedGlyphRun) {
    slide.commands.push(VectorCommand::DrawText {
        runs: vec![run],
        transform,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{compile, letter_page, text_run, FixtureEngine};
    use oxml_layout::FontId;

    #[test]
    fn glyph_positions_accumulate_advances_in_emu() {
        let page = letter_page(vec![oxml_layout::PositionedElement::Text(text_run(
            "ABC", 72.0, 90.0, 4, 12.0,
        ))]);
        let engine = FixtureEngine::new(Vec::new()).with_font(4, "Carlito");
        let (slide, warnings) = compile(&page, &engine);
        assert!(warnings.is_empty());

        let VectorCommand::DrawText { runs, transform } = &slide.commands[0] else {
            panic!("expected a text command");
        };
        assert_eq!(runs.len(), 1);
        assert_eq!(*transform, AffineTransform::identity());
        let run = &runs[0];
        assert_eq!(run.font_family, "Carlito");
        assert_eq!(run.text, "ABC");
        assert!(!run.single_face);
        // advance = size * 0.5 = 6 pt in the fixture
        assert_eq!(run.glyphs[0].x, 72.0 * 12_700.0);
        assert_eq!(run.glyphs[1].x, 78.0 * 12_700.0);
        assert_eq!(run.glyphs[2].x, 84.0 * 12_700.0);
        assert_eq!(run.glyphs[0].y, 90.0 * 12_700.0);
        assert_eq!(run.glyphs[0].advance, 6.0 * 12_700.0);
    }

    #[test]
    fn unknown_font_ids_warn_instead_of_painting_wrong_glyphs() {
        let page = letter_page(vec![oxml_layout::PositionedElement::Text(text_run(
            "AB", 0.0, 0.0, 9, 12.0,
        ))]);
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page, &engine);
        assert!(slide.commands.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn empty_runs_carry_no_command() {
        let mut run = text_run("", 0.0, 0.0, 0, 12.0);
        run.glyph_ids.clear();
        run.advances.clear();
        let page = letter_page(vec![oxml_layout::PositionedElement::Text(run)]);
        let engine = FixtureEngine::new(Vec::new()).with_font(0, "Carlito");
        let (slide, _warnings) = compile(&page, &engine);
        assert!(slide.commands.is_empty());
    }

    #[test]
    fn fixture_font_resolution_goes_through_the_engine() {
        let engine = FixtureEngine::new(Vec::new()).with_font(3, "Caladea");
        let resolved = {
            let ctx_font = crate::engine::DocxEngine::resolve_font(&engine, FontId(3));
            ctx_font.expect("registered font").family.clone()
        };
        assert_eq!(resolved, "Caladea");
        assert!(crate::engine::DocxEngine::resolve_font(&engine, FontId(2)).is_none());
    }
}
