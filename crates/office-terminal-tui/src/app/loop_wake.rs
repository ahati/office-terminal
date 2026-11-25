//! Deadline computation and redraw bookkeeping for the interactive loop.
//!
//! The presenter loop ([`super::PresentationApp::run`]) is deadline-driven:
//! each iteration computes when the next frame is actually required and only
//! draws when something changed. This module owns the two halves of that
//! decision:
//!
//! * [`next_wake`] — the earliest poll timeout at which a time-driven
//!   category (reader scroll animation, in-flight renders, displayed 1 s
//!   clocks, toast TTL) requires the loop to wake again. `None` means only
//!   input (or a resize, which also wakes `event::poll`) can change the
//!   screen, so the loop may block.
//! * [`RedrawTracker`] — latched comparisons (clock seconds, toast
//!   visibility, reader animation, worker count) that decide whether the
//!   current iteration needs a full draw pass. Latches refresh only on
//!   DRAWN frames, so nothing is lost across skipped iterations.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::app::state_notices::TOAST_TTL;

/// Reader scroll-smoothing tick: the displayed offset converges toward its
/// target at ~35% per frame, so 8 ms ticks keep the glide fluid.
pub(crate) const ANIMATION_TICK: Duration = Duration::from_millis(8);

/// Cap while pipeline workers are rendering: the completion channel is
/// drained on every wake, so finished rasters surface within one tick.
pub(crate) const RENDER_TICK: Duration = Duration::from_millis(16);

/// Upper bound for clock-driven wakes; the next second boundary is at most
/// 1 s away by construction, the cap documents the intent.
pub(crate) const CLOCK_TICK_CAP: Duration = Duration::from_millis(1000);

/// Fully-idle poll timeout. `event::poll` needs a concrete duration; one
/// hour is "indefinitely" in practice because any key, mouse move, focus
/// change or resize wakes it immediately.
pub(crate) const IDLE_POLL: Duration = Duration::from_secs(3600);

/// Whether a drawn frame would move the reader's smoothed scroll.
///
/// Superset of [`crate::app::reader_state::ReaderState::is_animating`]: a
/// sub-pixel residual (`is_animating` is false once the delta drops below
/// 1 px, but the displayed offset has not snapped onto its target yet)
/// still needs one 8 ms frame — `advance_display` only runs inside a drawn
/// reader view, so without that final tick the offset would stall just
/// short of its target. The blank screen and non-reader base views never
/// call `advance_display`, so they must not tick the loop either.
pub(crate) fn reader_animating(state: &PresentationState) -> bool {
    !state.is_blank()
        && state.base_mode() == AppMode::Reader
        && state
            .reader_state()
            .is_some_and(|reader| reader.is_animating() || reader.displayed_scroll_px() != reader.scroll_px as f32)
}

/// Whether the current frame displays a 1-second-granularity clock: the
/// presenter console shows the elapsed and wall clocks, and the fullscreen
/// status line shows the elapsed clock (unless hidden with `t` or while a
/// slide-jump digit buffer owns the line). Blank screens and other views
/// show none.
pub(crate) fn clock_displayed(state: &PresentationState) -> bool {
    if state.is_blank() {
        return false;
    }
    match state.base_mode() {
        AppMode::PresenterConsole => true,
        AppMode::Fullscreen => state.status_line_visible() && state.digit_buffer().is_empty(),
        _ => false,
    }
}

/// Seconds since the Unix epoch — the wall-clock display granularity.
fn wall_clock_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Time until the wall clock's displayed second flips (1..=1000 ms).
fn next_wall_second() -> Duration {
    let subsec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_millis())
        .unwrap_or(0);
    let remaining = u64::from(1000 - subsec.min(999));
    Duration::from_millis(remaining).min(CLOCK_TICK_CAP)
}

/// Time until the elapsed clock's displayed second flips (1..=1000 ms).
fn next_elapsed_second(state: &PresentationState, now: Instant) -> Duration {
    let subsec = now
        .checked_duration_since(state.session.start_time)
        .unwrap_or_default()
        .subsec_millis();
    let remaining = u64::from(1000 - subsec.min(999));
    Duration::from_millis(remaining).min(CLOCK_TICK_CAP)
}

/// The earlier of an optional deadline and a candidate.
fn shorter(current: Option<Duration>, candidate: Duration) -> Duration {
    current.map_or(candidate, |held| held.min(candidate))
}

