use std::sync::Arc;
use crossbeam_channel::Sender;

use crate::contracts::geometry::RectF32;
use crate::contracts::pipeline::{RenderTargetSpec, RgbaSlide, WarningEvent};
use crate::contracts::vector_slide::VectorSlide;
use crate::error::RenderError;
use crate::loader::AssetStore;

/// Type alias for rectangles expressed in English Metric Units (EMU).
pub type EmuRect = RectF32;

/// Recognized input document container formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    Pptx,
    Pdf,
}

/// A Table of Contents or PDF outline entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    pub title: String,
    pub page_index: usize,
    pub level: usize,
}

/// An extracted line of text with its bounding box on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub rect: EmuRect,
}

/// Abstract search and text extraction corpus over a presentation or document.
pub trait SearchCorpus: Send + Sync {
    fn page_count(&self) -> usize;
    fn page_text(&self, page: usize) -> &[TextLine];
}

/// Trivial empty search corpus for documents with no extracted text.
#[derive(Clone, Debug, Default)]
pub struct EmptySearchCorpus;

impl SearchCorpus for EmptySearchCorpus {
    fn page_count(&self) -> usize {
        0
    }

    fn page_text(&self, _page: usize) -> &[TextLine] {
        &[]
    }
}

/// Uniform page-indexed presentation or document source.
///
/// Bridges PPTX slide decks and PDF documents behind a shared contract so
/// pipelines, presenters, and exporters can consume them interchangeably.
pub trait PageSource: Send + Sync {
    /// Number of pages or slides in this source.
    fn len(&self) -> usize;

    /// Whether the source contains zero pages.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Size of page `index` in integer English Metric Units (EMU).
    /// Mixed page dimensions within a single document are fully supported.
    fn page_size_emu(&self, index: usize) -> (i64, i64);

    /// Compiles page `index` into an intermediate vector display list.
    fn compile_page(
        &self,
        index: usize,
        warnings: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError>;

    /// Renders a page natively at the exact pixel target, bypassing the
    /// Tier-1 vector pipeline.
    ///
    /// Raster-native sources (PDF pages) implement this so the interactive
    /// pipeline never pays a codec round-trip: the returned bitmap is cached
    /// in Tier 2 directly. `None` (the default) keeps the compile→rasterize
    /// path. Errors are reported as `Some(Err(..))` so a source can still
    /// fall back to `compile_page` when native rendering fails.
    fn render_raster(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Option<Result<RgbaSlide, RenderError>> {
        let _ = (index, target);
        None
    }

    /// Optional speaker notes for page `index`.
    fn notes(&self, _index: usize) -> Option<String> {
        None
    }

    /// Table of contents / outline entries.
    fn toc(&self) -> Vec<TocEntry> {
        Vec::new()
    }

    /// Extracted text corpus for in-document search and indexing.
    fn search_corpus(&self) -> Arc<dyn SearchCorpus> {
        Arc::new(EmptySearchCorpus)
    }

    /// Optional underlying asset store for embedded media/images.
    fn asset_store(&self) -> Option<Arc<dyn AssetStore>> {
        None
    }
}

/// High-level presentation or document loaded from a container.
pub enum DeckSource {
    Pptx(pptx_model::Presentation, Arc<dyn AssetStore>),
    Pdf(Arc<dyn PageSource>),
}
