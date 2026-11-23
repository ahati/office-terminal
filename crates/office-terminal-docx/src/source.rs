//! [`PageSource`] adapter for paginated WordprocessingML documents
//! (PRD 15 §4.1, issue #40 §7.6).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use crossbeam_channel::Sender;
use office_terminal_core::contracts::{
    DocumentMetadata, PageSource, SearchCorpus, TocEntry, VectorSlide, WarningEvent, EMU_PER_PT,
};
use office_terminal_core::{AssetStore, RenderError};
use oxml_layout::PageFrame;

use crate::bridge;
use crate::cache::IrCache;
use crate::engine::DocxEngine;
#[cfg(feature = "rdocx-backend")]
use crate::engine::RdocxEngine;
#[cfg(feature = "rdocx-backend")]
use crate::error::DocxError;
use crate::limits::DocxLimits;
use crate::media::DocxAssetStore;
use crate::search::DocxSearchCorpus;

/// A DOCX document opened as a page-indexed vector source.
///
/// The engine paginates once at open; every page compiles lazily into the
/// shared display list IR behind a bounded cache, and the pipeline's compile →
/// rasterize / SVG-export paths treat it exactly like any other document.
pub struct DocxPageSource {
    engine: Arc<dyn DocxEngine>,
    media: Arc<DocxAssetStore>,
    ir_cache: IrCache,
    corpus: OnceLock<Arc<DocxSearchCorpus>>,
    /// The limits the document was opened under; every lazy consumer
    /// (corpus extraction, …) must honour the same bounds as the open path.
    limits: DocxLimits,
    /// Diagnostics already surfaced through the warning channel; the bridge
    /// re-compiles pages per surface, so each must fire once per document.
    emitted: Arc<Mutex<HashSet<&'static str>>>,
}

impl DocxPageSource {
    /// Open and paginate a DOCX package with the default limits.
    #[cfg(feature = "rdocx-backend")]
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, DocxError> {
        Self::open_with_limits(bytes, &DocxLimits::default())
    }

    /// Open and paginate a DOCX package under explicit resource limits.
    #[cfg(feature = "rdocx-backend")]
    pub fn open_with_limits(bytes: Arc<[u8]>, limits: &DocxLimits) -> Result<Self, DocxError> {
        let engine = Arc::new(RdocxEngine::open_with_limits(bytes, limits)?);
        Ok(Self::from_engine_parts(engine, limits))
    }

    /// Build a source from any engine (tests use hand-authored page frames).
    pub fn from_engine(engine: Arc<dyn DocxEngine>) -> Self {
        Self::from_engine_parts(engine, &DocxLimits::default())
    }

