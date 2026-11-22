//! Spatial view state for slide-mode zoom: pan offsets and their bounds.
//!
//! The zoom *factor* is a navigation contract on [`NavState`]
//! (`crates/office-terminal-tui/src/app/nav_state.rs`); pan is its spatial half.
//! Pan offsets are signed cell-space distances from the centered position
//! and are clamped against the overflow the last drawn frame published —
//! the same publish-then-query discipline the reader uses for its pan
//! bounds, so input can never push content past what the geometry function
//! would clamp anyway.
//!
//! The factor and the offsets meet in exactly one place:
//! [`crate::widget::zoom_fit::compute_zoomed_slide_fit`], which both the
//! widget and the bindings run.

use crate::app::state::PresentationState;

/// A zoom factor at or below this is the unzoomed view.
const ZOOM_EPS: f32 = 1e-3;

impl PresentationState {
    /// Current pan offsets in cell units (cols, rows), from the centered
    /// position. Already clamped against the last drawn frame's overflow.
    pub fn zoom_pan(&self) -> (f32, f32) {
        self.zoom_pan
    }

    /// Whether the current-slide view is zoomed past the plain fit (and
    /// therefore pans instead of navigating on the arrow keys).
    pub fn is_zoomed(&self) -> bool {
        self.scale() > 1.0 + ZOOM_EPS
    }

    /// Pan the zoomed view by `(dx, dy)` cell units, clamped to the overflow
    /// the last drawn frame published. Returns whether the view moved.
    ///
    /// A no-op when the view is not zoomed (pan is meaningless without
    /// overflow) or when no frame has been drawn yet (no bounds to clamp
    /// against — the first frame places the centered fit anyway).
    pub fn pan_zoom(&mut self, dx: f32, dy: f32) -> bool {
        if !self.is_zoomed() {
            return false;
        }
        let (overflow_cols, overflow_rows) = self.zoom_overflow();
        let (x, y) = self.zoom_pan;
        let new_x = (x + dx).clamp(-overflow_cols / 2.0, overflow_cols / 2.0);
        let new_y = (y + dy).clamp(-overflow_rows / 2.0, overflow_rows / 2.0);
        let moved = (new_x != x) || (new_y != y);
        self.zoom_pan = (new_x, new_y);
        moved
    }

    /// Re-center the zoomed view (pan offsets back to the centered start).
    pub fn reset_zoom_pan(&mut self) {
        self.zoom_pan = (0.0, 0.0);
    }

    /// Cell-space pan bounds from the placement the last drawn frame
    /// published: `(overflow_cols, overflow_rows)`, `(0, 0)` when no frame
    /// has been drawn or the view fits without overflow.
    fn zoom_overflow(&self) -> (f32, f32) {
        self.current_placement
            .map(|placement| (placement.overflow_cols, placement.overflow_rows))
            .unwrap_or((0.0, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::bindings::SlidePlacement;
    use crate::widget::zoom_fit::compute_zoomed_slide_fit;
    use office_terminal_display::CellRect;
    use ratatui::layout::Rect;

    fn placement_with_overflow(cols: f32, rows: f32) -> SlidePlacement {
        let fit = compute_zoomed_slide_fit(Rect::new(0, 0, 80, 24), 16.0 / 9.0, 0.5, 2.0, 0.0, 0.0);
        SlidePlacement {
            rect: CellRect {
                row: 0,
                col: 0,
                rows: 24,
                cols: 80,
            },
            crop: fit.crop,
            zoom: 2.0,
            overflow_cols: cols,
            overflow_rows: rows,
        }
    }

    #[test]
    fn pan_is_inert_until_zoomed() {
        let mut state = PresentationState::new(1, vec![None]);
        assert!(!state.is_zoomed());
        assert!(!state.pan_zoom(10.0, 0.0));
        assert_eq!(state.zoom_pan(), (0.0, 0.0));
    }

    #[test]
    fn pan_moves_and_clamps_to_the_published_overflow() {
        let mut state = PresentationState::new(1, vec![None]);
        state.set_scale(2.0);
        state.publish_frame_bindings(crate::app::bindings::FrameBindings {
            current: Some(placement_with_overflow(80.0, 22.0)),
            ..Default::default()
        });

        assert!(state.pan_zoom(10.0, 0.0));
        assert_eq!(state.zoom_pan(), (10.0, 0.0));

        // Way past the bound pins at +overflow/2 (centered start).
        assert!(state.pan_zoom(500.0, 0.0));
        assert_eq!(state.zoom_pan(), (40.0, 0.0));

        // Vertical pan clamps at -overflow/2.
        assert!(state.pan_zoom(0.0, -500.0));
        assert_eq!(state.zoom_pan(), (40.0, -11.0));
    }

    #[test]
    fn reset_recenters_the_view() {
        let mut state = PresentationState::new(1, vec![None]);
        state.set_scale(2.0);
        state.publish_frame_bindings(crate::app::bindings::FrameBindings {
            current: Some(placement_with_overflow(80.0, 22.0)),
            ..Default::default()
        });
        state.pan_zoom(40.0, -11.0);
        state.reset_zoom_pan();
        assert_eq!(state.zoom_pan(), (0.0, 0.0));
    }
}
