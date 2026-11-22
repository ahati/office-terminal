//! Process-global registry of externally supplied font faces (PRD 15 §7.8).
//!
//! Document engines (the DOCX layout engine, a future PDF vector bridge,
//! metafile-embedded fonts) shape text with their own font stacks and emit
//! **glyph IDs** valid only for the exact face they used. Resolving those ids
//! again through the shared family-name runtime (`crate::text::fonts`) can
//! pick a different face — embedded fonts, TTC face indices, substituted
//! families — and would paint garbage.
//!
//! The registry holds the authoritative raw face bytes (plus the collection
//! face index) for the lifetime of the loaded document and hands out compact
//! `DocumentFaceId` handles that ride along in
//! [`crate::contracts::PositionedGlyphRun::face`]. Backends paint glyph IDs
//! from these bytes directly.
//!
//! v1 policy: append-only and process-global, immutable after document
//! open, so worker threads need no setup and the thread-local shaping
//! runtime is untouched. `clear_document_faces` exists for tests that open
//! many documents in one process.

use std::sync::{Arc, OnceLock, RwLock};

/// Handle to one registered document face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentFaceId(u32);

/// One externally supplied font face.
#[derive(Clone, Debug)]
pub struct DocumentFace {
    /// Raw font file bytes (TTF/OTF/TTC), kept for the document's lifetime.
    pub data: Arc<[u8]>,
    /// Face index inside a font collection (`0` for plain TTF/OTF).
    pub face_index: u32,
    /// Family name for family-based fallbacks (SVG selectable text).
    pub family: String,
    /// Whether the face is a bold variant.
    pub bold: bool,
    /// Whether the face is an italic variant.
    pub italic: bool,
    /// Selected non-default OpenType variation-axis coordinates.
    /// v1 carries them for diagnostics without applying them.
    pub variations: Vec<([u8; 4], f32)>,
}

impl DocumentFace {
    /// Whether any non-default variation axis is selected.
    pub fn has_variations(&self) -> bool {
        !self.variations.is_empty()
    }
}

fn registry() -> &'static RwLock<Vec<Arc<DocumentFace>>> {
    static REGISTRY: OnceLock<RwLock<Vec<Arc<DocumentFace>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

/// Append `faces` to the registry and return their handles in order.
///
/// Handles are dense from zero per process; registering the same face bytes
/// twice yields distinct handles (document opens are independent).
pub fn register_document_faces(faces: Vec<DocumentFace>) -> Vec<DocumentFaceId> {
    let mut guard = registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let base = guard.len() as u32;
    let count = faces.len() as u32;
    for face in faces {
        guard.push(Arc::new(face));
    }
    (0..count).map(|offset| DocumentFaceId(base + offset)).collect()
}

/// Run `f` with the face registered under `id`.
///
/// Returns `None` when the handle is out of range (a cleared registry or a
/// foreign handle), letting callers fall back to family-based rendering.
pub fn with_document_face<R>(id: DocumentFaceId, f: impl FnOnce(&DocumentFace) -> R) -> Option<R> {
    let guard = registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(id.0 as usize).map(|face| f(face))
}

/// Drop every registered face. Only for tests and engine reloads: live runs
/// referencing cleared handles fall back to the family path.
pub fn clear_document_faces() {
    let mut guard = registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(family: &str) -> DocumentFace {
        DocumentFace {
            data: Arc::from(b"fake-face-bytes".as_slice()),
            face_index: 0,
            family: family.to_string(),
            bold: false,
            italic: false,
            variations: Vec::new(),
        }
    }

    #[test]
    fn registered_faces_resolve_by_handle() {
        clear_document_faces();
        let ids = register_document_faces(vec![face("Carlito"), face("Caladea")]);
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);

        let first = with_document_face(ids[0], |f| f.family.clone());
        let second = with_document_face(ids[1], |f| f.family.clone());
        assert_eq!(first.as_deref(), Some("Carlito"));
        assert_eq!(second.as_deref(), Some("Caladea"));
        clear_document_faces();
    }

    #[test]
    fn clearing_invalidates_handles_and_restarts_at_zero() {
        clear_document_faces();
        let ids = register_document_faces(vec![face("Temp")]);
        clear_document_faces();
        assert!(with_document_face(ids[0], |_| ()).is_none());
        let fresh = register_document_faces(vec![face("New")]);
        assert_eq!(fresh[0], DocumentFaceId(0));
        clear_document_faces();
    }

    #[test]
    fn unknown_handle_is_none_not_a_panic() {
        assert!(with_document_face(DocumentFaceId(u32::MAX), |_| ()).is_none());
    }
}
