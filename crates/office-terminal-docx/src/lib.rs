//! DOCX (WordprocessingML) document input engine for Office Terminal.
//!
//! Turns a `.docx` / `.dotx` / `.docm` package into a
//! [`PageSource`](office_terminal_core::contracts::PageSource): a third-party
//! layout engine (issue #40 §6) paginates the flow document once at open,
//! then this crate bridges every positioned page frame into the shared
//! resolution-independent display list (PRD 15 §5), with exact-face glyph
//! painting, metafile playback, reading-order search text, heading outlines
//! and hyperlink annotations along for the ride.
//!
//! The pipeline, TUI and CLI never see an engine type: they consume pages,
//! rasters, text and links like any other document.
#![forbid(unsafe_code)]

mod bridge;
mod cache;
mod engine;
mod error;
mod fonts;
mod limits;
mod media;
mod search;
mod source;
#[cfg(test)]
mod test_support;

pub use engine::{DocxEngine, MediaBytes, ResolvedFont};
pub use error::DocxError;
pub use limits::DocxLimits;
pub use media::DocxAssetStore;
pub use search::DocxSearchCorpus;
pub use source::DocxPageSource;

#[cfg(feature = "rdocx-backend")]
pub use engine::RdocxEngine;

#[cfg(any(feature = "rdocx-backend", test))]
use std::sync::Arc;

/// Registers the DOCX loader hook into the core presentation loader
/// (mirrors the PDF crate's `register()`).
///
/// The hook maps typed engine errors onto the loader's taxonomy: encrypted
/// documents stay actionable password errors, resource limits keep their
/// identity, and everything else is a package error.
#[cfg(feature = "rdocx-backend")]
pub fn register() {
    office_terminal_core::loader::presentation_loader::register_docx_loader(|bytes| {
        DocxPageSource::open(bytes)
            .map(|source| Arc::new(source) as Arc<dyn office_terminal_core::contracts::PageSource>)
            .map_err(DocxError::into)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use office_terminal_core::contracts::PageSource;

    #[test]
    fn page_source_exposes_the_document_surface() {
        let engine = test_support::FixtureEngine::new(vec![test_support::text_page("hello")]);
        let source = DocxPageSource::from_engine(Arc::new(engine));
        assert_eq!(source.len(), 1);
        assert_eq!(source.page_size_emu(0), (612 * 12_700, 792 * 12_700));
        assert!(source.page_labels().is_none());
        assert!(source.asset_store().is_some());
        let corpus = source.search_corpus();
        assert_eq!(corpus.page_text(0)[0].text, "hello");
        assert!(source.metadata().is_none());
    }

    #[test]
    fn compiled_pages_expand_into_the_shared_ir() {
        let engine = test_support::FixtureEngine::new(vec![test_support::text_page("hello")])
            .with_font(0, "Carlito");
        let source = DocxPageSource::from_engine(Arc::new(engine));
        let (tx, _rx) = crossbeam_channel::unbounded();
        let slide = source.compile_page(0, &tx).expect("compile");
        assert_eq!(slide.width_emu, 612 * 12_700);
        assert_eq!(slide.commands.len(), 1);
        assert!(matches!(
            slide.commands[0],
            office_terminal_core::contracts::VectorCommand::DrawText { .. }
        ));
    }
}
