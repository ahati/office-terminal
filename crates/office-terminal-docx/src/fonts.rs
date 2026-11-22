//! `oxml_layout::FontData` → core `DocumentFace` registration (PRD 15 §7.8).
//!
//! The layout engine shapes with its own font stack and emits glyph IDs that
//! are only valid for the exact faces it used. Registering those faces once
//! per document (and warning about the v1 approximation cases) lets every
//! backend paint the ids from the authoritative bytes.

#[cfg(any(feature = "rdocx-backend", test))]
use office_terminal_core::text::document_faces::{register_document_faces, DocumentFace};
#[cfg(any(feature = "rdocx-backend", test))]
use oxml_layout::FontData;

#[cfg(any(feature = "rdocx-backend", test))]
use crate::engine::ResolvedFont;

/// Register every layout font in the core document-face registry and return
/// the per-font identity slots indexed by `FontId.0`.
///
/// v1 approximations, each recorded once per document (issue #40 §7.8):
/// - non-default variation axes are carried but not applied;
/// - synthetic bold/italic falls back to the family path (no face handle).
#[cfg(any(feature = "rdocx-backend", test))]
pub(crate) fn register_engine_fonts(
    fonts: &[FontData],
    diagnostics: &mut Vec<String>,
) -> Vec<Option<ResolvedFont>> {
    let faces: Vec<DocumentFace> = fonts
        .iter()
        .map(|font| DocumentFace {
            data: font.data.clone(),
            face_index: font.face_index,
            family: font.family.clone(),
            bold: font.bold,
            italic: font.italic,
            variations: font.variations.clone(),
        })
        .collect();
    let handles = register_document_faces(faces);

    let max_id = fonts.iter().map(|font| font.id.0).max().unwrap_or(0);
    let mut slots: Vec<Option<ResolvedFont>> = vec![None; max_id as usize + 1];

    let mut warned_variations = false;
    let mut warned_synthetic = false;
    for (font, handle) in fonts.iter().zip(handles) {
        if !warned_variations && !font.variations.is_empty() {
            diagnostics.push(format!(
                "variable font axes on '{}' are carried but not applied",
                font.family
            ));
            warned_variations = true;
        }
        let face = if font.synthetic_bold || font.synthetic_italic {
            if !warned_synthetic {
                diagnostics.push(format!(
                    "synthetic bold/italic on '{}' renders through the family fallback",
                    font.family
                ));
                warned_synthetic = true;
            }
            None
        } else {
            Some(handle)
        };
        slots[font.id.0 as usize] = Some(ResolvedFont {
            family: font.family.clone(),
            face,
        });
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_terminal_core::text::document_faces::clear_document_faces;
    use std::sync::Arc;

    fn font(id: u32, family: &str) -> FontData {
        FontData {
            id: oxml_layout::FontId(id),
            family: family.to_string(),
            data: Arc::from(b"font-bytes".as_slice()),
            face_index: 0,
            bold: false,
            italic: false,
            variations: Vec::new(),
            synthetic_bold: false,
            synthetic_italic: false,
        }
    }

    #[test]
    fn fonts_resolve_by_id_with_exact_face_handles() {
        clear_document_faces();
        let fonts = vec![font(0, "Carlito"), font(2, "Caladea")];
        let mut diagnostics = Vec::new();
        let slots = register_engine_fonts(&fonts, &mut diagnostics);

        assert!(diagnostics.is_empty());
        assert_eq!(slots.len(), 3);
        assert!(slots[0].as_ref().is_some_and(|f| f.face.is_some()));
        assert!(slots[1].is_none());
        let caladea = slots[2].as_ref().expect("dense slot");
        assert_eq!(caladea.family, "Caladea");
        assert!(caladea.face.is_some());
        clear_document_faces();
    }

    #[test]
    fn synthetic_styles_fall_back_to_the_family_path_with_one_warning() {
        clear_document_faces();
        let mut synthetic = font(0, "Carlito");
        synthetic.synthetic_bold = true;
        let mut variable = font(1, "Aptos");
        variable.variations = vec![(*b"wght", 400.0)];

        let mut diagnostics = Vec::new();
        let slots = register_engine_fonts(&[synthetic, variable], &mut diagnostics);

        assert!(slots[0].as_ref().is_some_and(|f| f.face.is_none()));
        assert!(slots[1].as_ref().is_some_and(|f| f.face.is_some()));
        assert_eq!(diagnostics.len(), 2, "one warning per approximation class");
        clear_document_faces();
    }
}
