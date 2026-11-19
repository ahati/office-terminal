//! Frame geometry produced by the render pass.
//!
//! The renderer is pure with respect to presentation state: instead of
//! writing fitted rects back into [`crate::app::state::PresentationState`]
//! through interior-mutability sinks, every drawn frame fills one
//! [`FrameBindings`] value. The graphics syncer consumes it to place staged
//! images, and the run loop publishes it onto the state afterwards so
//! input-side queries (hyperlink hit-testing, reader pan bounds) see exactly
//! what the last drawn frame showed.

use kitty_pptx_display::CellRect;
use ratatui::layout::Rect;

use crate::app::reader_state::ReaderSlotPlacement;
use crate::widget::layout::{compute_slide_rect, rect_to_cell_rect, DEFAULT_CELL_ASPECT_RATIO};

/// What the render pass produced for one drawn frame: consumed by the
/// graphics syncer and published to state for input-side queries.
#[derive(Default, Debug, Clone)]
pub struct FrameBindings {
    /// Fitted rect of the full-screen current slide (from the base view).
    pub current: Option<CellRect>,
    /// Fitted rect of the presenter next-slide preview.
    pub preview: Option<CellRect>,
    /// Fitted rect of each visible sorter card (slide index → rect).
    pub thumbnails: Vec<(usize, CellRect)>,
    /// Visible reader panes (exact placement geometry the syncer stages).
    pub reader: Vec<ReaderSlotPlacement>,
    /// Effective reader column count (layout output).
    pub reader_columns: usize,
}

impl FrameBindings {
    /// Records the fitted rect a [`crate::widget::SlideWidget`] computes for
    /// `area` under the slide's aspect ratio, replicating the widget's fit
    /// exactly (default terminal cell aspect, zero-area guard).
    fn fitted_cell_rect(area: Rect, aspect_ratio: f32) -> Option<CellRect> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let fitted = compute_slide_rect(area, aspect_ratio, DEFAULT_CELL_ASPECT_RATIO);
        Some(rect_to_cell_rect(fitted))
    }

    /// Records the fitted rect of the full-screen current slide.
    pub fn record_current(&mut self, area: Rect, aspect_ratio: f32) {
        self.current = Self::fitted_cell_rect(area, aspect_ratio);
    }

    /// Records the fitted rect of the presenter next-slide preview.
    pub fn record_preview(&mut self, area: Rect, aspect_ratio: f32) {
        self.preview = Self::fitted_cell_rect(area, aspect_ratio);
    }

    /// Records a sorter card's fitted rect, replacing any earlier entry for
    /// the same slide (later entry wins, mirroring the previous sink).
    pub fn record_thumbnail(&mut self, slide_index: usize, area: Rect, aspect_ratio: f32) {
        if let Some(rect) = Self::fitted_cell_rect(area, aspect_ratio) {
            match self
                .thumbnails
                .iter_mut()
                .find(|(index, _)| *index == slide_index)
            {
                Some(entry) => entry.1 = rect,
                None => self.thumbnails.push((slide_index, rect)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn records_the_same_rect_the_widget_fits() {
        let mut bindings = FrameBindings::default();
        bindings.record_current(area(80, 24), 16.0 / 9.0);

        let expected = rect_to_cell_rect(compute_slide_rect(
            area(80, 24),
            16.0 / 9.0,
            DEFAULT_CELL_ASPECT_RATIO,
        ));
        assert_eq!(bindings.current, Some(expected));
        assert_eq!(bindings.preview, None);
    }

    #[test]
    fn zero_areas_record_nothing() {
        let mut bindings = FrameBindings::default();
        bindings.record_current(area(0, 24), 16.0 / 9.0);
        bindings.record_preview(area(80, 0), 16.0 / 9.0);
        bindings.record_thumbnail(3, area(0, 10), 16.0 / 9.0);
        assert_eq!(bindings.current, None);
        assert_eq!(bindings.preview, None);
        assert!(bindings.thumbnails.is_empty());
    }

    #[test]
    fn thumbnail_entries_are_replaced_not_duplicated() {
        let mut bindings = FrameBindings::default();
        bindings.record_thumbnail(4, area(30, 12), 16.0 / 9.0);
        bindings.record_thumbnail(4, area(20, 12), 16.0 / 9.0);
        bindings.record_thumbnail(5, area(30, 12), 16.0 / 9.0);

        assert_eq!(bindings.thumbnails.len(), 2);
        assert_eq!(bindings.thumbnails[0].0, 4);
        assert_eq!(
            bindings.thumbnails[0].1,
            rect_to_cell_rect(compute_slide_rect(
                area(20, 12),
                16.0 / 9.0,
                DEFAULT_CELL_ASPECT_RATIO
            ))
        );
        assert_eq!(bindings.thumbnails[1].0, 5);
    }
}