/// Earliest wake the loop needs: the minimum over every applicable
/// time-driven deadline. `None` = only input can change the screen, so the
/// loop blocks on `event::poll`.
///
/// Categories:
/// * reader animation — 8 ms ticks while the smoothed scroll converges;
/// * in-flight renders — 16 ms cap so the completion drain (which runs on
///   every wake) picks finished rasters up promptly;
/// * displayed 1 s clocks — the next second boundary of whichever clocks
///   the active view actually shows;
/// * a live toast — its TTL expiry, when the status lines drop it.
pub(crate) fn next_wake(state: &PresentationState, in_flight: usize, now: Instant) -> Option<Duration> {
    let mut wake = None;

    if reader_animating(state) {
        wake = Some(ANIMATION_TICK);
    }

    if in_flight > 0 {
        wake = Some(shorter(wake, RENDER_TICK));
    }

    if clock_displayed(state) {
        // The presenter console shows the wall clock next to the elapsed
        // clock; both flip on (different) 1 s boundaries.
        if state.base_mode() == AppMode::PresenterConsole {
            wake = Some(shorter(wake, next_wall_second()));
        }
        wake = Some(shorter(wake, next_elapsed_second(state, now)));
    }

    if state.toast().is_some() {
        if let Some(shown_at) = state.session.toast_time {
            let age = now.checked_duration_since(shown_at).unwrap_or_default();
            wake = Some(shorter(wake, TOAST_TTL.saturating_sub(age)));
        }
    }

    wake
}

/// Latched dirty-checks for the deadline-driven loop.
///
/// [`RedrawTracker::evaluate`] runs once per iteration, before the draw
/// decision; it compares the current state against the values latched at
/// the last DRAWN frame. Input events, drained completions and
/// sync-surfaced diagnostics call [`RedrawTracker::mark_dirty`] directly.
pub(crate) struct RedrawTracker {
    needs_redraw: bool,
    last_clock_secs: u64,
    last_wall_secs: u64,
    last_toast_visible: bool,
    prev_in_flight: usize,
}

impl RedrawTracker {
    pub(crate) fn new() -> Self {
        Self {
            // The very first frame draws unconditionally.
            needs_redraw: true,
            // Sentinel seconds force a latch on the first drawn frame.
            last_clock_secs: u64::MAX,
            last_wall_secs: u64::MAX,
            last_toast_visible: false,
            prev_in_flight: 0,
        }
    }

    /// Evaluates every time-driven dirty category against the latches.
    pub(crate) fn evaluate(&mut self, state: &PresentationState, in_flight: usize) {
        // Reader smoothing produces a frame on every wake while it runs;
        // the final frame snaps the sub-pixel residual to its target.
        if reader_animating(state) {
            self.needs_redraw = true;
        }

        // 1 s clocks: redraw only when a displayed second flips. The wall
        // clock is only shown in the presenter console.
        if clock_displayed(state) {
            let elapsed_flipped = state.elapsed().as_secs() != self.last_clock_secs;
            let wall_flipped =
                state.base_mode() == AppMode::PresenterConsole && wall_clock_secs() != self.last_wall_secs;
            if elapsed_flipped || wall_flipped {
                self.needs_redraw = true;
            }
        }

        // Toast TTL: the status lines drop the toast without any input.
        let toast_visible = state.toast().is_some();
        if toast_visible != self.last_toast_visible {
            self.needs_redraw = true;
        }

        // A released worker slot means a render finished or failed since
        // the last iteration. Success signals through the completion
        // channel; failure only surfaces through `sync_graphics`, so force
        // a pass that lets it observe the outcome.
        if in_flight < self.prev_in_flight {
            self.needs_redraw = true;
        }
        self.prev_in_flight = in_flight;
    }

    /// Whether the current iteration should run a full draw pass.
    pub(crate) fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Requests a redraw (input events, drained completions, diagnostics
    /// surfaced during the graphics sync).
    pub(crate) fn mark_dirty(&mut self) {
        self.needs_redraw = true;
    }

    /// Latches the on-screen state after a drawn frame.
    pub(crate) fn mark_drawn(&mut self, state: &PresentationState) {
        self.needs_redraw = false;
        self.last_clock_secs = state.elapsed().as_secs();
        self.last_wall_secs = wall_clock_secs();
        self.last_toast_visible = state.toast().is_some();
    }
}

impl Default for RedrawTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "loop_wake_tests.rs"]
mod loop_wake_tests;
