use office_terminal_display::{
    CellRect, ImageId, CURRENT_IMAGE_ID_BASE, CURRENT_IMAGE_ID_SLOTS, PREVIEW_IMAGE_ID_BASE,
    PREVIEW_IMAGE_ID_SLOTS,
};
use pptx_model::{PptxComment, Slide};
use std::sync::{Arc, Mutex};

use crate::app::deck_state::DeckState;
use crate::app::find_state::FindState;
use crate::app::link_state::LinkPickerState;
use crate::app::mode_stack::{BaseView, ModeStack};
pub use crate::app::mode::{AppMode, BlankMode, PresenterMode};
use crate::app::nav_state::NavState;
use crate::app::search_state::SearchState;
use crate::app::session_state::SessionState;
use crate::app::sorter_state::SorterState;
use crate::app::warnings_state::WarningsState;

/// Presentation navigation and UI state machine.
///
/// Thin façade over dedicated owners, each reachable through unchanged
/// accessor methods: view/overlay mode ([`ModeStack`]), deck data
/// ([`DeckState`]), session ephemera ([`SessionState`]), the view sub-states
/// (`nav`, `sorter`, `search`, `find`, `warnings`, `link_picker`,
/// `reader_state`) and the last-frame geometry mirrors published by
/// [`crate::app::state_bindings`].
pub struct PresentationState {
    pub(crate) current_slide: usize,
    pub(crate) total_slides: usize,
    /// View/overlay mode: the single owner of `mode`/`previous_mode`/
    /// `sorter_return_mode` bookkeeping.
    pub(crate) mode_stack: ModeStack,
    pub(crate) blank_mode: BlankMode,
    pub(crate) status_line_visible: bool,
    pub(crate) digit_buffer: String,

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

    // Deck data (slide models, media/link flags, geometry, TOC, title).
    pub(crate) deck: DeckState,
    // Session ephemera (presentation clock, toast).
    pub(crate) session: SessionState,

    // Terminal grid recorded for mouse hit-testing (terminal, not deck data).
    pub(crate) layout_cols: u16,
    pub(crate) layout_rows: u16,

    // Last drawn frame geometry (published by the run loop, never written
    // during rendering: the render pass fills a FrameBindings value instead)
    /// Zoom-aware placement of the full-screen current slide in the last frame.
    pub(crate) current_placement: Option<crate::app::bindings::SlidePlacement>,
    /// Placement of the presenter next-slide preview in the last drawn frame.
    pub(crate) preview_placement: Option<crate::app::bindings::SlidePlacement>,
    /// Fitted rect of every sorter card in the last drawn frame.
    pub(crate) thumbnail_placements: Vec<(usize, CellRect)>,
    /// Reader panes placed by the last drawn frame.
    pub(crate) reader_placements_last: Vec<crate::app::reader_state::ReaderSlotPlacement>,
    /// Reader column count the last drawn frame laid out.
    pub(crate) reader_columns_last: usize,
    pub(crate) sorter_scroll_row: Arc<Mutex<usize>>,

    /// Spatial view state for slide-mode zoom: signed cell-space pan from the
    /// centered position (cols, rows). Clamped against the overflow the last
    // drawn frame published; methods live in `zoom_pan.rs`.
    pub(crate) zoom_pan: (f32, f32),

