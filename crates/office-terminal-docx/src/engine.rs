//! Pluggable document layout engine seam (PRD 15 §3.3, issue #40 §7.5).
//!
//! The pipeline, bridge and page source only know [`DocxEngine`]; the
//! concrete `rdocx` implementation is isolated behind it so a future native
//! engine can replace `RdocxEngine` without touching anything downstream.
//! Tests build hand-authored page frames through the same trait.

#[cfg(feature = "rdocx-backend")]
use std::collections::HashMap;
#[cfg(feature = "rdocx-backend")]
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
#[cfg(feature = "rdocx-backend")]
use std::time::Instant;

use office_terminal_core::text::document_faces::DocumentFaceId;
use oxml_layout::{DocumentMetadata, FontId, MediaId, OutlineEntry, PageFrame};

#[cfg(feature = "rdocx-backend")]
use crate::error::{page_count_violation, DocxError};
#[cfg(feature = "rdocx-backend")]
use crate::fonts::register_engine_fonts;
#[cfg(feature = "rdocx-backend")]
use crate::limits::DocxLimits;
#[cfg(feature = "rdocx-backend")]
use crate::media::collect_page_media;

/// Raw media bytes keyed by the engine's stable content address.
#[derive(Clone, Debug)]
pub struct MediaBytes {
    pub data: Arc<[u8]>,
    pub content_type: String,
}

/// Per-font identity the text bridge needs to emit a run: the family for
/// family-based fallbacks and SVG selectable text, plus the exact registered
/// face when the engine's glyph IDs are only valid for it.
#[derive(Clone, Debug)]
pub struct ResolvedFont {
    pub family: String,
    /// `None` forces the family-lookup path (synthetic styles in v1).
    pub face: Option<DocumentFaceId>,
}

/// Opening words of the soft-deadline diagnostic. The page source classifies
/// by this prefix, so a timing observation is reported as a resource event
/// rather than as a fidelity loss.
pub(crate) const LAYOUT_DEADLINE_PREFIX: &str = "layout took";

/// Backend-neutral document layout surface consumed by the bridge.
pub trait DocxEngine: Send + Sync {
    /// Number of paginated pages.
    fn page_count(&self) -> usize;
    /// Page frame by 0-based index.
    fn page(&self, index: usize) -> Option<Arc<PageFrame>>;
    /// Font identity by the engine's font id (glyph runs reference it).
    fn resolve_font(&self, id: FontId) -> Option<&ResolvedFont>;
    /// Every media id referenced by positioned images (for the asset store).
    fn media_ids(&self) -> Vec<MediaId>;
    /// Raw media bytes by id, when the engine can supply them.
    fn media(&self, id: MediaId) -> Option<MediaBytes>;
    /// Heading outline entries with 0-based page indices.
    fn outlines(&self) -> &[OutlineEntry];
    /// 0-based **physical** page of a unique placed bookmark target
    /// (`WordLayoutResult::bookmark_page_section` minus one; the displayed
    /// number diverges after a section restart — issue #40 §7.7.7).
    fn bookmark_page(&self, name: &str) -> Option<usize>;
    /// Rendering approximations collected during layout.
    fn diagnostics(&self) -> &[String];
    /// Document metadata (title, author, …), when the package declares it.
    fn metadata(&self) -> Option<&DocumentMetadata>;
    /// Logical structure tree (paragraph / table / cell roles) the engine
    /// attached to the positioned elements, when it produces one. The
    /// default `None` keeps fixture engines and engines without source
    /// semantics on the purely geometric reading-order heuristics.
    fn structure(&self) -> Option<&oxml_layout::DocumentStructure> {
        None
    }
}

/// The `rdocx` 0.17 layout engine adapter (issue #40 §6 decision).
#[cfg(feature = "rdocx-backend")]
pub struct RdocxEngine {
    layout: Arc<rdocx_layout::WordLayoutResult>,
    pages: Vec<Arc<PageFrame>>,
    font_slots: Vec<Option<ResolvedFont>>,
    media: HashMap<MediaId, MediaBytes>,
    outlines: Vec<OutlineEntry>,
    diagnostics: Vec<String>,
    metadata: Option<DocumentMetadata>,
    structure: Option<oxml_layout::DocumentStructure>,
}

