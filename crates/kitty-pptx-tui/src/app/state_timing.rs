//! Presentation clock, wall-clock and progress readouts.
//!
//! Split out of [`super::state`] to keep that state machine under the file
//! limit; every field read here is `pub(crate)` on `PresentationState`.

use std::time::{Duration, SystemTime};

use super::state::PresentationState;

impl PresentationState {
    /// Elapsed time since presentation start.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Formatted presentation elapsed time (`HH:MM:SS`).
    pub fn elapsed_formatted(&self) -> String {
        let elapsed_secs = self.elapsed().as_secs();
        let hours = elapsed_secs / 3600;
        let mins = (elapsed_secs % 3600) / 60;
        let secs = elapsed_secs % 60;
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }

    /// Wall clock time in `HH:MM:SS UTC`.
    pub fn wall_clock_formatted(&self) -> String {
        let total_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let secs_in_day = total_secs % 86400;
        let hours = secs_in_day / 3600;
        let mins = (secs_in_day % 3600) / 60;
        let secs = secs_in_day % 60;
        format!("{:02}:{:02}:{:02} UTC", hours, mins, secs)
    }

    /// Presentation progress as a fraction between 0.0 and 1.0.
    pub fn progress_ratio(&self) -> f64 {
        if self.total_slides == 0 {
            0.0
        } else {
            ((self.current_slide + 1) as f64 / self.total_slides as f64).clamp(0.0, 1.0)
        }
    }
}
