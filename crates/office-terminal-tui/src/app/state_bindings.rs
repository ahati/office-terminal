//! Last-drawn-frame geometry publication for [`PresentationState`].
//!
//! Split out of `state.rs` to keep the state module under the project's
//! 600-line ceiling. The render pass fills a [`FrameBindings`] value per
//! frame; the run loop publishes it here after the graphics sync so
//! input-side queries (hyperlink hit-testing, reader pan bounds, column
//! counts) observe exactly the geometry that is on screen — never
//! mid-frame state.

use office_terminal_display::CellRect;

use crate::app::bindings::{FrameBindings, SlidePlacement};
use crate::app::state::PresentationState;

impl PresentationState {
    /// Publishes what the last drawn frame produced so input-side queries
    /// (hyperlink hit-testing, reader pan bounds, column counts) observe
    /// exactly the geometry that is on screen.
    ///
    /// Called by the run loop once per drawn frame, after the graphics sync:
    /// the render pass fills a [`FrameBindings`] value and hands it over.
    pub fn publish_frame_bindings(&mut self, bindings: FrameBindings) {
        self.current_placement = bindings.current;
        self.preview_placement = bindings.preview;
        self.thumbnail_placements = bindings.thumbnails;
        self.reader_placements_last = bindings.reader;
        self.reader_columns_last = bindings.reader_columns;
        // Route the layout column count into the reader so event-side
        // queries (`visible_pages`, row steps) match the drawn geometry.
        if self.reader_columns_last > 0 {
            if let Some(reader) = self.reader_state.as_mut() {
                reader.set_num_columns(self.reader_columns_last);
            }
        }
    }

    /// Zoom-aware placement of the full-screen current slide in the last
    /// drawn frame (rect plus the visible crop window when zoomed).
    pub fn current_placement(&self) -> Option<SlidePlacement> {
        self.current_placement
    }

    /// Fitted rect of the full-screen current slide in the last drawn frame.
    pub fn current_placement_rect(&self) -> Option<CellRect> {
        self.current_placement.map(|placement| placement.rect)
    }

    /// Fitted rect of the presenter next-slide preview in the last drawn frame.
    pub fn preview_placement_rect(&self) -> Option<CellRect> {
        self.preview_placement.map(|placement| placement.rect)
    }

    /// `(slide, rect)` pairs the last drawn sorter frame recorded.
    pub fn thumbnail_placements(&self) -> &[(usize, CellRect)] {
        &self.thumbnail_placements
    }
}
