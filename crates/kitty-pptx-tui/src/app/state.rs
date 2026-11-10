use std::time::{Duration, Instant, SystemTime};
use kitty_pptx_display::ImageId;
use pptx_model::Slide;
use crate::app::sorter_state::SorterState;

/// Application display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    /// Fullscreen slide view with minimal status line (or hidden until toggled).
    Fullscreen,
    /// Split-pane presenter console (current slide, next preview, notes, timers, gauge).
    PresenterConsole,
    /// Help popup modal showing keyboard shortcuts.
    HelpModal,
    /// Thumbnail slide sorter grid mode.
    Sorter,
}

/// Backwards-compatible alias for `AppMode`.
pub type PresenterMode = AppMode;

#[allow(non_upper_case_globals)]
impl AppMode {
    pub const Normal: AppMode = AppMode::Fullscreen;
    pub const ThumbnailGrid: AppMode = AppMode::Sorter;
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

    // Sorter grid state
    sorter: SorterState,

    // Slide models & media
    slides: Vec<Slide>,

    // Notification toast
    toast_message: Option<String>,
    toast_time: Option<Instant>,
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
            sorter: SorterState::new(),
            slides: Vec::new(),
            toast_message: None,
            toast_time: None,
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

    /// Return the previous app mode before entering overlay or modal.
    pub fn previous_mode(&self) -> AppMode {
        self.previous_mode
    }

    /// Return the currently highlighted slide index in sorter mode.
    pub fn selected_sorter_index(&self) -> usize {
        self.sorter.selected_index()
    }

    /// Set the highlighted slide index in sorter mode, clamping to valid slide bounds.
    pub fn set_selected_sorter_index(&mut self, index: usize) -> bool {
        self.sorter.set_selected_index(index, self.total_slides)
    }

    /// Current number of columns in the thumbnail sorter grid.
    pub fn sorter_cols(&self) -> usize {
        self.sorter.cols()
    }

    /// Set the number of columns in the thumbnail sorter grid.
    pub fn set_sorter_cols(&mut self, cols: usize) {
        self.sorter.set_cols(cols);
    }

    /// Whether cursor navigation wraps around grid boundaries.
    pub fn sorter_wrap(&self) -> bool {
        self.sorter.wrap()
    }

    /// Configure whether cursor navigation wraps around grid boundaries.
    pub fn set_sorter_wrap(&mut self, wrap: bool) {
        self.sorter.set_wrap(wrap);
    }

    /// Toggle sorter grid mode on and off.
    pub fn toggle_sorter(&mut self) {
        if self.mode == AppMode::Sorter {
            self.exit_sorter(false);
        } else {
            self.unblank();
            if self.mode != AppMode::HelpModal {
                self.previous_mode = self.mode;
            }
            self.mode = AppMode::Sorter;
            self.sorter.selected_index = self.current_slide;
        }
    }

    /// Open sorter mode directly.
    pub fn open_sorter(&mut self) {
        if self.mode != AppMode::Sorter {
            self.unblank();
            if self.mode != AppMode::HelpModal {
                self.previous_mode = self.mode;
            }
            self.mode = AppMode::Sorter;
            self.sorter.selected_index = self.current_slide;
        }
    }

    /// Exit sorter mode, optionally selecting the highlighted slide.
    /// Returns true if the presentation slide index changed.
    pub fn exit_sorter(&mut self, apply_selection: bool) -> bool {
        if self.mode != AppMode::Sorter {
            return false;
        }
        let mut slide_changed = false;
        if apply_selection && self.total_slides > 0 {
            let clamped = self.sorter.selected_index.min(self.total_slides - 1);
            slide_changed = self.current_slide != clamped;
            self.current_slide = clamped;
        }
        self.mode = if self.previous_mode == AppMode::Sorter {
            AppMode::Fullscreen
        } else {
            self.previous_mode
        };
        slide_changed
    }

    /// Move sorter selection cursor one cell to the left.
    pub fn sorter_left(&mut self) -> bool {
        self.sorter.left(self.total_slides)
    }

    /// Move sorter selection cursor one cell to the right.
    pub fn sorter_right(&mut self) -> bool {
        self.sorter.right(self.total_slides)
    }

    /// Move sorter selection cursor one row up.
    pub fn sorter_up(&mut self) -> bool {
        self.sorter.up(self.total_slides)
    }

    /// Move sorter selection cursor one row down.
    pub fn sorter_down(&mut self) -> bool {
        self.sorter.down(self.total_slides)
    }

    /// Move sorter selection cursor one page up.
    pub fn sorter_page_up(&mut self) -> bool {
        self.sorter.page_up(self.total_slides)
    }

    /// Move sorter selection cursor one page down.
    pub fn sorter_page_down(&mut self) -> bool {
        self.sorter.page_down(self.total_slides)
    }

    /// Jump sorter cursor to the first slide.
    pub fn sorter_home(&mut self) -> bool {
        self.sorter.home(self.total_slides)
    }

    /// Jump sorter cursor to the last slide.
    pub fn sorter_end(&mut self) -> bool {
        self.sorter.end(self.total_slides)
    }

    /// Update dynamic layout dimensions when terminal is resized.
    pub fn update_layout_dimensions(&mut self, width: u16, _height: u16) {
        self.sorter.update_cols_for_width(width);
    }

    /// Slide models attached to this presentation.
    pub fn slides(&self) -> &[Slide] {
        &self.slides
    }

    /// Set presentation slide models.
    pub fn set_slides(&mut self, slides: Vec<Slide>) {
        self.slides = slides;
    }

    /// Builder pattern helper to attach slide models.
    pub fn with_slides(mut self, slides: Vec<Slide>) -> Self {
        self.slides = slides;
        self
    }

    /// Current slide's model if available.
    pub fn current_slide_model(&self) -> Option<&Slide> {
        self.slides.get(self.current_slide)
    }

    /// Returns true if the currently active slide contains embedded media.
    pub fn current_slide_has_media(&self) -> bool {
        self.current_slide_model().is_some_and(crate::app::media::has_slide_media)
    }

    /// Set an ephemeral user notification toast.
    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast_message = Some(msg.into());
        self.toast_time = Some(Instant::now());
    }

    /// Get current active toast notification if it has not expired (5s timeout).
    pub fn toast(&self) -> Option<&str> {
        if let Some(time) = self.toast_time {
            if time.elapsed() < Duration::from_secs(5) {
                return self.toast_message.as_deref();
            }
        }
        None
    }

    /// Clear user notification toast.
    pub fn clear_toast(&mut self) {
        self.toast_message = None;
        self.toast_time = None;
    }
}

