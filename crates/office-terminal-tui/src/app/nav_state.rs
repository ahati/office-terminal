//! Viewer navigation policy: hidden-slide mode, the zoom-factor contract,
//! the slide-jump entry points (digit buffer, first/last/jump) and the
//! terminal-grid setter consumed by mouse hit-testing.
//!
//! Kept separate from `PresentationState` so the pure index arithmetic and the
//! scale latch are testable without a deck, and so the state struct stays a
//! thin façade over its sub-states (SOLID: single responsibility).
//!
//! ref `packages/pptx/src/hidden.ts:6-29` (mode + dim defaults, the dim half is
//! not ported: the terminal presenter has no hidden overlay),
//! `packages/pptx/src/viewer.ts:437-575` (goToSlide / _step / setHiddenSlideMode /
//! visibleSlideCount) and `packages/pptx/src/viewer.ts:800-980` (setScale clamp).

use office_terminal_core::navigation::{count_visible, next_visible, resolve_visible, HiddenSlideMode};
use office_terminal_core::EMU_PER_PX;

use super::state::PresentationState;

/// Default zoom bounds (`setScale` clamps into this range).
// ref viewer-zoom-contract.test.ts:58-63 configures `[0.5, 2]`; the family-wide
// defaults are `[0.1, 4]` (`scroll-viewer-options.ts` zoomMin/zoomMax).
pub const DEFAULT_ZOOM_MIN: f32 = 0.1;
pub const DEFAULT_ZOOM_MAX: f32 = 4.0;

/// Discrete stepper rungs (factor, 0.25 = 25 %). Superset of the Excel /
/// Acrobat status-bar presets; ref `core/src/interaction/zoomable.ts:99-107`.
pub const ZOOM_STEP_LADDER: &[f32] = &[
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0,
];

/// Small epsilon so a value sitting on a rung steps to the next one.
const LADDER_EPS: f32 = 0.005;

/// Next ladder rung strictly greater than `scale` (top rung at the ceiling).
pub fn next_zoom_step(scale: f32) -> f32 {
    ZOOM_STEP_LADDER
        .iter()
        .copied()
        .find(|rung| *rung > scale + LADDER_EPS)
        .unwrap_or(ZOOM_STEP_LADDER[ZOOM_STEP_LADDER.len() - 1])
}

/// Next ladder rung strictly less than `scale`, or `floor` below the ladder.
pub fn prev_zoom_step(scale: f32, floor: f32) -> f32 {
    ZOOM_STEP_LADDER
        .iter()
        .rev()
        .copied()
        .find(|rung| *rung < scale - LADDER_EPS)
        .unwrap_or(floor.min(ZOOM_STEP_LADDER[0]))
}

/// Fit target used by `fit_width` / `fit_page`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitTarget {
    Width,
    Page,
}

