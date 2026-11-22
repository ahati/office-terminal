//! Embedded-font part loading (ECMA-376 §19.2.1.9 `p:embeddedFontLst`).
//!
//! The reference reads each authored Font part and registers a face for the
//! slots that were actually extracted; a part that cannot be read drops its own
//! slot only (`embedded-fonts.test.ts:56-72`). A **typed** resource-limit
//! rejection is different: it is propagated by identity rather than treated as
//! a malformed font (`embedded-fonts.test.ts:160-165`), because the quota is a
//! package-level verdict that must reach the caller unchanged.

use pptx_model::{EmbeddedFontStyle, PptxEmbeddedFontRef};

use crate::error::AssetError;

/// One successfully read embedded-font part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedFont {
    pub font_name: String,
    pub style: EmbeddedFontStyle,
    pub part_path: String,
    pub content_type: String,
    /// Raw Font part bytes (a `.fntdata` payload is an obfuscated OpenType
    /// file; de-obfuscation is a host concern).
    pub bytes: Vec<u8>,
}

/// Read every authored embedded-font reference through `read`.
///
/// * `Ok(bytes)` — the slot is kept with its authored family and style.
/// * `Err(AssetError::ResourceLimit(violation))` — the whole load fails with
///   that same error, unwrapped (never re-labelled `malformed font`).
/// * any other `Err` — only that slot is dropped; siblings survive.
///
/// ref `embedded-fonts.test.ts:56-72` (sibling isolation) and `:160-165`
/// (typed limits preserved).
pub fn load_embedded_fonts(
    refs: &[PptxEmbeddedFontRef],
    read: &dyn Fn(&str) -> Result<Vec<u8>, AssetError>,
) -> Result<Vec<LoadedFont>, AssetError> {
    let mut loaded = Vec::with_capacity(refs.len());
    for reference in refs {
        match read(&reference.part_path) {
            Ok(bytes) => loaded.push(LoadedFont {
                font_name: reference.font_name.clone(),
                style: reference.style,
                part_path: reference.part_path.clone(),
                content_type: reference.content_type.clone(),
                bytes,
            }),
            Err(error @ AssetError::ResourceLimit(_)) => return Err(error),
            Err(_) => continue,
        }
    }
    Ok(loaded)
}
