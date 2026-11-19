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
use crate::widget::layout::{rect_to_cell_rect, DEFAULT_CELL_ASPECT_RATIO};
use crate::widget::zoom_fit::{compute_zoomed_slide_fit, NormalizedCrop};

/// What the render pass produced for one drawn frame: consumed by the
/// graphics syncer and published to state for input-side queries.
#[derive(Default, Debug, Clone)]
pub struct FrameBindings {
    /// Zoom-aware placement of the full-screen current slide.
    pub current: Option<SlidePlacement>,
    /// Placement of the presenter next-slide preview (always the plain fit).
    pub preview: Option<SlidePlacement>,
    /// Fitted rect of each visible sorter card (slide index → rect).
    pub thumbnails: Vec<(usize, CellRect)>,
    /// Visible reader panes (exact placement geometry the syncer stages).
    pub reader: Vec<ReaderSlotPlacement>,
    /// Effective reader column count (layout output).
    pub reader_columns: usize,
}

/// One placed slide canvas: the cell rect the image occupies, plus the
/// normalized window of the slide that rect shows when a zoomed view
/// overflows the viewport.
///
/// Produced exclusively by [`FrameBindings::fitted_placement`], so the
/// widget that reserves the cells and the driver that places the image
/// always agree (both run the same `compute_zoomed_slide_fit`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlidePlacement {
    /// Cell rect the image is placed into (fully inside the view area).
    pub rect: CellRect,
    /// Visible window in normalized slide coordinates; `None` = whole slide.
    pub crop: Option<NormalizedCrop>,
    /// Zoom factor the placement was computed with.
    pub zoom: f32,
    /// Cell-space overflow of the zoomed rect per axis (pan bounds).
    pub overflow_cols: f32,
    pub overflow_rows: f32,
}

impl FrameBindings {
    /// Zoom-aware placement for `area`, replicating the [`SlideWidget`]'s fit
    /// exactly (default terminal cell aspect, zero-area guard).
    fn fitted_placement(
        area: Rect,
        aspect_ratio: f32,
        zoom: f32,
        pan: (f32, f32),
    ) -> Option<SlidePlacement> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let fit = compute_zoomed_slide_fit(
            area,
            aspect_ratio,
            DEFAULT_CELL_ASPECT_RATIO,
            zoom,
            pan.0,
            pan.1,
        );
        Some(SlidePlacement {
            rect: rect_to_cell_rect(fit.rect),
            crop: fit.crop,
            zoom,
            overflow_cols: fit.overflow_cols,
            overflow_rows: fit.overflow_rows,
        })
    }

    /// Records the zoom-aware placement of the full-screen current slide.
    pub fn record_current(
        &mut self,
        area: Rect,
        aspect_ratio: f32,
        zoom: f32,
        pan: (f32, f32),
    ) {
        self.current = Self::fitted_placement(area, aspect_ratio, zoom, pan);
    }

    /// Records the fitted placement of the presenter next-slide preview.
    /// The preview always shows the plain fit: the console's next slide is a
    /// preview, not a zoom target.
    pub fn record_preview(&mut self, area: Rect, aspect_ratio: f32) {
        self.preview = Self::fitted_placement(area, aspect_ratio, 1.0, (0.0, 0.0));
    }

    /// Records a sorter card's fitted rect, replacing any earlier entry for
    /// the same slide (later entry wins, mirroring the previous sink).
    pub fn record_thumbnail(&mut self, slide_index: usize, area: Rect, aspect_ratio: f32) {
        // Cards never zoom: the sorter target is aspect-derived and the fit is
        // the plain letterbox one.
        if let Some(placement) = Self::fitted_placement(area, aspect_ratio, 1.0, (0.0, 0.0)) {
            let rect = placement.rect;
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
    use crate::widget::layout::compute_slide_rect;

    fn area(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn records_the_same_rect_the_widget_fits() {
        let mut bindings = FrameBindings::default();
        bindings.record_current(area(80, 24), 16.0 / 9.0, 1.0, (0.0, 0.0));

        let expected_rect = rect_to_cell_rect(compute_slide_rect(
            area(80, 24),
            16.0 / 9.0,
            DEFAULT_CELL_ASPECT_RATIO,
        ));
        let placed = bindings.current.expect("current placement");
        assert_eq!(placed.rect, expected_rect);
        assert_eq!(placed.crop, None);
        assert_eq!(placed.zoom, 1.0);
        assert_eq!(bindings.preview, None);
    }

    #[test]
    fn zoomed_current_records_a_crop_and_pan_bounds() {
        let mut bindings = FrameBindings::default();
        bindings.record_current(area(80, 24), 16.0 / 9.0, 2.0, (10.0, 0.0));

        let placed = bindings.current.expect("zoomed placement");
        assert_eq!(placed.rect, rect_to_cell_rect(area(80, 24)));
        assert_eq!(placed.zoom, 2.0);
        assert!(placed.overflow_cols > 0.0);
        assert!(placed.overflow_rows > 0.0);
        let crop = placed.crop.expect("overflow requires a crop");
        assert!((crop.w - 0.5).abs() < 1e-4, "half the slide width visible");
        assert!(crop.x > 0.25, "pan right of center shifts the window right");
    }

    #[test]
    fn preview_never_zooms() {
        let mut bindings = FrameBindings::default();
        bindings.record_preview(area(40, 20), 16.0 / 9.0);

        let placed = bindings.preview.expect("preview placement");
        assert_eq!(placed.zoom, 1.0);
        assert_eq!(placed.crop, None);
    }

    #[test]
    fn zero_areas_record_nothing() {
        let mut bindings = FrameBindings::default();
        bindings.record_current(area(0, 24), 16.0 / 9.0, 2.0, (0.0, 0.0));
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
