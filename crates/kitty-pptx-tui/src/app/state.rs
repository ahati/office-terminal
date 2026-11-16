use kitty_pptx_display::{
    CellRect, ImageId, CURRENT_IMAGE_ID_BASE, CURRENT_IMAGE_ID_SLOTS, PREVIEW_IMAGE_ID_BASE,
    PREVIEW_IMAGE_ID_SLOTS,
};
use pptx_model::{PptxComment, Slide};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::app::find_state::FindState;
use crate::app::link_state::LinkPickerState;
pub use crate::app::mode::{AppMode, BlankMode, PresenterMode};
use crate::app::nav_state::NavState;
use crate::app::search_state::SearchState;
use crate::app::sorter_state::SorterState;
use crate::app::warnings_state::WarningsState;
use kitty_pptx_core::{HiddenSlideMode, SlideLink};

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

    // Navigation policy: hidden-slide mode + zoom factor.
    pub(crate) nav: NavState,
    /// A caller-owned hyperlink callback owns internal clicks (no navigation).
    pub(crate) internal_link_delegation: bool,

    // Sub-states
    pub(crate) sorter: SorterState,
    pub(crate) search: SearchState,
    pub(crate) warnings: WarningsState,
    pub(crate) link_picker: LinkPickerState,
    pub(crate) find: FindState,
    pub(crate) comments_scroll: usize,

    // Slide models & media
    pub(crate) slides: Vec<Slide>,
    /// Per-slide "has external media" flags, computed once per deck so the
    /// sorter does not rescan element trees on every frame.
    pub(crate) slide_media_flags: Vec<bool>,
    pub(crate) slide_links: Vec<Vec<SlideLink>>,
    pub(crate) slide_width_emu: i64,
    pub(crate) slide_height_emu: i64,
    pub(crate) layout_cols: u16,
    pub(crate) layout_rows: u16,

    // Terminal graphics placement
    pub(crate) placement_rect: Arc<Mutex<Option<CellRect>>>,
    pub(crate) preview_placement_rect: Arc<Mutex<Option<CellRect>>>,
    /// Fitted rects of every sorter card drawn in the last frame.
    pub(crate) thumbnail_placement_rects: Arc<Mutex<Vec<(usize, CellRect)>>>,
    /// Placements of visible document pages in reader mode.
    pub(crate) reader_placements: Arc<Mutex<Vec<crate::app::reader_state::ReaderSlotPlacement>>>,

    // Reader state & doc title
    pub(crate) reader_state: Option<crate::app::reader_state::ReaderState>,
    pub(crate) deck_title: Option<String>,

    // Notification toast
    pub(crate) toast_message: Option<String>,
    pub(crate) toast_time: Option<Instant>,

    /// The slide a background render failed for, with its message. Surfaced
    /// on the canvas (a silent blank otherwise reads as a broken projector).
    pub(crate) render_failure: Option<(usize, String)>,
}

