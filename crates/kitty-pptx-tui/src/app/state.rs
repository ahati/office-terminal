use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use kitty_pptx_display::{CellRect, ImageId};
use pptx_model::{PptxComment, Slide};

pub use crate::app::mode::{AppMode, BlankMode, PresenterMode};
use crate::app::search_state::SearchState;
use crate::app::sorter_state::SorterState;
use crate::app::warnings_state::WarningsState;
use crate::app::link_state::LinkPickerState;
use crate::widget::SlideRenderMode;
use kitty_pptx_core::SlideLink;

/// Presentation navigation and UI state machine.
pub struct PresentationState {
    pub(crate) current_slide: usize,
    pub(crate) total_slides: usize,
    pub(crate) mode: AppMode,
    pub(crate) previous_mode: AppMode,
    pub(crate) blank_mode: BlankMode,
    pub(crate) status_line_visible: bool,
    pub(crate) notes: Vec<Option<String>>,
    pub(crate) start_time: Instant,
    pub(crate) digit_buffer: String,
    pub(crate) aspect_ratio: f32,

    // Sub-states
    pub(crate) sorter: SorterState,
    pub(crate) search: SearchState,
    pub(crate) warnings: WarningsState,
    pub(crate) link_picker: LinkPickerState,
    pub(crate) comments_scroll: usize,

    // Slide models & media
    pub(crate) slides: Vec<Slide>,
    pub(crate) slide_links: Vec<Vec<SlideLink>>,
    pub(crate) slide_width_emu: i64,
    pub(crate) slide_height_emu: i64,
    pub(crate) layout_cols: u16,
    pub(crate) layout_rows: u16,

    // Terminal graphics placement
    pub(crate) slide_render_mode: SlideRenderMode,
    pub(crate) placement_rect: Arc<Mutex<Option<CellRect>>>,
    pub(crate) preview_placement_rect: Arc<Mutex<Option<CellRect>>>,