    fn from_engine_parts(engine: Arc<dyn DocxEngine>, limits: &DocxLimits) -> Self {
        let media = Arc::new(DocxAssetStore::from_engine_with_budget(
            engine.as_ref(),
            limits.image_cache_bytes,
        ));
        Self {
            engine,
            media,
            ir_cache: IrCache::new(limits.ir_cache_entries, limits.ir_cache_bytes),
            corpus: OnceLock::new(),
            limits: *limits,
            emitted: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Engine diagnostics, surfaced once through the per-page warning channel.
    fn emit_engine_diagnostics(&self, warnings: &Sender<WarningEvent>) {
        let mut emitted = self
            .emitted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !emitted.insert("diagnostics") {
            return;
        }
        for message in self.engine.diagnostics() {
            // A soft-deadline crossing is a resource observation, not a
            // rendering approximation: it must not pollute the fidelity
            // warning tally.
            let event = if message.starts_with(crate::engine::LAYOUT_DEADLINE_PREFIX) {
                WarningEvent::ResourceLimitHit {
                    limit: "docx-layout-deadline",
                }
            } else {
                WarningEvent::FidelityDegraded {
                    element_id: "docx-engine".to_string(),
                    feature: feature_tag(message),
                    class: office_terminal_core::contracts::FidelityClass::Degraded,
                }
            };
            let _ = warnings.send(event);
        }
    }
}

impl PageSource for DocxPageSource {
    fn len(&self) -> usize {
        self.engine.page_count()
    }

    fn page_size_emu(&self, index: usize) -> (i64, i64) {
        self.engine
            .page(index)
            .map(|page| {
                (
                    (page.width * f64::from(EMU_PER_PT)).round() as i64,
                    (page.height * f64::from(EMU_PER_PT)).round() as i64,
                )
            })
            .unwrap_or((0, 0))
    }

    fn compile_page(
        &self,
        index: usize,
        warnings: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        if index >= self.len() {
            return Err(RenderError::CompilationFailed(format!(
                "DOCX page {} out of bounds (total: {})",
                index,
                self.len()
            )));
        }
        self.emit_engine_diagnostics(warnings);

        let engine = Arc::clone(&self.engine);
        let media = Arc::clone(&self.media);
        let emitted = Arc::clone(&self.emitted);
        let slide = self.ir_cache.get_or_compile(index, || {
            let page: Arc<PageFrame> = engine.page(index).ok_or_else(|| {
                RenderError::CompilationFailed(format!("DOCX page {index} missing"))
            })?;
            let mut slide = bridge::compile_page(&page, engine.as_ref(), media.as_ref(), warnings, emitted)?;
            // Metafile markers expand where the asset store is available,
            // exactly like the PPTX adapter (PRD 13 §3.1).
            office_terminal_core::metafile::expand_metafiles(&mut slide, media.as_ref(), warnings);
            Ok(slide)
        })?;
        // `PageSource::compile_page` hands out an owned display list, and the
        // cache keeps its own handle, so the clone here is unavoidable; what
        // the cache saves is the page-frame → IR bridging, not this copy.
        Ok(slide.as_ref().clone())
    }

    fn notes(&self, _index: usize) -> Option<String> {
        // WordprocessingML has no speaker notes; the presenter pane shows the
        // standard empty-notes placeholder (issue #40 §7.11).
        None
    }

    fn toc(&self) -> Vec<TocEntry> {
        self.engine
            .outlines()
            .iter()
            .filter(|entry| !entry.title.trim().is_empty())
            .map(|entry| TocEntry {
                title: entry.title.clone(),
                page_index: entry.page_index,
                level: entry.level as usize,
            })
            .collect()
    }

    fn page_labels(&self) -> Option<Vec<String>> {
        // Word section page-numbering restarts show through as logical
        // labels; the TUI falls back to 1-based physical numbers otherwise.
        let total = self.len();
        let mut labels = Vec::with_capacity(total);
        let mut any_divergence = false;
        for index in 0..total {
            let displayed = self
                .engine
                .page(index)
                .map(|page| page.displayed_page_number)
                .unwrap_or(index + 1);
            if displayed != index + 1 {
                any_divergence = true;
            }
            labels.push(displayed.to_string());
        }
        any_divergence.then_some(labels)
    }

    fn search_corpus(&self) -> Arc<dyn SearchCorpus> {
        let corpus = self.corpus.get_or_init(|| {
            let pages: Vec<Arc<PageFrame>> =
                (0..self.engine.page_count()).filter_map(|i| self.engine.page(i)).collect();
            Arc::new(DocxSearchCorpus::build(&pages, &self.limits))
        });
        Arc::clone(corpus) as Arc<dyn SearchCorpus>
    }

    fn asset_store(&self) -> Option<Arc<dyn AssetStore>> {
        Some(Arc::clone(&self.media) as Arc<dyn AssetStore>)
    }

    fn metadata(&self) -> Option<DocumentMetadata> {
        // Map the engine's record onto the engine-agnostic contract so the
        // type-erased `DeckSource` arms can print it (issue #40 §7.11).
        let meta = self.engine.metadata()?;
        let meta = DocumentMetadata {
            title: meta.title.clone(),
            author: meta.author.clone(),
            subject: meta.subject.clone(),
            keywords: meta.keywords.clone(),
            creator: meta.creator.clone(),
        };
        (meta != DocumentMetadata::default()).then_some(meta)
    }
}

/// Map a per-document diagnostic string onto the small closed feature
/// vocabulary the pipeline already deduplicates on; unknown messages
/// collapse into the generic engine bucket so the warning channel cannot be
/// flooded with unique strings.
fn feature_tag(message: &str) -> &'static str {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("font") {
        "docx-engine-font"
    } else if lowered.contains("image") || lowered.contains("media") {
        "docx-engine-media"
    } else if lowered.contains("grid") || lowered.contains("spacing") {
        "docx-engine-spacing"
    } else {
        "docx-engine"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FixtureEngine;
    use oxml_layout::PositionedElement;

    fn engine_with(diagnostics: Vec<String>, pages: Vec<Arc<PageFrame>>) -> Arc<FixtureEngine> {
        let engine = FixtureEngine::new(Vec::new())
            .with_font(0, "Carlito");
        // Rebuild with real pages: FixtureEngine::new takes owned frames.
        let mut engine = engine;
        engine.pages = pages;
        engine.diagnostics = diagnostics;
        Arc::new(engine)
    }

    fn text_page(page_number: usize, text: &str) -> Arc<PageFrame> {
        Arc::new(PageFrame::new(
            page_number,
            612.0,
            792.0,
            vec![PositionedElement::Text(crate::test_support::text_run(
                text, 72.0, 90.0, 0, 12.0,
            ))],
        ))
    }

    #[test]
    fn out_of_bounds_pages_are_typed_errors() {
        let source = DocxPageSource::from_engine(engine_with(Vec::new(), vec![text_page(1, "hi")]));
        let (tx, _rx) = crossbeam_channel::unbounded();
        assert!(source.compile_page(1, &tx).is_err());
        assert!(source.compile_page(0, &tx).is_ok());
    }

    #[test]
    fn diagnostics_fire_once_not_per_compile() {
        let engine = engine_with(
            vec!["a font substitution happened".to_string()],
            vec![text_page(1, "a"), text_page(2, "b")],
        );
        let source = DocxPageSource::from_engine(engine);
        let (tx, rx) = crossbeam_channel::unbounded();
        let _ = source.compile_page(0, &tx);
        let _ = source.compile_page(1, &tx);
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1, "engine diagnostics dedupe across pages");
    }

