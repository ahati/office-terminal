//! High-fidelity PDF document and slide deck input engine for kitty-pptx.

pub mod detect;
pub mod engine;
pub mod error;
pub mod limits;
pub mod links;
pub mod raster_bridge;
pub mod source;
pub mod text;
pub mod vector_bridge;

use std::sync::Arc;

pub use detect::{classify_mode, is_encrypted, is_pdf, parse_version, ReaderModeDecision};
pub use engine::{HayroEngine, PdfEngine};
pub use error::PdfError;
pub use source::PdfPageSource;

/// Registers the PDF loader hook into `kitty-pptx-core` presentation loader.
pub fn register() {
    kitty_pptx_core::loader::presentation_loader::register_pdf_loader(|bytes| {
        PdfPageSource::open(bytes)
            .map(|src| Arc::new(src) as Arc<dyn kitty_pptx_core::PageSource>)
            .map_err(|e| kitty_pptx_core::ParseError::ArchiveError(e.to_string()))
    });
}