    // Notification toast
    pub(crate) toast_message: Option<String>,
    pub(crate) toast_time: Option<Instant>,
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
            search: SearchState::new(),
            warnings: WarningsState::new(),
            link_picker: LinkPickerState::new(),
            comments_scroll: 0,
            slides: Vec::new(),
            slide_links: Vec::new(),
            slide_width_emu: 12_192_000,
            slide_height_emu: 6_858_000,
            layout_cols: 80,
            layout_rows: 24,
            slide_render_mode: SlideRenderMode::Placeholder,
            placement_rect: Arc::new(Mutex::new(None)),
            preview_placement_rect: Arc::new(Mutex::new(None)),
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
        if changed {
            self.comments_scroll = 0;
        }
        changed
    }

    /// Advance to the next slide, clamping to the last slide.
    pub fn next_slide(&mut self) -> bool {
        if self.total_slides == 0 {
            return false;
        }
        if self.current_slide + 1 < self.total_slides {
            self.current_slide += 1;
            self.comments_scroll = 0;
            true
        } else {
            false
        }
    }

    /// Move to the previous slide, clamping to slide 0.
    pub fn prev_slide(&mut self) -> bool {
        if self.current_slide > 0 {
            self.current_slide -= 1;
            self.comments_scroll = 0;
            true
        } else {
            false
        }
    }

    /// Jump to the first slide (slide 0).
    pub fn first_slide(&mut self) -> bool {
        let changed = self.current_slide != 0;
        self.current_slide = 0;
        if changed {
            self.comments_scroll = 0;
        }
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
        if changed {
            self.comments_scroll = 0;
        }
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
        if changed {
            self.comments_scroll = 0;
        }
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

    pub(crate) fn enter_overlay(&mut self, target: AppMode) {
        self.unblank();
        if self.mode.is_base_mode() {
            self.previous_mode = self.mode;
        }
        self.mode = target;
    }

    pub(crate) fn exit_overlay(&mut self) {
        self.mode = if self.previous_mode.is_base_mode() {
            self.previous_mode
        } else {
            AppMode::Fullscreen
        };
    }

    /// Toggle help modal ('?' or 'h').
    pub fn toggle_help(&mut self) {
        if self.mode == AppMode::HelpModal {
            self.close_help();
        } else {
            self.enter_overlay(AppMode::HelpModal);
        }
    }

    /// Close help modal if open.
    pub fn close_help(&mut self) {
        if self.mode == AppMode::HelpModal {
            self.exit_overlay();
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
        self.notes.get(self.current_slide).and_then(|opt| opt.as_deref())
    }

    pub fn notes_slice(&self) -> &[Option<String>] {
        &self.notes
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
        self.next_slide_index().map(|idx| ImageId((idx % 8 + 1) as u32))
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

    // ─── Comments Drawer ────────────────────────────────────────────

    pub fn comments_scroll(&self) -> usize {
        self.comments_scroll
    }

    pub fn scroll_comments_up(&mut self) {
        self.comments_scroll = self.comments_scroll.saturating_sub(1);
    }

    pub fn scroll_comments_down(&mut self) {
        self.comments_scroll = self.comments_scroll.saturating_add(1);
    }

    pub fn current_slide_comments(&self) -> &[PptxComment] {
        if let Some(slide) = self.current_slide_model() {
            &slide.comments
        } else {
            &[]
        }
    }

    pub fn toggle_comments(&mut self) {
        if self.mode == AppMode::CommentsDrawer {
            self.close_comments();
        } else {
            self.open_comments();
        }
    }

    pub fn open_comments(&mut self) {
        self.comments_scroll = 0;
        self.enter_overlay(AppMode::CommentsDrawer);
    }

    pub fn close_comments(&mut self) {
        if self.mode == AppMode::CommentsDrawer {
            self.exit_overlay();
        }
    }

    // ─── Slide Models & Toasts ──────────────────────────────────────

    pub fn slides(&self) -> &[Slide] {
        &self.slides
    }

    pub fn set_slides(&mut self, slides: Vec<Slide>) {
        self.slide_links = kitty_pptx_core::extract_deck_hyperlinks(&slides);
        self.slides = slides;
        self.search.sync_entries(&self.slides, &self.notes, self.total_slides);
    }

    pub fn with_slides(mut self, slides: Vec<Slide>) -> Self {
        self.slide_links = kitty_pptx_core::extract_deck_hyperlinks(&slides);
        self.slides = slides;
        self.search.sync_entries(&self.slides, &self.notes, self.total_slides);
        self
    }

    /// Set the physical slide dimensions used for mouse hit-testing.
    pub fn set_slide_dimensions(&mut self, width_emu: i64, height_emu: i64) {
        if width_emu > 0 && height_emu > 0 {
            self.slide_width_emu = width_emu;
            self.slide_height_emu = height_emu;
        }
    }

    pub fn with_slide_dimensions(mut self, width_emu: i64, height_emu: i64) -> Self {
        self.set_slide_dimensions(width_emu, height_emu);
        self
    }

    /// Record the latest terminal cell grid for mouse hit-testing.
    pub fn set_layout_dimensions(&mut self, cols: u16, rows: u16) {
        self.layout_cols = cols;
        self.layout_rows = rows;
    }

    /// Physical slide dimensions in EMUs (`width`, `height`).
    pub fn slide_dimensions(&self) -> (i64, i64) {
        (self.slide_width_emu, self.slide_height_emu)
    }

    pub fn current_slide_model(&self) -> Option<&Slide> {
        self.slides.get(self.current_slide)
    }

    pub fn current_slide_has_media(&self) -> bool {
        self.current_slide_model().is_some_and(crate::app::media::has_slide_media)
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast_message = Some(msg.into());
        self.toast_time = Some(Instant::now());
    }

    pub fn toast(&self) -> Option<&str> {
        if let Some(time) = self.toast_time {
            if time.elapsed() < Duration::from_secs(5) {
                return self.toast_message.as_deref();
            }
        }
        None
    }

    pub fn clear_toast(&mut self) {
        self.toast_message = None;
        self.toast_time = None;
    }
}