#[cfg(feature = "rdocx-backend")]
impl RdocxEngine {
    /// Parse and paginate a DOCX package under `limits`.
    ///
    /// Layout runs exactly once at open, inside a panic boundary: a malformed
    /// third-party input must surface as [`DocxError::LayoutEnginePanic`],
    /// never abort the interactive presenter (PRD 15 §8.2).
    pub fn open_with_limits(bytes: Arc<[u8]>, limits: &DocxLimits) -> Result<Self, DocxError> {
        let started = Instant::now();
        let outcome: Result<Arc<rdocx_layout::WordLayoutResult>, DocxError> = catch_unwind(AssertUnwindSafe(|| {
            let document = rdocx::Document::from_bytes_with_limits(&bytes, limits.package_limits())
                .map_err(map_rdocx_open_error)?;
            document.layout_deterministic().map_err(map_rdocx_open_error)
        }))
        .map_err(panic_message)?;
        let layout = outcome?;
        let result = &layout.layout;

        let pages = result.pages.clone();
        if pages.len() > limits.max_pages {
            return Err(page_count_violation(pages.len(), limits.max_pages));
        }

        // Soft deadline: record, never abort a running pagination
        // (issue #40 §7.14).
        let elapsed = started.elapsed();
        let mut diagnostics: Vec<String> = result.diagnostics.iter().map(|d| d.message.clone()).collect();
        if elapsed > limits.layout_deadline {
            diagnostics.push(format!(
                "{LAYOUT_DEADLINE_PREFIX} {:.1}s (soft deadline {:.0}s)",
                elapsed.as_secs_f64(),
                limits.layout_deadline.as_secs_f64()
            ));
        }

        let font_data = result.fonts.clone();
        let font_slots = register_engine_fonts(&font_data, &mut diagnostics);
        let media = collect_page_media(&pages);
        let outlines = result.outlines.clone();
        let metadata = result.metadata.clone();
        let structure = result.structure.clone();

        Ok(Self {
            layout,
            pages,
            font_slots,
            media,
            outlines,
            diagnostics,
            metadata,
            structure,
        })
    }
}

#[cfg(feature = "rdocx-backend")]
impl DocxEngine for RdocxEngine {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page(&self, index: usize) -> Option<Arc<PageFrame>> {
        self.pages.get(index).cloned()
    }

    fn resolve_font(&self, id: FontId) -> Option<&ResolvedFont> {
        self.font_slots.get(id.0 as usize).and_then(|slot| slot.as_ref())
    }

    fn media_ids(&self) -> Vec<MediaId> {
        let mut ids: Vec<MediaId> = self.media.keys().copied().collect();
        ids.sort_unstable_by_key(|id| id.0);
        ids
    }

    fn media(&self, id: MediaId) -> Option<MediaBytes> {
        self.media.get(&id).cloned()
    }

    fn outlines(&self) -> &[OutlineEntry] {
        &self.outlines
    }

    fn bookmark_page(&self, name: &str) -> Option<usize> {
        self.layout
            .bookmark_page_section(name)
            .map(|section| section.physical_page.saturating_sub(1))
    }

    fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    fn metadata(&self) -> Option<&DocumentMetadata> {
        self.metadata.as_ref()
    }

    fn structure(&self) -> Option<&oxml_layout::DocumentStructure> {
        self.structure.as_ref()
    }
}

/// Map an `rdocx` open/layout error onto the typed taxonomy (issue #40 §7.14):
/// package limits keep their identity, encryption stays actionable, and a
/// limit crossing is never relabelled as a corrupt archive.
#[cfg(feature = "rdocx-backend")]
fn map_rdocx_open_error(error: rdocx::Error) -> DocxError {
    use oxml_opc::OpcError;
    match &error {
        rdocx::Error::Opc(OpcError::PackageLimitExceeded { kind, limit }) => {
            DocxError::ResourceLimit(Box::new(ooxml_common::resource::ResourceViolation {
                stage: "docx-package",
                resource: "package",
                metric: kind,
                part: None,
                limit: *limit,
                observed: *limit,
                configurable: true,
                usage: ooxml_common::resource::ResourceUsage::default(),
            }))
        }
        rdocx::Error::Opc(OpcError::PartNotFound(part)) => {
            if part.contains("EncryptedPackage") {
                DocxError::Encrypted
            } else {
                DocxError::Package(error.to_string())
            }
        }
        rdocx::Error::Layout(_) => DocxError::Layout(error.to_string()),
        rdocx::Error::NoDocumentPart => DocxError::Package(error.to_string()),
        _ => DocxError::Package(error.to_string()),
    }
}

/// Extract a readable message from a caught panic payload.
#[cfg(feature = "rdocx-backend")]
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> DocxError {
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown layout engine panic".to_string());
    DocxError::LayoutEnginePanic(truncate(&message, 300))
}

#[cfg(any(feature = "rdocx-backend", test))]
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "rdocx-backend")]
    #[test]
    fn package_limit_errors_keep_their_identity() {
        use oxml_opc::OpcError;
        let error = rdocx::Error::Opc(OpcError::PackageLimitExceeded {
            kind: "max_entries",
            limit: 10,
        });
        let mapped = map_rdocx_open_error(error);
        assert!(matches!(mapped, DocxError::ResourceLimit(_)));
    }

    #[cfg(feature = "rdocx-backend")]
    #[test]
    fn missing_encrypted_package_maps_to_the_encrypted_error() {
        use oxml_opc::OpcError;
        let error = rdocx::Error::Opc(OpcError::PartNotFound("word/EncryptedPackage".to_string()));
        assert!(matches!(map_rdocx_open_error(error), DocxError::Encrypted));
    }

    #[cfg(feature = "rdocx-backend")]
    #[test]
    fn panic_payloads_become_typed_engine_panics() {
        let error = panic_message(Box::new("kaboom".to_string()));
        match error {
            DocxError::LayoutEnginePanic(message) => assert_eq!(message, "kaboom"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn long_panic_messages_are_truncated() {
        assert_eq!(truncate("short", 300), "short");
        let long = "x".repeat(500);
        let cut = truncate(&long, 300);
        assert_eq!(cut.chars().count(), 301);
        assert!(cut.ends_with('…'));
    }
}