impl PresentationState {
    pub fn new(total_slides: usize, notes: Vec<Option<String>>) -> Self {
        let mut nav = NavState::new();
        nav.set_total(total_slides);
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
            nav,
            internal_link_delegation: false,
            sorter: SorterState::new(),
            search: SearchState::new(),
            warnings: WarningsState::new(),
            link_picker: LinkPickerState::new(),
            find: FindState::new(),
            comments_scroll: 0,
            slides: Vec::new(),
            slide_media_flags: Vec::new(),
            slide_links: Vec::new(),
            slide_width_emu: 12_192_000,
            slide_height_emu: 6_858_000,
            layout_cols: 80,
            layout_rows: 24,
            placement_rect: Arc::new(Mutex::new(None)),
            preview_placement_rect: Arc::new(Mutex::new(None)),
            thumbnail_placement_rects: Arc::new(Mutex::new(Vec::new())),
            reader_placements: Arc::new(Mutex::new(Vec::new())),
            reader_state: None,
            deck_title: None,
            toast_message: None,
            toast_time: None,
            render_failure: None,
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

    /// The persistent view underneath any overlay modal.
    ///
    /// Both the renderer and the graphics syncer resolve the visible base view
    /// through this method so thumbnails keep their images while a help modal
    /// or drawer is open on top of the sorter.
    pub fn base_mode(&self) -> AppMode {
        if self.mode.is_base_mode() {
            self.mode
        } else if self.previous_mode.is_base_mode() {
            self.previous_mode
        } else {
            AppMode::Fullscreen
        }
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

    // ─── Hidden-slide policy (ref hidden.ts, viewer.ts:437-582) ─────

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
        let changed = self.current_slide != target;
        self.current_slide = target;
        if changed {
            self.comments_scroll = 0;
        }
        if let Some(reader) = self.reader_state.as_mut() {
            if reader.focused_page() != target {
                reader.jump_to_page(target);
            }
        }
        changed
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

    /// When set, internal link clicks are reported to the caller instead of
    /// navigating (`onHyperlinkClick`). ref viewer-internal-nav.test.ts:52-72
    pub fn set_internal_link_delegation(&mut self, delegate: bool) {
        self.internal_link_delegation = delegate;
    }

    pub fn delegates_internal_links(&self) -> bool {
        self.internal_link_delegation
    }

    pub fn set_current_slide(&mut self, slide_0based: usize) -> bool {
        if self.total_slides == 0 {
            self.current_slide = 0;
            return false;
        }
        let clamped = slide_0based.min(self.total_slides.saturating_sub(1));
        // `skip` mode replaces a hidden target with the nearest shown slide.
        // ref viewer.ts:437-462
        let target = self.nav.resolve(clamped);
        let changed = self.current_slide != target;
        self.current_slide = target;
        if changed {
            self.comments_scroll = 0;
        }
        if let Some(reader) = self.reader_state.as_mut() {
            if reader.focused_page() != target {
                reader.jump_to_page(target);
            }
        }
        changed
    }

    /// Advance one slide under the hidden-slide policy, clamping at the last.
    // ref viewer.ts:466-510
    pub fn next_slide(&mut self) -> bool {
        self.step_slide(1)
    }

    /// Move back one slide under the hidden-slide policy, clamping at slide 0.
    pub fn prev_slide(&mut self) -> bool {
        self.step_slide(-1)
    }

    fn step_slide(&mut self, direction: i8) -> bool {
        match self.nav.step(self.current_slide, direction) {
            Some(next) => {
                self.current_slide = next;
                self.comments_scroll = 0;
                true
            }
            None => false,
        }
    }

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
        self.notes
            .get(self.current_slide)
            .and_then(|opt| opt.as_deref())
    }

    pub fn notes_slice(&self) -> &[Option<String>] {
        &self.notes
    }

    /// Next slide index under the hidden-slide policy (presenter preview).
    pub fn next_slide_index(&self) -> Option<usize> {
        self.nav.step(self.current_slide, 1)
    }

    /// ImageId for the current slide.
    pub fn current_image_id(&self) -> ImageId {
        ImageId(CURRENT_IMAGE_ID_BASE + (self.current_slide as u32 % CURRENT_IMAGE_ID_SLOTS))
    }

    /// ImageId for the next slide preview.
    ///
    /// Preview ids live in a separate range (9..=16) so releasing the old
    /// preview cannot delete an image that has just become the current slide.
    pub fn next_image_id(&self) -> Option<ImageId> {
        self.next_slide_index()
            .map(|idx| ImageId(PREVIEW_IMAGE_ID_BASE + (idx as u32 % PREVIEW_IMAGE_ID_SLOTS)))
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
        self.nav.set_hidden(
            slides.iter().map(|slide| slide.hidden).collect(),
            self.total_slides,
        );
        self.slide_media_flags = slides
            .iter()
            .map(crate::app::media::has_slide_media)
            .collect();
        self.slide_links = kitty_pptx_core::extract_deck_hyperlinks(&slides);
        self.slides = slides;
        // Initial-slide selection: in `skip` mode a hidden opening slide lands
        // on the first shown one. ref viewer.ts:512-522 (`_initialSlide`)
        if self.total_slides > 0 {
            self.current_slide = self
                .nav
                .resolve(self.current_slide.min(self.total_slides - 1));
        }
        self.sync_search_entries();
    }

    pub fn with_slides(mut self, slides: Vec<Slide>) -> Self {
        self.set_slides(slides);
        self
    }

    /// Rebuild the search index, excluding hidden slides while in `skip` mode.
    pub(crate) fn sync_search_entries(&mut self) {
        let hidden: Vec<bool> = (0..self.total_slides)
            .map(|index| self.nav.is_hidden(index))
            .collect();
        self.search.sync_entries_filtered(
            &self.slides,
            &self.notes,
            self.total_slides,
            &hidden,
            self.nav.hidden_mode().is_skip(),
        );
        self.sync_find_index();
    }

    /// Whether the slide at `index` carries external media.
    ///
    /// Precomputed in [`PresentationState::set_slides`] so per-frame views (the
    /// sorter grid) never walk element trees.
    pub fn slide_has_media(&self, index: usize) -> bool {
        self.slide_media_flags.get(index).copied().unwrap_or(false)
    }

    /// Set the physical slide dimensions used for mouse hit-testing.
    pub fn set_slide_dimensions(&mut self, width_emu: i64, height_emu: i64) {
        if width_emu > 0 && height_emu > 0 {
            self.slide_width_emu = width_emu;
            self.slide_height_emu = height_emu;
            self.aspect_ratio = width_emu as f32 / height_emu as f32;
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
        self.slide_has_media(self.current_slide)
    }
}
