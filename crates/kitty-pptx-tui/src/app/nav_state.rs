//! Viewer navigation policy: hidden-slide mode plus the zoom-factor contract.
//!
//! Kept separate from `PresentationState` so the pure index arithmetic and the
//! scale latch are testable without a deck, and so the state struct stays a
//! thin façade over its sub-states (SOLID: single responsibility).
//!
//! ref `packages/pptx/src/hidden.ts:6-29` (mode + dim defaults, the dim half is
//! not ported: the terminal presenter has no hidden overlay),
//! `packages/pptx/src/viewer.ts:437-575` (goToSlide / _step / setHiddenSlideMode /
//! visibleSlideCount) and `packages/pptx/src/viewer.ts:800-980` (setScale clamp).

use kitty_pptx_core::navigation::{
    count_visible, next_visible, resolve_visible, HiddenSlideMode,
};

/// Default zoom bounds (`setScale` clamps into this range).
// ref viewer-zoom-contract.test.ts:58-63 configures `[0.5, 2]`; the family-wide
// defaults are `[0.1, 4]` (`scroll-viewer-options.ts` zoomMin/zoomMax).
pub const DEFAULT_ZOOM_MIN: f32 = 0.1;
pub const DEFAULT_ZOOM_MAX: f32 = 4.0;

/// Hidden-slide mode + shown mask + zoom factor for one presenter session.
#[derive(Clone, Debug)]
pub struct NavState {
    hidden_mode: HiddenSlideMode,
    hidden: Vec<bool>,
    total: usize,
    scale: f32,
    zoom_min: f32,
    zoom_max: f32,
    scale_changes: u64,
}

impl Default for NavState {
    fn default() -> Self {
        Self {
            hidden_mode: HiddenSlideMode::Show,
            hidden: Vec::new(),
            total: 0,
            scale: 1.0,
            zoom_min: DEFAULT_ZOOM_MIN,
            zoom_max: DEFAULT_ZOOM_MAX,
            scale_changes: 0,
        }
    }
}

impl NavState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the deck size (slides without a model are shown).
    pub fn set_total(&mut self, total: usize) {
        self.total = total;
        self.hidden.resize(total, false);
    }

    /// Record one `<p:sld show="0">` flag per slide, in `sldIdLst` order.
    pub fn set_hidden(&mut self, hidden: Vec<bool>, total: usize) {
        self.total = total;
        self.hidden = hidden;
        self.hidden.resize(total, false);
    }

    pub fn is_hidden(&self, index: usize) -> bool {
        self.hidden.get(index).copied().unwrap_or(false)
    }

    pub fn hidden_mode(&self) -> HiddenSlideMode {
        self.hidden_mode
    }

    /// Switch the mode. Returns whether the mode actually changed; the caller
    /// re-resolves the current slide and clears search results on a crossing.
    // ref viewer.ts:545-575
    pub fn set_hidden_mode(&mut self, mode: HiddenSlideMode) -> bool {
        let changed = self.hidden_mode != mode;
        self.hidden_mode = mode;
        changed
    }

    /// Absolute jump target under the current policy. In `skip` mode a hidden
    /// target resolves forward, else backward; `show`/`dim` keep it addressable.
    // ref viewer.ts:437-462, packages/core/src/nav/visible-index.ts:21-29
    pub fn resolve(&self, index: usize) -> usize {
        if self.hidden_mode.is_skip() {
            resolve_visible(index, |i| self.is_hidden(i), self.total)
        } else {
            index
        }
    }

    /// Relative step from `from`. `None` means the index would not move (deck
    /// edge, or no shown slide left in that direction).
    // ref viewer.ts:466-478, 484-510
    pub fn step(&self, from: usize, direction: i8) -> Option<usize> {
        if self.total == 0 {
            return None;
        }
        let next = if self.hidden_mode.is_skip() && self.visible_count() > 0 {
            // An all-hidden deck degenerates to `show` (no slide to skip to).
            next_visible(from, direction, |i| self.is_hidden(i), self.total)
        } else if direction > 0 {
            (from + 1).min(self.total - 1)
        } else if direction < 0 {
            from.saturating_sub(1)
        } else {
            from
        };
        (next != from).then_some(next)
    }

    /// `visibleSlideCount`: shown slides only, absolute `total` unchanged.
    // ref viewer.ts:577-582
    pub fn visible_count(&self) -> usize {
        count_visible(|i| self.is_hidden(i), self.total)
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Number of effective scale changes (the `onScaleChange` observer seam).
    pub fn scale_changes(&self) -> u64 {
        self.scale_changes
    }

    pub fn zoom_bounds(&self) -> (f32, f32) {
        (self.zoom_min, self.zoom_max)
    }

    pub fn set_zoom_bounds(&mut self, min: f32, max: f32) {
        if min.is_finite() && max.is_finite() && min > 0.0 && min <= max {
            self.zoom_min = min;
            self.zoom_max = max;
            self.scale = self.scale.clamp(min, max);
        }
    }

    /// Clamp `scale` into `[zoom_min, zoom_max]` and record a change only when
    /// the effective factor moves, matching `setScale` + `onScaleChange`.
    // ref viewer-zoom-contract.test.ts:58-72
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if !scale.is_finite() {
            return false;
        }
        let clamped = scale.clamp(self.zoom_min, self.zoom_max);
        if clamped == self.scale {
            return false;
        }
        self.scale = clamped;
        self.scale_changes += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_step_walks_shown_slides_and_stops_at_the_edges() {
        let mut nav = NavState::new();
        nav.set_hidden(vec![false, true, true, false], 4);
        nav.set_hidden_mode(HiddenSlideMode::Skip);
        assert_eq!(nav.step(0, 1), Some(3));
        assert_eq!(nav.step(3, 1), None);
        assert_eq!(nav.step(3, -1), Some(0));
        assert_eq!(nav.resolve(1), 3);
        assert_eq!(nav.visible_count(), 2);
    }

    #[test]
    fn scale_changes_only_for_an_effective_factor() {
        let mut nav = NavState::new();
        nav.set_zoom_bounds(0.5, 2.0);
        assert!(nav.set_scale(10.0));
        assert_eq!(nav.scale(), 2.0);
        assert!(!nav.set_scale(2.0));
        assert_eq!(nav.scale_changes(), 1);
        assert!(nav.set_scale(0.01));
        assert_eq!(nav.scale(), 0.5);
    }
}
