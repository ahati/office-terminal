//! Shared test fixtures for the bridge and page source (unit-test only).
//!
//! Hand-authored `oxml_layout` page frames exercise the bridge without the
//! third-party engine, exactly the `FixtureDocxEngine` seam the design
//! prescribes (issue #40 §7.5).

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use office_terminal_core::contracts::{VectorSlide, WarningEvent};

use crate::engine::{DocxEngine, MediaBytes, ResolvedFont};
use oxml_layout::{
    Color, DocumentMetadata, FontId, GlyphRun, MediaId, OutlineEntry, PageFrame, Point, PositionedElement,
};

/// Configurable engine fixture over hand-built page frames.
pub(crate) struct FixtureEngine {
    pub pages: Vec<Arc<PageFrame>>,
    pub fonts: HashMap<FontId, ResolvedFont>,
    pub media: HashMap<MediaId, MediaBytes>,
    pub outlines: Vec<OutlineEntry>,
    pub bookmarks: HashMap<String, usize>,
    pub diagnostics: Vec<String>,
    pub metadata: Option<DocumentMetadata>,
}

impl FixtureEngine {
    pub fn new(pages: Vec<PageFrame>) -> Self {
        Self {
            pages: pages.into_iter().map(Arc::new).collect(),
            fonts: HashMap::new(),
            media: HashMap::new(),
            outlines: Vec::new(),
            bookmarks: HashMap::new(),
            diagnostics: Vec::new(),
            metadata: None,
        }
    }

    pub fn with_font(mut self, id: u32, family: &str) -> Self {
        self.fonts.insert(
            FontId(id),
            ResolvedFont {
                family: family.to_string(),
                face: None,
            },
        );
        self
    }

    pub fn with_media(mut self, id: MediaId, bytes: MediaBytes) -> Self {
        self.media.insert(id, bytes);
        self
    }

    pub fn with_bookmark(mut self, name: &str, page: usize) -> Self {
        self.bookmarks.insert(name.to_string(), page);
        self
    }
}

impl DocxEngine for FixtureEngine {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page(&self, index: usize) -> Option<Arc<PageFrame>> {
        self.pages.get(index).cloned()
    }

    fn resolve_font(&self, id: FontId) -> Option<&ResolvedFont> {
        self.fonts.get(&id)
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
        self.bookmarks.get(name).copied()
    }

    fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    fn metadata(&self) -> Option<&DocumentMetadata> {
        self.metadata.as_ref()
    }
}

/// One glyph run in the plain (monolingual) projection.
pub(crate) fn text_run(text: &str, x: f64, y: f64, font_id: u32, size: f64) -> GlyphRun {
    GlyphRun {
        origin: Point { x, y },
        font_id: FontId(font_id),
        font_size: size,
        glyph_ids: vec![1; text.chars().count()],
        advances: vec![size * 0.5; text.chars().count()],
        text: text.to_string(),
        source: None,
        color: Color::BLACK,
        bold: false,
        italic: false,
        field_kind: None,
        field_source: None,
        note: None,
        note_reference_source: None,
        tab_aligned: None,
    }
}

/// A page whose only element is one text run.
pub(crate) fn text_page(text: &str) -> PageFrame {
    PageFrame::new(
        1,
        612.0,
        792.0,
        vec![PositionedElement::Text(text_run(text, 72.0, 90.0, 0, 12.0))],
    )
}

/// Standard Letter portrait page frame.
pub(crate) fn letter_page(elements: Vec<PositionedElement>) -> PageFrame {
    PageFrame::new(1, 612.0, 792.0, elements)
}

/// Compile one page against a fixture engine, collecting warnings.
pub(crate) fn compile(page: &PageFrame, engine: &FixtureEngine) -> (VectorSlide, Vec<WarningEvent>) {
    let store = crate::media::DocxAssetStore::from_engine(engine);
    let (tx, rx) = crossbeam_channel::unbounded::<WarningEvent>();
    let slide = crate::bridge::compile_page(page, engine, &store, &tx, Arc::new(Mutex::new(HashSet::new())))
        .expect("bridge compile");
    let warnings: Vec<WarningEvent> = rx.try_iter().collect();
    (slide, warnings)
}

/// One raw media entry.
pub(crate) fn media_bytes(data: &[u8], content_type: &str) -> MediaBytes {
    MediaBytes {
        data: Arc::from(data),
        content_type: content_type.to_string(),
    }
}
