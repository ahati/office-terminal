//! Reader mode state extensions for [`PresentationState`].
//!
//! Split out of `state.rs` to keep the state module under the 600-line
//! ceiling.

use crate::app::mode_stack::BaseView;
use crate::app::reader_state::{ReaderSlotPlacement, ReaderState};
use crate::app::state::PresentationState;
use office_terminal_core::EMU_PER_PT;

impl PresentationState {
    /// Enable document reader mode with total page count and page dimensions.
    pub fn enable_reader_mode(&mut self, total_pages: usize, page_sizes: Vec<(f32, f32)>) {
        if let Some(&(w_pt, h_pt)) = page_sizes.first() {
            let w_emu = (w_pt * EMU_PER_PT).round() as i64;
            let h_emu = (h_pt * EMU_PER_PT).round() as i64;
            self.set_slide_dimensions(w_emu, h_emu);
        }
        self.reader_state = Some(ReaderState::new(total_pages, page_sizes));
        // Documents with logical page labels carry them into the fresh
        // reader state for status-bar display.
        if !self.deck.page_labels().is_empty() {
            if let Some(reader) = self.reader_state.as_mut() {
                reader.set_page_labels(self.deck.page_labels().to_vec());
            }
        }
        // Reader is a persistent base view: switch the mode stack's base so
        // overlays (find modal, help) remember it underneath.
        self.mode_stack.switch_base(BaseView::Reader);
    }

    /// Set human-readable presentation or document title.
    pub fn set_deck_title(&mut self, title: impl Into<String>) {
        self.deck.set_deck_title(title);
    }

    /// Presentation or document title if configured.
    pub fn presentation_title(&self) -> Option<&str> {
        self.deck.presentation_title()
    }

    /// Read-only access to reader mode state.
    pub fn reader_state(&self) -> Option<&ReaderState> {
        self.reader_state.as_ref()
    }

    /// Mutable access to reader mode state.
    pub fn reader_state_mut(&mut self) -> Option<&mut ReaderState> {
        self.reader_state.as_mut()
    }

    /// Effective reader column count: the layout output of the last drawn
    /// frame (0 before the first frame), falling back to the reader's own
    /// default until a frame publishes real geometry.
    pub fn reader_num_columns(&self) -> usize {
        if self.reader_columns_last > 0 {
            self.reader_columns_last
        } else {
            self.reader_state().map(|r| r.num_columns()).unwrap_or(1)
        }
    }

    /// Reader panes placed by the last drawn frame (input-side pan bounds).
    pub fn reader_placements_last(&self) -> &[ReaderSlotPlacement] {
        &self.reader_placements_last
    }

    /// Populate TOC search entries from table of contents.
    pub fn set_toc(&mut self, toc: &[office_terminal_core::TocEntry]) {
        self.deck.set_toc(toc);
        let total = self.total_slides;
        let labels = self.deck.page_labels().to_vec();
        self.search.sync_from_toc(toc, total, &labels);
    }

    /// Replace the logical page labels (PDF `/PageLabels`) and re-sync the
    /// TOC search index so entries carry the fresh labels.
    pub fn set_page_labels(&mut self, labels: Vec<String>) {
        self.deck.set_page_labels(labels);
        let labels = self.deck.page_labels().to_vec();
        if let Some(reader) = self.reader_state.as_mut() {
            reader.set_page_labels(labels);
        }
        if !self.deck.toc().is_empty() {
            let toc = self.deck.toc().to_vec();
            self.set_toc(&toc);
        }
    }
}
