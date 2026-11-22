//! Exact-face painting for externally shaped runs (PRD 15 §7.8).
//!
//! A run carrying a [`DocumentFaceId`](crate::text::document_faces::DocumentFaceId)
//! was shaped by a document engine against its own font stack: the glyph IDs
//! are only valid for the exact face bytes registered under that handle, so
//! they must never be re-resolved through the family-name runtime (a
//! substituted or embedded face would render garbage). Instead the face
//! bytes are parsed here and every glyph is outlined by ID and mapped
//! through the element transform — the same point pipeline the affine
//! painter uses, so upright and rotated runs share one code path.

use super::affine::paint_affine_bar;
use crate::contracts::{AffineTransform, PositionedGlyphRun};
use crate::text::document_faces::with_document_face;
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Paint `run` from its registered document face. Returns `false` when the
/// handle cannot be resolved or the bytes are not a parseable font, letting
/// the caller fall back to family-based rendering instead of dropping ink.
pub(super) fn render_document_run(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let Some(face_id) = run.face else {
        return false;
    };
    with_document_face(face_id, |document_face| {
        render_with_face(
            pixmap,
            run,
            &document_face.data[..],
            document_face.face_index,
            transform,
            scale,
            offset_x,
            offset_y,
        )
    })
    .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn render_with_face(
    pixmap: &mut tiny_skia::Pixmap,
    run: &PositionedGlyphRun,
    data: &[u8],
    face_index: u32,
    transform: &AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let Ok(face) = Face::parse(data, face_index) else {
        return false;
    };
    let units_per_em = face.units_per_em() as f32;
    if units_per_em <= 0.0 {
        return false;
    }
    let emu_per_unit = run.font_size_pt * 12_700.0 / units_per_em;
    let [red, green, blue, alpha] = run.color;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(red, green, blue, alpha);
    paint.anti_alias = true;

    let mut any = false;
    for glyph in &run.glyphs {
        let mut builder = DocumentOutlineBuilder {
            path: tiny_skia::PathBuilder::new(),
            origin_x: glyph.x,
            origin_y: glyph.y,
            emu_per_unit,
            transform: *transform,
            scale,
            offset_x,
            offset_y,
        };
        // Glyph IDs from the document engine are authoritative: no character
        // mapping, no re-shaping. A `.notdef` (id 0) paints its own box, which
        // is the honest visible degradation for a face the engine picked.
        if face.outline_glyph(GlyphId(glyph.glyph_id), &mut builder).is_none() {
            continue;
        }
        let Some(path) = builder.path.finish() else {
            continue;
        };
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
        any = true;
    }

    // DOCX runs carry no IR decorations (underlines/strikes arrive as
    // positioned line elements), but the contract allows both; bands map
    // through the same transform as the glyph outlines.
    for line in [run.decorations.underline, run.decorations.strike]
        .into_iter()
        .flatten()
    {
        for offset in super::decorations::band_offsets(&line).into_iter().flatten() {
            any |= paint_affine_bar(pixmap, run, transform, scale, offset_x, offset_y, &line, offset);
        }
    }
    any
}

/// Maps font-space outline points to device pixels:
/// font units → run-local EMU (baseline origin, y down) → slide → pixels.
struct DocumentOutlineBuilder {
    path: tiny_skia::PathBuilder,
    origin_x: f32,
    origin_y: f32,
    emu_per_unit: f32,
    transform: AffineTransform,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl DocumentOutlineBuilder {
    fn point(&mut self, x: f32, y: f32) -> (f32, f32) {
        let local_x = self.origin_x + x * self.emu_per_unit;
        let local_y = self.origin_y - y * self.emu_per_unit;
        let (slide_x, slide_y) = self.transform.transform_point(local_x, local_y);
        (
            self.offset_x + slide_x * self.scale,
            self.offset_y + slide_y * self.scale,
        )
    }
}

impl OutlineBuilder for DocumentOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        self.path.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AffineTransform, PositionedGlyph};
    use crate::text::document_faces::{clear_document_faces, register_document_faces, DocumentFace};

    fn carlito_face() -> crate::text::document_faces::DocumentFaceId {
        let bytes = crate::text::fonts::BUNDLED_FONTS[0];
        let ids = register_document_faces(vec![DocumentFace {
            data: std::sync::Arc::from(bytes),
            face_index: 0,
            family: "Carlito".to_string(),
            bold: false,
            italic: false,
            variations: Vec::new(),
        }]);
        ids[0]
    }

    fn ink_pixels(pixmap: &tiny_skia::Pixmap) -> usize {
        pixmap
            .pixels()
            .iter()
            .filter(|p| p.alpha() > 0)
            .count()
    }

    fn sample_run(face_id: crate::text::document_faces::DocumentFaceId) -> PositionedGlyphRun {
        // Resolve two real glyph IDs ('A', 'B') from the registered face so
        // the run exercises the by-ID outlining path.
        let glyph_ids = crate::text::document_faces::with_document_face(face_id, |face| {
            let parsed = Face::parse(&face.data[..], face.face_index).expect("bundled font");
            [
                parsed.glyph_index('A').map(|g| g.0).unwrap_or(0),
                parsed.glyph_index('B').map(|g| g.0).unwrap_or(0),
            ]
        })
        .expect("registered");

        let emu = 12_700.0;
        PositionedGlyphRun {
            text: "AB".to_string(),
            font_family: "Carlito".to_string(),
            font_size_pt: 32.0,
            color: [0, 0, 0, 255],
            bold: false,
            italic: false,
            glyphs: vec![
                PositionedGlyph {
                    glyph_id: glyph_ids[0],
                    x: 20.0 * emu,
                    y: 40.0 * emu,
                    advance: 18.0 * emu,
                    warp_u: None,
                },
                PositionedGlyph {
                    glyph_id: glyph_ids[1],
                    x: 38.0 * emu,
                    y: 40.0 * emu,
                    advance: 18.0 * emu,
                    warp_u: None,
                },
            ],
            decorations: Default::default(),
            single_face: false,
            face: Some(face_id),
            warp: None,
            reflection: None,
        }
    }

    // One sequential test: the registry is process-global, so parallel test
    // functions clearing it would race each other.
    #[test]
    fn document_face_paints_then_stale_handle_degrades() {
        clear_document_faces();
        let face_id = carlito_face();
        let run = sample_run(face_id);

        let mut pixmap = tiny_skia::Pixmap::new(128, 96).expect("pixmap");
        let scale = 1.0 / 12_700.0;
        assert!(render_document_run(
            &mut pixmap,
            &run,
            &AffineTransform::identity(),
            scale,
            0.0,
            0.0,
        ));
        assert!(ink_pixels(&pixmap) > 50, "two 32 pt glyphs must paint ink");

        // A stale handle (registry cleared after a document closed) must
        // degrade to the family fallback, not panic or paint stray ink.
        clear_document_faces();
        let mut cleared = tiny_skia::Pixmap::new(128, 96).expect("pixmap");
        assert!(!render_document_run(
            &mut cleared,
            &run,
            &AffineTransform::identity(),
            scale,
            0.0,
            0.0,
        ));
        assert_eq!(ink_pixels(&cleared), 0);
        clear_document_faces();
    }
}
