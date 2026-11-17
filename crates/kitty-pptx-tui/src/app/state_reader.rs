//! Reader mode state extensions for [`PresentationState`].
//!
//! Split out of `state.rs` to keep the state module under the project's
//! 600-line ceiling.

use crate::app::mode::AppMode;
use crate::app::reader_state::{ReaderSlotPlacement, ReaderState};
use crate::app::state::PresentationState;

impl PresentationState {
    /// Enable document reader mode with total page count and page dimensions.
    pub fn enable_reader_mode(&mut self, total_pages: usize, page_sizes: Vec<(f32, f32)>) {
        if let Some(&(w_pt, h_pt)) = page_sizes.first() {
            let w_emu = (w_pt * 12_700.0).round() as i64;
            let h_emu = (h_pt * 12_700.0).round() as i64;
            self.set_slide_dimensions(w_emu, h_emu);
        }
        self.reader_state = Some(
            ReaderState::new(total_pages, page_sizes)
                .with_shared_placements(self.reader_placements.clone()),
        );
        self.mode = AppMode::Reader;
        self.previous_mode = AppMode::Reader;
    }

    /// Set human-readable presentation or document title.
    pub fn set_deck_title(&mut self, title: impl Into<String>) {
        self.deck_title = Some(title.into());
    }

    /// Presentation or document title if configured.
    pub fn presentation_title(&self) -> Option<&str> {
        self.deck_title.as_deref()
    }

    /// Read-only access to reader mode state.
    pub fn reader_state(&self) -> Option<&ReaderState> {
        self.reader_state.as_ref()
    }

    /// Mutable access to reader mode state.
    pub fn reader_state_mut(&mut self) -> Option<&mut ReaderState> {
        self.reader_state.as_mut()
    }

    /// Update effective columns for multi-column reader layouts.
    pub fn set_reader_num_columns(&self, cols: usize) {
        if let Some(reader) = self.reader_state() {
            reader.set_num_columns(cols);
        }
    }

    /// Retrieve effective columns for multi-column reader layouts.
    pub fn reader_num_columns(&self) -> usize {
        self.reader_state().map(|r| r.num_columns()).unwrap_or(1)
    }

    /// Update staged placement coordinates recorded during reader layout rendering.
    pub fn update_reader_placements(&self, placements: Vec<ReaderSlotPlacement>) {
        if let Ok(mut lock) = self.reader_placements.lock() {
            *lock = placements;
        }
    }

    /// Retrieve recorded reader placement coordinates.
    pub fn reader_placements(&self) -> Vec<ReaderSlotPlacement> {
        self.reader_placements
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    /// Clear recorded reader placements.
    pub fn clear_reader_placements(&self) {
        if let Ok(mut lock) = self.reader_placements.lock() {
            lock.clear();
        }
    }

    /// Populate TOC search entries from table of contents.
    pub fn set_toc(&mut self, toc: &[kitty_pptx_core::TocEntry]) {
        self.toc = toc.to_vec();
        let total = self.total_slides;
        self.search.sync_from_toc(toc, total);
    }
}
