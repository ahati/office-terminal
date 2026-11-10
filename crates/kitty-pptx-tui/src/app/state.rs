use std::time::{Duration, Instant, SystemTime};
use kitty_pptx_display::ImageId;

/// Application display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    /// Fullscreen slide view with minimal status line (or hidden until toggled).
    Fullscreen,
    /// Split-pane presenter console (current slide, next preview, notes, timers, gauge).
    PresenterConsole,
    /// Help popup modal showing keyboard shortcuts.
    HelpModal,
}

/// Blank screen mode for pausing talks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlankMode {
    None,
    Black,
    White,
}

/// Presentation navigation and UI state machine.
pub struct PresentationState {
    current_slide: usize,
    total_slides: usize,
    mode: AppMode,
    previous_mode: AppMode,
    blank_mode: BlankMode,
    status_line_visible: bool,
    notes: Vec<Option<String>>,
    start_time: Instant,
    digit_buffer: String,
    aspect_ratio: f32,
}

impl PresentationState {
    pub fn new(total_slides: usize, notes: Vec<Option<String>>) -> Self {
        Self {
            current_slide: 0,
            total_slides,
            mode: AppMode::Fullscreen,
            previous_mode: AppMode::Fullscreen,
            blank_mode: BlankMode::None,
            status_line_visible: true,
            notes,
            start_time: Instant::now(),
            digit_buffer: String::new(),
            aspect_ratio: 16.0 / 9.0,
        }
    }

    pub fn current_slide(&self) -> usize {
        self.current_slide
    }

    pub fn total_slides(&self) -> usize {
        self.total_slides
    }

    pub fn mode(&self) -> AppMode {
        self.mode
    }

    pub fn blank_mode(&self) -> BlankMode {
        self.blank_mode
    }

    pub fn is_blank(&self) -> bool {
        self.blank_mode != BlankMode::None
    }

    pub fn status_line_visible(&self) -> bool {
        self.status_line_visible
    }

    pub fn digit_buffer(&self) -> &str {
        &self.digit_buffer
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    pub fn set_aspect_ratio(&mut self, ratio: f32) {
        if ratio.is_finite() && ratio > 0.0 {
            self.aspect_ratio = ratio;
        }
    }

    pub fn set_current_slide(&mut self, slide_0based: usize) -> bool {
        if self.total_slides == 0 {
            self.current_slide = 0;
            return false;
        }
        let clamped = slide_0based.min(self.total_slides.saturating_sub(1));
        let changed = self.current_slide != clamped;
        self.current_slide = clamped;
        changed
    }

    /// Advance to the next slide, clamping to the last slide.
    pub fn next_slide(&mut self) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        if self.current_slide + 1 < self.total_slides {
            self.current_slide += 1;
            true
        } else {
            false
        }
    }

    /// Move to the previous slide, clamping to slide 0.
    pub fn prev_slide(&mut self) -> bool {
        if self.current_slide > 0 {
            self.current_slide -= 1;
            true
        } else {
            false
        }
    }

    /// Jump to the first slide (slide 0).
    pub fn first_slide(&mut self) -> bool {
        let changed = self.current_slide != 0;
        self.current_slide = 0;
        changed
    }