/// Scale that fits `content` into `container`; 0 when the container or content
/// is unlaid-out. Ref `fitScale` (`zoomable.ts:177-185`).
pub fn fit_scale(
    content_width: f32,
    content_height: f32,
    container_width: f32,
    container_height: f32,
    target: FitTarget,
) -> f32 {
    if content_width <= 0.0 || container_width <= 0.0 {
        return 0.0;
    }
    let width_fit = container_width / content_width;
    if target == FitTarget::Width {
        return width_fit;
    }
    if content_height <= 0.0 || container_height <= 0.0 {
        return 0.0;
    }
    width_fit.min(container_height / content_height)
}

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

    /// Step to the next ladder rung and apply it.
    pub fn zoom_in(&mut self) -> bool {
        self.set_scale(next_zoom_step(self.scale))
    }

    /// Step to the previous ladder rung and apply it.
    pub fn zoom_out(&mut self) -> bool {
        self.set_scale(prev_zoom_step(self.scale, self.zoom_min))
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

impl PresentationState {
    pub fn scale(&self) -> f32 {
        self.nav.scale()
    }

    /// Clamp and apply a zoom factor; `true` only when the factor moved (the
    /// `onScaleChange` gate). ref viewer-zoom-contract.test.ts:58-72
    pub fn set_scale(&mut self, scale: f32) -> bool {
        self.nav.set_scale(scale)
    }

    /// Count of effective scale changes (the observer seam).
    pub fn scale_changes(&self) -> u64 {
        self.nav.scale_changes()
    }

    pub fn zoom_bounds(&self) -> (f32, f32) {
        self.nav.zoom_bounds()
    }

    pub fn set_zoom_bounds(&mut self, min: f32, max: f32) {
        self.nav.set_zoom_bounds(min, max);
    }

    /// Fit the slide width into `container_width` (status/terminal columns).
    /// Returns whether the factor changed; 0/false when unlaid-out.
    pub fn fit_width(&mut self, container_width: f32) -> bool {
        let (width_emu, height_emu) = self.slide_dimensions();
        let scale = fit_scale(
            width_emu as f32 / EMU_PER_PX,
            height_emu as f32 / EMU_PER_PX,
            container_width,
            0.0,
            FitTarget::Width,
        );
        if scale > 0.0 {
            self.set_scale(scale)
        } else {
            false
        }
    }

    /// Fit the whole slide into the container (tighter of width/height).
    pub fn fit_page(&mut self, container_width: f32, container_height: f32) -> bool {
        let (width_emu, height_emu) = self.slide_dimensions();
        let scale = fit_scale(
            width_emu as f32 / EMU_PER_PX,
            height_emu as f32 / EMU_PER_PX,
            container_width,
            container_height,
            FitTarget::Page,
        );
        if scale > 0.0 {
            self.set_scale(scale)
        } else {
            false
        }
    }

    // ─── Hidden-slide policy (ref hidden.ts, viewer.ts:437-582) ─────────

    pub fn hidden_slide_mode(&self) -> HiddenSlideMode {
        self.nav.hidden_mode()
    }

    /// Switch the hidden-slide mode. Returns whether the current slide moved.
    /// Entering or leaving `skip` clears the active search match (`find` may
    /// only match shown slides in `skip` mode). ref viewer.ts:545-575
    pub fn set_hidden_slide_mode(&mut self, mode: HiddenSlideMode) -> bool {
        let previous = self.nav.hidden_mode();
        if !self.nav.set_hidden_mode(mode) {
            return false;
        }
        if previous.is_skip() || mode.is_skip() {
            self.sync_search_entries();
            self.search.clear_matches();
            self.sync_find_index();
        }
        let target = if self.total_slides == 0 {
            0
        } else {
            self.nav.resolve(self.current_slide.min(self.total_slides - 1))
        };
        self.navigate_to(target)
    }

    /// `<p:sld show="0">` for an absolute slide index.
    pub fn is_slide_hidden(&self, index: usize) -> bool {
        self.nav.is_hidden(index)
    }

    /// Shown slides only; the absolute slide count is unchanged.
    // ref viewer.ts:577-582
    pub fn visible_slide_count(&self) -> usize {
        self.nav.visible_count()
    }

    // ─── Slide-jump entry points ────────────────────────────────────────

    /// Jump to the first slide (slide 0, or the first shown slide under `skip`).
    pub fn first_slide(&mut self) -> bool {
        self.set_current_slide(0)
    }

    /// Jump to the last slide (or the nearest shown slide under `skip`).
    pub fn last_slide(&mut self) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        self.set_current_slide(self.total_slides - 1)
    }

    /// Jump to a 1-based slide index.
    pub fn jump_to_slide(&mut self, slide_1based: usize) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        self.set_current_slide(slide_1based.saturating_sub(1))
    }

    /// Push a digit for slide jumping.
    pub fn push_digit(&mut self, ch: char) {
        if ch.is_ascii_digit() && self.digit_buffer.len() < 5 {
            self.digit_buffer.push(ch);
        }
    }

    /// Commit accumulated digits into slide jump.
    pub fn commit_digits(&mut self) -> bool {
        if self.digit_buffer.is_empty() {
            return false;
        }
        let parsed = self.digit_buffer.parse::<usize>().ok();
        self.digit_buffer.clear();
        if let Some(target_1based) = parsed {
            self.jump_to_slide(target_1based)
        } else {
            false
        }
    }

    /// Clear accumulated digit buffer.
    pub fn clear_digits(&mut self) {
        self.digit_buffer.clear();
    }

    /// Last terminal grid recorded by the layout pass.
    pub fn layout_size(&self) -> (u16, u16) {
        (self.layout_cols, self.layout_rows)
    }

    /// Record the latest terminal cell grid for mouse hit-testing.
    pub fn set_layout_dimensions(&mut self, cols: u16, rows: u16) {
        self.layout_cols = cols;
        self.layout_rows = rows;
    }

    /// Navigation policy for zoom/step callers (events, app).
    pub fn nav_mut(&mut self) -> &mut NavState {
        &mut self.nav
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

    #[test]
    fn zoom_steppers_walk_the_shared_ladder() {
        let mut nav = NavState::new();
        assert!(nav.zoom_in());
        assert_eq!(nav.scale(), 1.1);
        assert!(nav.zoom_in());
        assert_eq!(nav.scale(), 1.25);
        assert!(nav.zoom_out());
        assert_eq!(nav.scale(), 1.1);
        assert_eq!(next_zoom_step(0.9), 1.0);
        assert_eq!(prev_zoom_step(1.0, 0.1), 0.9);
        assert_eq!(next_zoom_step(4.0), 4.0);
        assert_eq!(prev_zoom_step(0.25, 0.1), 0.1);
    }

    #[test]
    fn fit_scale_takes_the_tighter_page_fit() {
        assert_eq!(fit_scale(960.0, 720.0, 480.0, 10_000.0, FitTarget::Width), 0.5);
        assert_eq!(fit_scale(960.0, 720.0, 960.0, 360.0, FitTarget::Page), 0.5);
        assert_eq!(fit_scale(960.0, 720.0, 0.0, 720.0, FitTarget::Width), 0.0);
        assert_eq!(fit_scale(0.0, 720.0, 960.0, 720.0, FitTarget::Page), 0.0);
    }
}
