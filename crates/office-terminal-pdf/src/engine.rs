use std::sync::Arc;
use hayro::RenderCache;
use hayro_syntax::object::String as PdfString;
use hayro_syntax::object::{Dict, ObjectIdentifier};
use hayro_syntax::page::Rotation;
use hayro_syntax::Pdf;
use office_terminal_core::{RenderTargetSpec, RgbaSlide, SlideLink, TextLine, TocEntry, VectorSlide};

use crate::detect::is_encrypted;
use crate::error::PdfError;
use crate::limits::calculate_target_scale;
use crate::links::extract_page_links;
use crate::raster_bridge::{compile_page_raster, render_page_rgba};
use crate::text::extract_page_text;
use crate::vector_bridge::compile_page_vector;

/// Abstract engine interface for PDF interpretation and rendering (PRD 14 §2.3).
pub trait PdfEngine: Send + Sync {
    fn page_count(&self) -> usize;
    fn page_size_pt(&self, page: usize) -> (f32, f32);
    fn page_rotation(&self, page: usize) -> u16;
    fn render_rgba(&self, page: usize, scale: f32) -> Result<RgbaSlide, PdfError>;
    fn render_raster(&self, page: usize, target: &RenderTargetSpec) -> Result<RgbaSlide, PdfError>;
    fn compile_raster(&self, page: usize) -> Result<VectorSlide, PdfError>;
    fn compile_vector(&self, page: usize) -> Result<VectorSlide, PdfError>;
    fn outline(&self) -> Vec<TocEntry>;
    fn links(&self, page: usize) -> Vec<SlideLink>;
    fn extract_text(&self, page: usize) -> Result<Vec<TextLine>, PdfError>;
}

/// Hayro-based pure-Rust PDF engine implementation.
pub struct HayroEngine {
    pdf: Pdf,
}

/// Sized wrapper handing the shared PDF bytes to hayro with zero copies.
///
/// hayro's `PdfData` accepts `Arc<T>` only for `T: Sized` (generic params
/// are implicitly sized), so an `Arc<[u8]>` cannot be transferred directly
/// and would force a full-file `to_vec()`. Wrapping the shared buffer in
/// this sized newtype satisfies `impl From<Arc<T>> for PdfData` and lets
/// the engine and the loader share one buffer instead of two.
struct SharedPdfBytes(Arc<[u8]>);

impl AsRef<[u8]> for SharedPdfBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl HayroEngine {
    pub fn open(data: Arc<[u8]>) -> Result<Self, PdfError> {
        if is_encrypted(&data) {
            return Err(PdfError::EncryptedUnsupported);
        }

        let pdf = Pdf::new(Arc::new(SharedPdfBytes(Arc::clone(&data)))).map_err(|e| {
            PdfError::Malformed {
                page: 0,
                detail: format!("Failed to parse PDF: {:?}", e),
            }
        })?;

        if pdf.pages().is_empty() {
            return Err(PdfError::EmptyDocument);
        }

        Ok(Self { pdf })
    }

    pub fn pdf(&self) -> &Pdf {
        &self.pdf
    }

    fn traverse_outlines(
        &self,
        first_id: ObjectIdentifier,
        level: usize,
        page_map: &std::collections::HashMap<ObjectIdentifier, usize>,
        root: &Dict<'_>,
        out: &mut Vec<TocEntry>,
    ) {
        let mut curr_id = Some(first_id);
        let mut visited = 0;

        while let Some(id) = curr_id {
            if visited > 1000 {
                break;
            }
            visited += 1;

            let Some(item) = self.pdf.xref().get::<Dict<'_>>(id) else {
                break;
            };

            let title = item
                .get::<PdfString<'_>>(b"Title")
                .map(|s| crate::text_encoding::decode_pdf_text_string(s.as_bytes()))
                .unwrap_or_else(|| format!("Section {}", out.len() + 1));

            let page_idx =
                crate::dest::resolve_destination(&item, page_map, root, self.pdf.xref()).unwrap_or(0);

            out.push(TocEntry {
                title,
                page_index: page_idx,
                level,
            });

            // Recurse into children
            if let Some(first_child) = item.get_ref(b"First") {
                self.traverse_outlines(first_child.into(), level + 1, page_map, root, out);
            }

            // Advance to next sibling
            curr_id = item.get_ref(b"Next").map(|r| r.into());
        }
    }
}

