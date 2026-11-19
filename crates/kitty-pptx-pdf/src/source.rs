use std::sync::Arc;
use crossbeam_channel::Sender;
use kitty_pptx_core::{
    AssetStore, PageSource, RenderError, RenderTargetSpec, RgbaSlide, SearchCorpus, TocEntry,
    VectorSlide, WarningEvent,
};

use crate::engine::{HayroEngine, PdfEngine};
use crate::error::PdfError;
use crate::text::PdfSearchCorpus;

/// Adapter implementing [`PageSource`] over an abstract [`PdfEngine`].
pub struct PdfPageSource {
    engine: Arc<dyn PdfEngine>,
    corpus: Arc<dyn SearchCorpus>,
    prefer_vector: bool,
}

impl PdfPageSource {
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, PdfError> {
        let engine = Arc::new(HayroEngine::open(bytes)?);
        Self::from_engine(engine)
    }

    pub fn from_engine(engine: Arc<dyn PdfEngine>) -> Result<Self, PdfError> {
        // Build the search corpus lazily: extracting text interprets every
        // page's content stream, which must stay off the startup path. The
        // first search query triggers a single build of all pages.
        let corpus = Arc::new(PdfSearchCorpus::lazy(Arc::clone(&engine)));

        Ok(Self {
            engine,
            corpus,
            prefer_vector: false,
        })
    }

    pub fn with_prefer_vector(mut self, prefer: bool) -> Self {
        self.prefer_vector = prefer;
        self
    }

    pub fn engine(&self) -> &Arc<dyn PdfEngine> {
        &self.engine
    }
}

impl PageSource for PdfPageSource {
    fn len(&self) -> usize {
        self.engine.page_count()
    }

    fn page_size_emu(&self, index: usize) -> (i64, i64) {
        let (w_pt, h_pt) = self.engine.page_size_pt(index);
        (
            (w_pt * 12_700.0).round() as i64,
            (h_pt * 12_700.0).round() as i64,
        )
    }

    fn compile_page(
        &self,
        index: usize,
        warnings: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        if index >= self.len() {
            return Err(RenderError::CompilationFailed(format!(
                "PDF page {} out of bounds (total pages: {})",
                index,
                self.len()
            )));
        }

        if self.prefer_vector {
            match self.engine.compile_vector(index) {
                Ok(slide) => return Ok(slide),
                Err(_e) => {
                    let _ = warnings.send(WarningEvent::FidelityDegraded {
                        element_id: format!("page-{}", index),
                        feature: "pdf-vector-fallback",
                        class: kitty_pptx_core::FidelityClass::Degraded,
                    });
                    // Fallback to raster bridge
                }
            }
        }

        self.engine
            .compile_raster(index)
            .map_err(|e| RenderError::CompilationFailed(format!("PDF page {}: {}", index, e)))
    }

    fn render_raster(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Option<Result<RgbaSlide, RenderError>> {
        Some(
            self.engine
                .render_raster(index, target)
                .map_err(|e| RenderError::CompilationFailed(format!("PDF page {}: {}", index, e))),
        )
    }

    fn notes(&self, _index: usize) -> Option<String> {
        None
    }

    fn toc(&self) -> Vec<TocEntry> {
        self.engine.outline()
    }

    fn search_corpus(&self) -> Arc<dyn SearchCorpus> {
        Arc::clone(&self.corpus)
    }

    fn asset_store(&self) -> Option<Arc<dyn AssetStore>> {
        None
    }
}
