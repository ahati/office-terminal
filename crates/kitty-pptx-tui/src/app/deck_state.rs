//! Deck data — slide models, per-slide derivatives and document metadata.
//!
//! One owner for everything derived from the loaded presentation (or PDF):
//! slide models, per-slide media/link flags, physical slide geometry, the
//! table of contents and the deck title. [`super::state::PresentationState`]
//! keeps only navigation/session ephemera and delegates the accessors here.
//!
//! Terminal-layout state (columns/rows, last-frame geometry) is deliberately
//! NOT deck data and stays on the presentation state.

use pptx_model::Slide;
use kitty_pptx_core::{SlideLink, TocEntry};

/// Default slide geometry: a 16:9 widescreen deck in EMUs.
const DEFAULT_SLIDE_WIDTH_EMU: i64 = 12_192_000;
const DEFAULT_SLIDE_HEIGHT_EMU: i64 = 6_858_000;
const DEFAULT_ASPECT_RATIO: f32 = 16.0 / 9.0;

/// Loaded deck: slide models plus everything precomputed from them.
#[derive(Clone, Debug)]
pub struct DeckState {
    /// Speaker notes per slide (index-aligned with `slides`).
    pub(crate) notes: Vec<Option<String>>,
    /// Slide models in `sldIdLst` order (empty until attached).
    pub(crate) slides: Vec<Slide>,
    /// Per-slide "has external media" flags, computed once per deck so the
    /// sorter does not rescan element trees on every frame.
    pub(crate) slide_media_flags: Vec<bool>,
    /// Per-slide hyperlink lists for the picker and mouse hit-testing.
    pub(crate) slide_links: Vec<Vec<SlideLink>>,
    /// Physical slide width in EMUs (mouse hit-testing, zoom fits).
    pub(crate) slide_width_emu: i64,
    /// Physical slide height in EMUs.
    pub(crate) slide_height_emu: i64,
    /// `width / height` of the physical slide (fitted rendering).
    pub(crate) aspect_ratio: f32,
    /// Document table of contents (fuzzy TOC search source).
    pub(crate) toc: Vec<TocEntry>,
    /// Human-readable presentation or document title.
    pub(crate) deck_title: Option<String>,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            slides: Vec::new(),
            slide_media_flags: Vec::new(),
            slide_links: Vec::new(),
            slide_width_emu: DEFAULT_SLIDE_WIDTH_EMU,
            slide_height_emu: DEFAULT_SLIDE_HEIGHT_EMU,
            aspect_ratio: DEFAULT_ASPECT_RATIO,
            toc: Vec::new(),
            deck_title: None,
        }
    }
}

impl DeckState {
    /// A deck with its speaker notes attached (slide models come later via
    /// [`DeckState::set_slides`]).
    pub fn new(notes: Vec<Option<String>>) -> Self {
        Self {
            notes,
            ..Self::default()
        }
    }

    /// Speaker notes per slide, index-aligned with the slide list.
    pub fn notes(&self) -> &[Option<String>] {
        &self.notes
    }

    /// Replace the slide models and recompute the per-slide derivatives
    /// (media flags, hyperlink lists).
    pub fn set_slides(&mut self, slides: Vec<Slide>) {
        self.slide_media_flags = slides
            .iter()
            .map(crate::app::media::has_slide_media)
            .collect();
        self.slide_links = kitty_pptx_core::extract_deck_hyperlinks(&slides);
        self.slides = slides;
    }

    /// Slide models in `sldIdLst` order.
    pub fn slides(&self) -> &[Slide] {
        &self.slides
    }

    /// The slide model at `index`, if attached.
    pub fn slide(&self, index: usize) -> Option<&Slide> {
        self.slides.get(index)
    }

    /// Hyperlinks on the slide at `index` (empty when it has none).
    pub fn slide_links_for(&self, index: usize) -> &[SlideLink] {
        self.slide_links.get(index).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether the slide at `index` carries external media.
    pub fn slide_has_media(&self, index: usize) -> bool {
        self.slide_media_flags.get(index).copied().unwrap_or(false)
    }

    /// Set the physical slide dimensions (mouse hit-testing, zoom fits).
    /// Ignores non-positive values, matching the previous setter.
    pub fn set_slide_dimensions(&mut self, width_emu: i64, height_emu: i64) {
        if width_emu > 0 && height_emu > 0 {
            self.slide_width_emu = width_emu;
            self.slide_height_emu = height_emu;
            self.aspect_ratio = width_emu as f32 / height_emu as f32;
        }
    }

    /// Physical slide dimensions in EMUs (`width`, `height`).
    pub fn slide_dimensions(&self) -> (i64, i64) {
        (self.slide_width_emu, self.slide_height_emu)
    }

    /// Physical slide `width / height`.
    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    /// Replace the document table of contents (fuzzy TOC search source).
    pub fn set_toc(&mut self, toc: &[TocEntry]) {
        self.toc = toc.to_vec();
    }

    /// Document table of contents.
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    /// Set the human-readable presentation or document title.
    pub fn set_deck_title(&mut self, title: impl Into<String>) {
        self.deck_title = Some(title.into());
    }

    /// Presentation or document title if configured.
    pub fn presentation_title(&self) -> Option<&str> {
        self.deck_title.as_deref()
    }
}