impl PdfEngine for HayroEngine {
    fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    fn page_size_pt(&self, page: usize) -> (f32, f32) {
        self.pdf
            .pages()
            .get(page)
            .map(|p| p.render_dimensions())
            .unwrap_or((612.0, 792.0))
    }

    fn page_rotation(&self, page: usize) -> u16 {
        self.pdf
            .pages()
            .get(page)
            .map(|p| match p.rotation() {
                Rotation::None => 0,
                Rotation::Horizontal => 90,
                Rotation::Flipped => 180,
                Rotation::FlippedHorizontal => 270,
            })
            .unwrap_or(0)
    }

    fn render_rgba(&self, page: usize, scale: f32) -> Result<RgbaSlide, PdfError> {
        let page_obj = self
            .pdf
            .pages()
            .get(page)
            .ok_or(PdfError::IndexOutOfBounds {
                index: page,
                total: self.page_count(),
            })?;

        let cache = RenderCache::new();
        render_page_rgba(page_obj, page, scale, &cache)
    }

    fn render_raster(&self, page: usize, target: &RenderTargetSpec) -> Result<RgbaSlide, PdfError> {
        let page_obj = self
            .pdf
            .pages()
            .get(page)
            .ok_or(PdfError::IndexOutOfBounds {
                index: page,
                total: self.page_count(),
            })?;

        // Render at the exact interactive pixel width, clamped so no axis
        // exceeds MAX_RASTER_AXIS. The effective dimensions match
        // `page_size_pt` (rotation-aware render dimensions).
        let (w_pt, h_pt) = page_obj.render_dimensions();
        let scale = calculate_target_scale(w_pt, h_pt, target.width_px);

        let cache = RenderCache::new();
        render_page_rgba(page_obj, page, scale, &cache)
    }

    fn compile_raster(&self, page: usize) -> Result<VectorSlide, PdfError> {
        let page_obj = self
            .pdf
            .pages()
            .get(page)
            .ok_or(PdfError::IndexOutOfBounds {
                index: page,
                total: self.page_count(),
            })?;

        let links = self.links(page);
        let cache = RenderCache::new();
        compile_page_raster(page_obj, page, &cache, links)
    }

    fn compile_vector(&self, page: usize) -> Result<VectorSlide, PdfError> {
        let page_obj = self
            .pdf
            .pages()
            .get(page)
            .ok_or(PdfError::IndexOutOfBounds {
                index: page,
                total: self.page_count(),
            })?;

        let links = self.links(page);
        compile_page_vector(page_obj, links)
    }

    fn outline(&self) -> Vec<TocEntry> {
        let mut entries = Vec::new();
        let root_id = self.pdf.xref().root_id();
        let Some(root) = self.pdf.xref().get::<Dict<'_>>(root_id) else {
            return entries;
        };

        let mut page_map = std::collections::HashMap::new();
        for (idx, page) in self.pdf.pages().iter().enumerate() {
            if let Some(id) = page.raw().obj_id() {
                page_map.insert(id, idx);
            }
        }

        if let Some(outlines) = root.get::<Dict<'_>>(b"Outlines") {
            if let Some(first_ref) = outlines.get_ref(b"First") {
                self.traverse_outlines(first_ref.into(), 0, &page_map, &root, &mut entries);
            }
        }

        entries
    }

    fn links(&self, page: usize) -> Vec<SlideLink> {
        self.pdf
            .pages()
            .get(page)
            .map(extract_page_links)
            .unwrap_or_default()
    }

    fn extract_text(&self, page: usize) -> Result<Vec<TextLine>, PdfError> {
        let page_obj = self
            .pdf
            .pages()
            .get(page)
            .ok_or(PdfError::IndexOutOfBounds {
                index: page,
                total: self.page_count(),
            })?;

        Ok(extract_page_text(page_obj))
    }
}
