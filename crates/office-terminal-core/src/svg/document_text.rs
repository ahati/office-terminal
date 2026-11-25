//! SVG `VectorGlyphs` outlining for externally shaped runs (PRD 15 §5.5).
//!
//! Runs carrying a [`DocumentFaceId`](crate::text::document_faces::DocumentFaceId)
//! hold glyph IDs authored by a document engine against its exact face bytes.
//! The family-based `VectorGlyphs` path maps *characters* through `cmap`,
//! which can select a different glyph than the engine shaped; this module
//! outlines by glyph ID from the registered face, so SVG export stays
//! byte-faithful to the terminal raster.

use std::fmt::Write;

use crate::contracts::PositionedGlyphRun;
use crate::text::document_faces::with_document_face;
use rustybuzz::ttf_parser::{Face, GlyphId};

/// Emit one `<path>` per run outlined from the run's registered document face.
///
/// Returns `false` when the handle cannot be resolved or produces no ink, so
/// the caller falls back to the family-based serializer.
pub(super) fn serialize_document_glyphs(run: &PositionedGlyphRun, transform_attr: &str, out: &mut String) -> bool {
    let Some(face_id) = run.face else {
        return false;
    };
    with_document_face(face_id, |document_face| {
        let path_d = outline_run(run, &document_face.data[..], document_face.face_index);
        if path_d.is_empty() {
            return false;
        }

        let [r, g, b, a] = run.color;
        let opacity = a as f32 / 255.0;
        let _ = writeln!(
            out,
            r#"  <path d="{}" fill="rgb({},{},{})" fill-opacity="{:.2}"{}/>"#,
            path_d, r, g, b, opacity, transform_attr
        );
        super::text::decoration_rects(run, transform_attr, out);
        true
    })
    .unwrap_or(false)
}

/// Outline every glyph by ID into one SVG path data string.
fn outline_run(run: &PositionedGlyphRun, data: &[u8], face_index: u32) -> String {
    let Ok(face) = Face::parse(data, face_index) else {
        return String::new();
    };
    let units_per_em = face.units_per_em() as f32;
    if units_per_em <= 0.0 {
        return String::new();
    }
    let font_size_emu = run.font_size_pt * crate::contracts::EMU_PER_PT;
    let scale = font_size_emu / units_per_em;

    let mut builder = DocumentGlyphBuilder {
        d: String::new(),
        scale,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    for glyph in &run.glyphs {
        builder.offset_x = glyph.x;
        builder.offset_y = glyph.y;
        // Authoritative glyph IDs: no character mapping, no re-shaping.
        let _ = face.outline_glyph(GlyphId(glyph.glyph_id), &mut builder);
    }
    builder.d.trim().to_string()
}

/// Font-unit → slide-EMU outline builder (y inverted for font space).
struct DocumentGlyphBuilder {
    d: String,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl rustybuzz::ttf_parser::OutlineBuilder for DocumentGlyphBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(&mut self.d, "M {:.1} {:.1} ", px, py);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(&mut self.d, "L {:.1} {:.1} ", px, py);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p1x = self.offset_x + x1 * self.scale;
        let p1y = self.offset_y - y1 * self.scale;
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(&mut self.d, "Q {:.1} {:.1}, {:.1} {:.1} ", p1x, p1y, px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p1x = self.offset_x + x1 * self.scale;
        let p1y = self.offset_y - y1 * self.scale;
        let p2x = self.offset_x + x2 * self.scale;
        let p2y = self.offset_y - y2 * self.scale;
        let px = self.offset_x + x * self.scale;
        let py = self.offset_y - y * self.scale;
        let _ = write!(
            &mut self.d,
            "C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1} ",
            p1x, p1y, p2x, p2y, px, py
        );
    }

    fn close(&mut self) {
        self.d.push_str("Z ");
    }
}
