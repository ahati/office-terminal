//! Smoothed scroll display for [`ReaderState`].
//!
//! The scroll offset shown on screen lives behind an `Arc<AtomicU32>` (f32
//! bits) so the immutable render path can advance it once per frame; it
//! converges exponentially toward the `scroll_px` target that event-side
//! navigation writes. Discrete jumps (layout toggles, row/page jumps) snap
//! instead of gliding.

use std::sync::atomic::Ordering;

use crate::app::reader_state::ReaderState;

impl ReaderState {
    /// Scroll offset currently shown on screen. The smoothing target is
    /// [`ReaderState::scroll_px`].
    pub fn displayed_scroll_px(&self) -> f32 {
        f32::from_bits(self.displayed_scroll_px.load(Ordering::Relaxed))
    }

    fn set_displayed_scroll_px(&self, value: f32) {
        self.displayed_scroll_px
            .store(value.to_bits(), Ordering::Relaxed);
    }

    /// Shifts the displayed scroll by the same amount the target wrapped when
    /// the anchor advanced, keeping the on-screen content position continuous
    /// across row transitions (see `reader_scroll`).
    pub(crate) fn shift_displayed_scroll_px(&self, delta_px: f32) {
        self.set_displayed_scroll_px(self.displayed_scroll_px() + delta_px);
    }

    /// Snaps the displayed scroll onto its target (discrete navigation:
    /// layout toggles, row jumps and page jumps must not glide).
    pub(crate) fn snap_display(&mut self) {
        self.set_displayed_scroll_px(self.scroll_px as f32);
    }

    /// Advances the displayed scroll toward its target with an exponential
    /// approach (~35% of the remaining distance per frame, snapping below
    /// 1 px). Called once per frame by the reader render path; the value
    /// lives behind an atomic because the renderer only sees `&self`.
    /// Returns whether the displayed value moved this frame.
    pub fn advance_display(&self) -> bool {
        let target = self.scroll_px as f32;
        let displayed = self.displayed_scroll_px();
        let delta = target - displayed;
        if delta.abs() < 1.0 {
            if displayed == target {
                return false;
            }
            self.set_displayed_scroll_px(target);
            return true;
        }
        self.set_displayed_scroll_px(displayed + delta * 0.35);
        true
    }

    /// Whether the displayed scroll is still converging toward its target
    /// (the event loop should keep ticking while this is true).
    pub fn is_animating(&self) -> bool {
        (self.displayed_scroll_px() - self.scroll_px as f32).abs() >= 1.0
    }
}