    // Reader mode view state.
    pub(crate) reader_state: Option<crate::app::reader_state::ReaderState>,

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
            mode_stack: ModeStack::default(),
            blank_mode: BlankMode::None,
            status_line_visible: true,
            digit_buffer: String::new(),
            nav,
            internal_link_delegation: false,
            sorter: SorterState::new(),
            search: SearchState::new(),
            warnings: WarningsState::new(),
            link_picker: LinkPickerState::new(),
            find: FindState::new(),
            comments_scroll: 0,
            deck: DeckState::new(notes),
            session: SessionState::new(),
            layout_cols: 80,
            layout_rows: 24,
            current_placement: None,
            preview_placement: None,
            thumbnail_placements: Vec::new(),
            reader_placements_last: Vec::new(),
            reader_columns_last: 0,
            sorter_scroll_row: Arc::new(Mutex::new(0)),
            zoom_pan: (0.0, 0.0),
            reader_state: None,
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
        self.mode_stack.mode()
    }

    /// The persistent view underneath any overlay modal.
    ///
    /// Both the renderer and the graphics syncer resolve the visible base view
    /// through this method so thumbnails keep their images while a help modal
    /// or drawer is open on top of the sorter.
    pub fn base_mode(&self) -> AppMode {
        self.mode_stack.base_mode()
    }

    /// The base view remembered for the current overlay/sorter session.
    pub fn previous_mode(&self) -> AppMode {
        self.mode_stack.previous_base().to_app_mode()
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
        self.deck.aspect_ratio()
    }

    // ─── Navigation tail ───────────────────────────────────────────────

    /// Shared navigation tail for resolved slide targets: write the target
    /// index, reset the comments scroll on movement, mirror the jump into
    /// reader mode, and report whether the slide actually moved.
    ///
    /// Callers resolve the target under their own policy first (hidden-slide
    /// clamping happens before this); the tail itself is policy-free.
    pub(crate) fn navigate_to(&mut self, target: usize) -> bool {
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

    // The hidden-slide policy accessors live in `nav_state.rs`, next to the
    // `NavState` policy they delegate to.

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
        self.navigate_to(target)
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

    // Absolute jump entry points (`first_slide` / `last_slide` /
    // `jump_to_slide`) and the digit-jump buffer (`push_digit` /
    // `commit_digits` / `clear_digits`) live in `nav_state.rs`.

    // ─── Mode stack ─────────────────────────────────────────────────

    /// Toggle split-pane presenter mode ('p').
    pub fn toggle_presenter_mode(&mut self) {
        self.unblank();
        // Toggles on the active mode: with an overlay up this enters the
        // console (and drops the overlay), matching the raw assignment this
        // replaced.
        let target = if self.mode() == AppMode::PresenterConsole {
            BaseView::Fullscreen
        } else {
            BaseView::PresenterConsole
        };
        self.mode_stack.switch_base(target);
    }

    /// Set mode directly.
    pub fn set_mode(&mut self, mode: AppMode) {
        self.mode_stack.set_app_mode(mode);
    }

    pub(crate) fn enter_overlay(&mut self, target: AppMode) {
        self.unblank();
        self.mode_stack.set_app_mode(target);
    }

    pub(crate) fn exit_overlay(&mut self) {
        self.mode_stack.close_overlay();
    }

    /// Toggle help modal ('?' or 'h').
    pub fn toggle_help(&mut self) {
        if self.mode() == AppMode::HelpModal {
            self.close_help();
        } else {
            self.enter_overlay(AppMode::HelpModal);
        }
    }

    /// Close help modal if open.
    pub fn close_help(&mut self) {
        if self.mode() == AppMode::HelpModal {
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
        self.deck
            .notes()
            .get(self.current_slide)
            .and_then(|opt| opt.as_deref())
    }

    pub fn notes_slice(&self) -> &[Option<String>] {
        self.deck.notes()
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
        if self.mode() == AppMode::CommentsDrawer {
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
        if self.mode() == AppMode::CommentsDrawer {
            self.exit_overlay();
        }
    }

    // ─── Slide Models ────────────────────────────────────────────────

    pub fn slides(&self) -> &[Slide] {
        self.deck.slides()
    }

    pub fn set_slides(&mut self, slides: Vec<Slide>) {
        self.nav.set_hidden(
            slides.iter().map(|slide| slide.hidden).collect(),
            self.total_slides,
        );
        self.deck.set_slides(slides);
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
        if !self.deck.toc().is_empty() {
            let labels = self.deck.page_labels().to_vec();
            self.search.sync_from_toc(self.deck.toc(), self.total_slides, &labels);
        } else {
            let hidden: Vec<bool> = (0..self.total_slides)
                .map(|index| self.nav.is_hidden(index))
                .collect();
            self.search.sync_entries_filtered(
                self.deck.slides(),
                self.deck.notes(),
                self.total_slides,
                &hidden,
                self.nav.hidden_mode().is_skip(),
            );
        }
        self.sync_find_index();
    }

    /// Whether the slide at `index` carries external media.
    ///
    /// Precomputed in [`PresentationState::set_slides`] so per-frame views (the
    /// sorter grid) never walk element trees.
    pub fn slide_has_media(&self, index: usize) -> bool {
        self.deck.slide_has_media(index)
    }

    /// Set the physical slide dimensions used for mouse hit-testing.
    pub fn set_slide_dimensions(&mut self, width_emu: i64, height_emu: i64) {
        self.deck.set_slide_dimensions(width_emu, height_emu);
    }

    pub fn with_slide_dimensions(mut self, width_emu: i64, height_emu: i64) -> Self {
        self.set_slide_dimensions(width_emu, height_emu);
        self
    }

    // The terminal-grid setter (`set_layout_dimensions`) lives in
    // `nav_state.rs` beside `layout_size`.

    /// Physical slide dimensions in EMUs (`width`, `height`).
    pub fn slide_dimensions(&self) -> (i64, i64) {
        self.deck.slide_dimensions()
    }

    pub fn current_slide_model(&self) -> Option<&Slide> {
        self.deck.slide(self.current_slide)
    }

    pub fn current_slide_has_media(&self) -> bool {
        self.slide_has_media(self.current_slide)
    }
}