    #[test]
    fn mixed_displayed_numbers_become_page_labels() {
        let mut pages = vec![text_page(1, "a"), text_page(2, "b")];
        if let Some(page) = Arc::get_mut(&mut pages[1]) {
            page.displayed_page_number = 5; // section restart
        }
        let source = DocxPageSource::from_engine(engine_with(Vec::new(), pages));
        let labels = source.page_labels().expect("restart divergence");
        assert_eq!(labels, vec!["1".to_string(), "5".to_string()]);
    }

    #[test]
    fn sequential_pages_have_no_labels() {
        let source = DocxPageSource::from_engine(engine_with(
            Vec::new(),
            vec![text_page(1, "a"), text_page(2, "b")],
        ));
        assert!(source.page_labels().is_none());
    }

    #[test]
    fn page_sizes_convert_pt_to_emu() {
        let source = DocxPageSource::from_engine(engine_with(Vec::new(), vec![text_page(1, "a")]));
        let (w, h) = source.page_size_emu(0);
        assert_eq!(w, 612 * 12_700);
        assert_eq!(h, 792 * 12_700);
    }

    #[test]
    fn corpus_is_shared_and_lazy() {
        let source = DocxPageSource::from_engine(engine_with(Vec::new(), vec![text_page(1, "needle")]));
        let first = source.search_corpus();
        let second = source.search_corpus();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.page_text(0)[0].text, "needle");
    }

    #[test]
    fn feature_tags_collapse_into_a_small_vocabulary() {
        assert_eq!(feature_tag("a font substitution"), "docx-engine-font");
        assert_eq!(feature_tag("image relationship missing"), "docx-engine-media");
        assert_eq!(feature_tag("docGrid spacing divergence"), "docx-engine-spacing");
        assert_eq!(feature_tag("something else"), "docx-engine");
    }

    #[test]
    fn soft_deadline_diagnostics_are_resource_events_not_fidelity_losses() {
        let engine = engine_with(
            vec![format!("{} 31.0s (soft deadline 30s)", crate::engine::LAYOUT_DEADLINE_PREFIX)],
            vec![text_page(1, "a")],
        );
        let source = DocxPageSource::from_engine(engine);
        let (tx, rx) = crossbeam_channel::unbounded();
        let _ = source.compile_page(0, &tx);
        let events: Vec<_> = rx.try_iter().collect();
        assert!(matches!(
            events.as_slice(),
            [WarningEvent::ResourceLimitHit { limit: "docx-layout-deadline" }]
        ));
    }

    #[test]
    fn declared_metadata_reaches_the_page_source_contract() {
        let mut metadata = oxml_layout::DocumentMetadata {
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: None,
        };
        metadata.title = Some("Quarterly report".to_string());
        metadata.author = Some("A. Writer".to_string());
        let mut engine = FixtureEngine::new(Vec::new());
        engine.pages = vec![text_page(1, "a")];
        engine.metadata = Some(metadata);

        let source = DocxPageSource::from_engine(Arc::new(engine));
        let meta = source.metadata().expect("declared metadata surfaces");
        assert_eq!(meta.title.as_deref(), Some("Quarterly report"));
        assert_eq!(meta.author.as_deref(), Some("A. Writer"));
        assert_eq!(meta.subject, None);

        // An entirely empty record reads as "no metadata".
        let mut blank = FixtureEngine::new(Vec::new());
        blank.pages = vec![text_page(1, "a")];
        blank.metadata = Some(oxml_layout::DocumentMetadata::default());
        let blank = Arc::new(blank);
        assert!(DocxPageSource::from_engine(blank).metadata().is_none());
    }

    #[test]
    fn asset_store_serves_the_document_media_surface() {
        let source = DocxPageSource::from_engine(engine_with(Vec::new(), vec![text_page(1, "a")]));
        assert!(source.asset_store().is_some());
    }
}
