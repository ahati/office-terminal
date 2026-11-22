//! Session ephemera — the presentation clock and the transient toast.
//!
//! Strictly per-session state that carries no deck data and no view
//! geometry: when the talk started and what (if anything) the status line
//! is currently toasting. [`super::state_notices`] and
//! [`super::state_timing`] keep their `PresentationState` method blocks and
//! read these fields through `state.session`.

use std::time::Instant;

/// Per-session ephemeral state for one presentation run.
#[derive(Clone, Debug)]
pub struct SessionState {
    /// Wall-clock instant the presentation started (elapsed clock source).
    pub(crate) start_time: Instant,
    /// Toast text currently on the status lines, if any.
    pub(crate) toast_message: Option<String>,
    /// When the current toast was shown (TTL ageing, see `state_notices`).
    pub(crate) toast_time: Option<Instant>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            toast_message: None,
            toast_time: None,
        }
    }
}

impl SessionState {
    /// A fresh session: clock starts now, no toast showing.
    pub fn new() -> Self {
        Self::default()
    }
}
