//! Transient UI notices: toast messages and render-failure surfacing.
//!
//! Split out of `state.rs` to keep the state module under the project's
//! 600-line ceiling; the fields still live on `PresentationState`.

use std::time::{Duration, Instant};

use office_terminal_core::{FidelityClass, WarningEvent};

use crate::app::state::PresentationState;

/// How long a toast stays on the status lines before ageing out.
/// Shared by [`PresentationState::toast`] and the event loop's wake logic
/// ([`crate::app::loop_wake`]) so expiry has a single source of truth.
pub(crate) const TOAST_TTL: Duration = Duration::from_secs(5);

impl PresentationState {
    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.session.toast_message = Some(msg.into());
        self.session.toast_time = Some(Instant::now());
    }

    pub fn toast(&self) -> Option<&str> {
        if let Some(time) = self.session.toast_time {
            if time.elapsed() < TOAST_TTL {
                return self.session.toast_message.as_deref();
            }
        }
        None
    }

    pub fn clear_toast(&mut self) {
        self.session.toast_message = None;
        self.session.toast_time = None;
    }

    /// Records a failed background render for `slide` and files one
    /// placeholder-class warning so the drawer explains the blank canvas.
    /// Idempotent per (slide, message) pair — the syncer calls this every
    /// frame while the failure stands.
    pub fn note_render_failure(&mut self, slide: usize, message: String) {
        let first_for_slide = !matches!(
            self.render_failure.as_ref(),
            Some((failed_slide, _)) if *failed_slide == slide
        );
        if self.render_failure.as_ref() == Some(&(slide, message.clone())) {
            return;
        }
        self.render_failure = Some((slide, message));
        if first_for_slide {
            self.warnings.add_warning(
                Some(slide),
                WarningEvent::FidelityDegraded {
                    element_id: format!("slide/{}", slide + 1),
                    feature: "render",
                    class: FidelityClass::Placeholder,
                },
            );
        }
    }

    /// Clears the failure record once `slide` renders successfully again.
    pub fn clear_render_failure_for(&mut self, slide: usize) {
        if self
            .render_failure
            .as_ref()
            .is_some_and(|(failed_slide, _)| *failed_slide == slide)
        {
            self.render_failure = None;
        }
    }

    /// The failed slide and message, if a background render failed.
    pub fn render_failure(&self) -> Option<(usize, &str)> {
        self.render_failure
            .as_ref()
            .map(|(slide, message)| (*slide, message.as_str()))
    }
}