    /// Jump to the last slide (`total_slides - 1`).
    pub fn last_slide(&mut self) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        let last = self.total_slides.saturating_sub(1);
        let changed = self.current_slide != last;
        self.current_slide = last;
        changed
    }

    /// Jump to a 1-based slide index.
    pub fn jump_to_slide(&mut self, slide_1based: usize) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        let target_0based = slide_1based.saturating_sub(1).min(self.total_slides - 1);
        let changed = self.current_slide != target_0based;
        self.current_slide = target_0based;
        changed
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

    /// Toggle split-pane presenter mode ('p').
    pub fn toggle_presenter_mode(&mut self) {
        self.unblank();
        self.mode = match self.mode {
            AppMode::PresenterConsole => AppMode::Fullscreen,
            _ => AppMode::PresenterConsole,
        };
    }

    /// Set mode directly.
    pub fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
    }

    /// Toggle help modal ('?' or 'h').
    pub fn toggle_help(&mut self) {
        if self.mode == AppMode::HelpModal {
            self.mode = self.previous_mode;
        } else {
            self.previous_mode = self.mode;
            self.mode = AppMode::HelpModal;
        }
    }

    /// Close help modal if open.
    pub fn close_help(&mut self) {
        if self.mode == AppMode::HelpModal {
            self.mode = self.previous_mode;
        }
    }

    /// Toggle blank screen mode ('b' for Black, 'w' for White).
    pub fn toggle_blank(&mut self, target: BlankMode) {
        if self.blank_mode == target {
            self.blank_mode = BlankMode::None;
        } else {
            self.blank_mode = target;
        }
    }

    /// Exit blank screen mode.
    pub fn unblank(&mut self) {
        self.blank_mode = BlankMode::None;
    }

    /// Toggle status line visibility ('t').
    pub fn toggle_status_line(&mut self) {
        self.status_line_visible = !self.status_line_visible;
    }

    /// Speaker notes for current slide.
    pub fn current_notes(&self) -> Option<&str> {
        self.notes.get(self.current_slide)
            .and_then(|opt| opt.as_deref())
    }

    /// Next slide index if available.
    pub fn next_slide_index(&self) -> Option<usize> {
        if self.current_slide + 1 < self.total_slides {
            Some(self.current_slide + 1)
        } else {
            None
        }
    }

    /// ImageId for the current slide.
    pub fn current_image_id(&self) -> ImageId {
        ImageId((self.current_slide % 8 + 1) as u32)
    }

    /// ImageId for the next slide preview.
    pub fn next_image_id(&self) -> Option<ImageId> {
        self.next_slide_index()
            .map(|idx| ImageId((idx % 8 + 1) as u32))
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_clamping() {
        let notes = vec![Some("Note 1".into()), None, Some("Note 3".into())];
        let mut state = PresentationState::new(3, notes);

        assert_eq!(state.current_slide(), 0);
        assert!(!state.prev_slide());
        assert_eq!(state.current_slide(), 0);

        assert!(state.next_slide());
        assert_eq!(state.current_slide(), 1);

        assert!(state.next_slide());
        assert_eq!(state.current_slide(), 2);

        // Clamps at last slide
        assert!(!state.next_slide());
        assert_eq!(state.current_slide(), 2);

        // Jump to first
        assert!(state.first_slide());
        assert_eq!(state.current_slide(), 0);

        // Jump to last
        assert!(state.last_slide());
        assert_eq!(state.current_slide(), 2);
    }

    #[test]
    fn test_digit_jump() {
        let mut state = PresentationState::new(10, vec![]);
        state.push_digit('5');
        assert_eq!(state.digit_buffer(), "5");
        assert!(state.commit_digits());
        assert_eq!(state.current_slide(), 4); // slide 5 is 0-indexed 4
        assert_eq!(state.digit_buffer(), "");

        // Jump beyond max clamps to total - 1
        state.push_digit('9');
        state.push_digit('9');
        assert!(state.commit_digits());
        assert_eq!(state.current_slide(), 9);
    }

    #[test]
    fn test_mode_toggles() {
        let mut state = PresentationState::new(5, vec![]);
        assert_eq!(state.mode(), AppMode::Fullscreen);

        state.toggle_presenter_mode();
        assert_eq!(state.mode(), AppMode::PresenterConsole);

        state.toggle_presenter_mode();
        assert_eq!(state.mode(), AppMode::Fullscreen);

        state.toggle_help();
        assert_eq!(state.mode(), AppMode::HelpModal);

        state.toggle_help();
        assert_eq!(state.mode(), AppMode::Fullscreen);
    }

    #[test]
    fn test_blank_screen_toggle() {
        let mut state = PresentationState::new(5, vec![]);
        assert_eq!(state.blank_mode(), BlankMode::None);

        state.toggle_blank(BlankMode::Black);
        assert_eq!(state.blank_mode(), BlankMode::Black);
        assert!(state.is_blank());

        state.toggle_blank(BlankMode::Black);
        assert_eq!(state.blank_mode(), BlankMode::None);

        state.toggle_blank(BlankMode::White);
        assert_eq!(state.blank_mode(), BlankMode::White);

        state.unblank();
        assert_eq!(state.blank_mode(), BlankMode::None);
    }
}
