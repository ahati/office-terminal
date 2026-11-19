//! Unit tests for the deadline-driven loop helpers ([`super::loop_wake`]).
//!
//! `next_wake` takes an injectable `now` and worker count so every category
//! is exercised against a plain [`PresentationState`]; wall-clock reads are
//! asserted by bounds (they cannot be frozen).

use super::*;
use crate::app::mode::BlankMode;
use std::time::Instant;

/// Fully idle presentation state: no clock line, no toast, no reader.
fn idle_state() -> PresentationState {
    let mut state = PresentationState::new(10, vec![]);
    state.toggle_status_line();
    state
}

/// Reader state whose smoothed scroll is converging toward `scroll_px`.
fn reader_state_with_target(scroll_px: i64) -> PresentationState {
    let mut state = PresentationState::new(10, vec![]);
    state.enable_reader_mode(4, vec![(612.0, 792.0)]);
    if let Some(reader) = state.reader_state_mut() {
        reader.scroll_px = scroll_px;
    }
    state
}

#[test]
fn idle_normal_mode_blocks_indefinitely() {
    // The status line is hidden: with no clock, toast, animation or in-flight
    // render, only input can change the screen.
    let state = idle_state();
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn fullscreen_status_line_clock_ticks_within_one_second() {
    // The fullscreen status line displays the elapsed clock, so the loop
    // wakes at the next second boundary even without input.
    let state = PresentationState::new(10, vec![]);
    let wake = next_wake(&state, 0, Instant::now()).expect("clock deadline");
    assert!(wake <= CLOCK_TICK_CAP, "got {wake:?}");
    assert!(wake > Duration::ZERO);
}

#[test]
fn digit_buffer_releases_the_status_line_clock() {
    // While a slide-jump digit buffer owns the status line, no clock is
    // displayed, so the loop may block.
    let mut state = PresentationState::new(10, vec![]);
    state.push_digit('5');
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn presenter_console_clock_ticks_within_one_second() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_mode(AppMode::PresenterConsole);
    let wake = next_wake(&state, 0, Instant::now()).expect("clock deadline");
    assert!(wake <= CLOCK_TICK_CAP, "got {wake:?}");
    assert!(wake > Duration::ZERO);
}

#[test]
fn sorter_idle_blocks_indefinitely() {
    let mut state = idle_state();
    state.set_mode(AppMode::Sorter);
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn reader_animation_ticks_at_animation_rate() {
    let state = reader_state_with_target(400);
    assert_eq!(
        next_wake(&state, 0, Instant::now()),
        Some(ANIMATION_TICK),
        "reader smoothing must tick at the animation rate"
    );
}

#[test]
fn converged_reader_blocks_indefinitely() {
    let mut state = idle_state();
    state.enable_reader_mode(4, vec![(612.0, 792.0)]);
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn subpixel_reader_residual_still_ticks() {
    // `is_animating()` goes false below 1 px of remaining delta, but the
    // displayed offset has not snapped onto its target yet: without a tick
    // the smoothing would stall just short of the target.
    let mut state = idle_state();
    state.enable_reader_mode(4, vec![(612.0, 792.0)]);
    if let Some(reader) = state.reader_state_mut() {
        reader.shift_displayed_scroll_px(0.5);
    }
    assert!(!state.reader_state().is_some_and(|r| r.is_animating()));
    assert_eq!(next_wake(&state, 0, Instant::now()), Some(ANIMATION_TICK));
}

#[test]
fn reader_animation_suppressed_when_blank() {
    // The blank screen renders no reader view, so `advance_display` cannot
    // run: ticking would spin the loop without moving anything.
    let mut state = reader_state_with_target(400);
    state.toggle_blank(BlankMode::Black);
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn in_flight_renders_wake_within_render_tick() {
    let state = idle_state();
    assert_eq!(next_wake(&state, 3, Instant::now()), Some(RENDER_TICK));
}

#[test]
fn animation_outranks_in_flight_renders() {
    let state = reader_state_with_target(400);
    assert_eq!(next_wake(&state, 2, Instant::now()), Some(ANIMATION_TICK));
}

#[test]
fn fresh_toast_wakes_at_its_ttl() {
    let mut state = idle_state();
    state.set_toast("buffering...");
    let wake = next_wake(&state, 0, Instant::now()).expect("toast deadline");
    // Fresh toast: nearly the whole TTL remains.
    assert!(wake <= TOAST_TTL, "got {wake:?}");
    assert!(wake > TOAST_TTL - Duration::from_secs(1), "got {wake:?}");
}

#[test]
fn expired_toast_imposes_no_deadline() {
    let mut state = idle_state();
    state.set_toast("buffering...");
    // Age the toast past its TTL: it is no longer displayed, so it must not
    // wake the loop either.
    state.toast_time = state
        .toast_time
        .and_then(|shown_at| shown_at.checked_sub(TOAST_TTL + Duration::from_secs(1)));
    assert_eq!(state.toast(), None);
    assert_eq!(next_wake(&state, 0, Instant::now()), None);
}

#[test]
fn tracker_starts_dirty_and_latches_after_a_frame() {
    let mut tracker = RedrawTracker::new();
    assert!(tracker.needs_redraw());

    let state = idle_state();
    tracker.evaluate(&state, 0);
    tracker.mark_drawn(&state);
    // A second pass over an unchanged state draws nothing.
    tracker.evaluate(&state, 0);
    assert!(!tracker.needs_redraw());
}

#[test]
fn tracker_marks_dirty_on_demand() {
    let mut tracker = RedrawTracker::new();
    let state = idle_state();
    tracker.evaluate(&state, 0);
    tracker.mark_drawn(&state);
    assert!(!tracker.needs_redraw());
    tracker.mark_dirty();
    assert!(tracker.needs_redraw());
}

#[test]
fn tracker_defaults_to_a_dirty_start() {
    let tracker = RedrawTracker::default();
    assert!(tracker.needs_redraw());
}

#[test]
fn tracker_requests_redraw_when_presenter_clock_flips() {
    let mut tracker = RedrawTracker::new();
    let mut state = PresentationState::new(10, vec![]);
    state.set_mode(AppMode::PresenterConsole);
    tracker.evaluate(&state, 0);
    tracker.mark_drawn(&state);
    assert!(!tracker.needs_redraw());

    // Two elapsed seconds pass with no input: the console clock must flip.
    if let Some(backdated) = state.start_time.checked_sub(Duration::from_secs(2)) {
        state.start_time = backdated;
    }
    tracker.evaluate(&state, 0);
    assert!(tracker.needs_redraw());
}

#[test]
fn tracker_requests_redraw_when_toast_appears_or_expires() {
    let mut tracker = RedrawTracker::new();
    let mut state = idle_state();

    tracker.evaluate(&state, 0);
    tracker.mark_drawn(&state);
    assert!(!tracker.needs_redraw());

    state.set_toast("hello");
    tracker.evaluate(&state, 0);
    assert!(tracker.needs_redraw(), "a new toast must be drawn");
    tracker.mark_drawn(&state);

    // The toast ages out: the status line drops it without any input.
    state.toast_time = state
        .toast_time
        .and_then(|shown_at| shown_at.checked_sub(TOAST_TTL + Duration::from_secs(1)));
    tracker.evaluate(&state, 0);
    assert!(tracker.needs_redraw(), "toast expiry must clear the line");
}

#[test]
fn tracker_requests_a_pass_when_a_worker_slot_is_released() {
    let mut tracker = RedrawTracker::new();
    let state = idle_state();

    // A rise (prefetch scheduled) is not by itself a redraw trigger.
    tracker.evaluate(&state, 2);
    tracker.mark_drawn(&state);
    assert!(!tracker.needs_redraw());

    // A release means a render finished or failed; failures only surface
    // through the graphics sync, so a pass must follow.
    tracker.evaluate(&state, 0);
    assert!(tracker.needs_redraw());
    tracker.mark_drawn(&state);
    tracker.evaluate(&state, 0);
    assert!(!tracker.needs_redraw());
}

#[test]
fn tracker_requests_frames_while_reader_animation_runs() {
    let mut tracker = RedrawTracker::new();
    let mut state = reader_state_with_target(400);

    tracker.evaluate(&state, 0);
    assert!(tracker.needs_redraw());
    tracker.mark_drawn(&state);

    // The drawn frame snapped the display onto its target: idle again.
    if let Some(reader) = state.reader_state_mut() {
        reader.scroll_px = 0;
    }
    tracker.evaluate(&state, 0);
    assert!(!tracker.needs_redraw());
}
