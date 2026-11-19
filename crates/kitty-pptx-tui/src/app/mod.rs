pub mod bindings;
mod deck_state;
pub mod events;
mod find_state;
#[cfg(test)]
#[path = "first_frame_tests.rs"]
mod first_frame_tests;
mod graphics;
pub(crate) mod graphics_reader;
pub(crate) mod graphics_slots;
#[cfg(test)]
mod graphics_tests;
pub(crate) mod graphics_thumbnails;
pub mod link_state;
mod loop_wake;
pub mod media;
pub mod mode;
mod mode_stack;
pub mod nav_state;
mod reader;
pub mod reader_events;
#[cfg(test)]
mod reader_graphics_tests;
pub mod reader_scroll;
pub mod reader_state;
pub mod reader_types;
pub mod search_state;
#[cfg(test)]
mod sorter_graphics_tests;
pub mod sorter_state;
#[cfg(test)]
mod sorter_thumbnail_recovery_tests;
pub mod state;
mod state_bindings;
mod state_notices;
mod state_reader;
mod state_timing;
mod session_state;
mod terminal_guard;
pub(crate) mod toc_numbering;
#[cfg(test)]
mod toc_numbering_tests;
pub mod ui;
pub mod warnings_state;

use crossterm::event::{self, Event, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kitty_pptx_core::{
    InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
};
use kitty_pptx_display::TerminalGraphicsDriver;

use crate::app::bindings::FrameBindings;

pub use events::{handle_key_event, handle_resize_event, AppAction};
pub use find_state::FindState;
pub use link_state::{handle_mouse_event, launch_external_browser, LinkDispatch, LinkPickerState};
pub use media::{find_slide_media, has_slide_media, try_play_current_slide_media};
pub use mode::{AppMode, BlankMode, PresenterMode};
pub use nav_state::NavState;
pub use search_state::{SearchState, SlideSearchEntry};
pub use sorter_state::SorterState;
pub use state::PresentationState;
pub use ui::render_app;
pub use warnings_state::{SlideWarning, WarningsState};

use graphics::GraphicsSyncer;
use terminal_guard::TerminalGuard;

/// Full interactive slide presentation runner.
pub struct PresentationApp {
    pipeline: Arc<dyn RenderPipeline>,
    driver: Box<dyn TerminalGraphicsDriver>,
    state: PresentationState,
    graphics: GraphicsSyncer,
    is_running: bool,
    current_target: RenderTargetSpec,
    /// Yields one event per worker-completed raster; drained on every loop
    /// wake so freshly published results re-sync without blocking the poll.
    render_events: Option<crossbeam_channel::Receiver<()>>,
}

/// Backwards-compatible type alias for `PresentationApp`.
pub type PresenterApp = PresentationApp;

impl PresentationApp {
    pub fn new(
        pipeline: Arc<dyn RenderPipeline>,
        driver: Box<dyn TerminalGraphicsDriver>,
        total_slides: usize,
        notes: Vec<Option<String>>,
    ) -> Self {
        let state = PresentationState::new(total_slides, notes);
        Self {
            pipeline,
            driver,
            state,
            graphics: GraphicsSyncer::new(),
            is_running: true,
            current_target: RenderTargetSpec {
                width_px: 1920,
                height_px: 1080,
            },
            render_events: None,
        }
    }

    /// Configure the initial slide number (0-based).
    pub fn with_start_slide(mut self, slide_0based: usize) -> Self {
        self.state.set_current_slide(slide_0based);
        self
    }

    /// Start directly in split-pane presenter mode.
    pub fn with_presenter_mode(mut self, presenter: bool) -> Self {
        if presenter {
            self.state.set_mode(AppMode::PresenterConsole);
        }
        self
    }

    /// Attach presentation slide models for media inspection and rich display.
    pub fn with_slides(mut self, slides: Vec<pptx_model::Slide>) -> Self {
        self.state.set_slides(slides);
        self
    }

    /// Attach physical slide dimensions for hyperlink mouse hit-testing.
    pub fn with_slide_dimensions(mut self, width_emu: i64, height_emu: i64) -> Self {
        self.state.set_slide_dimensions(width_emu, height_emu);
        self
    }

    /// Configure reader mode with total page count and page dimensions in points.
    pub fn with_reader_mode(
        mut self,
        enabled: bool,
        total_pages: usize,
        page_sizes: Vec<(f32, f32)>,
    ) -> Self {
        if enabled {
            self.state.enable_reader_mode(total_pages, page_sizes);
        }
        self
    }

    /// Set presentation or document title.
    pub fn with_deck_title(mut self, title: impl Into<String>) -> Self {
        self.state.set_deck_title(title);
        self
    }

    /// Attach a document table of contents for fuzzy search.
    pub fn with_toc(mut self, toc: Vec<kitty_pptx_core::TocEntry>) -> Self {
        self.state.set_toc(&toc);
        self
    }

    /// Attach the pipeline's worker-completion receiver so the UI loop can
    /// observe freshly published rasters and re-sync placements for them.
    pub fn with_render_events(mut self, rx: crossbeam_channel::Receiver<()>) -> Self {
        self.render_events = Some(rx);
        self
    }

    pub fn state(&self) -> &PresentationState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut PresentationState {
        &mut self.state
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn current_slide(&self) -> usize {
        self.state.current_slide()
    }

    pub fn total_slides(&self) -> usize {
        self.state.total_slides()
    }

    pub fn mode(&self) -> AppMode {
        self.state.mode()
    }

    /// Triggers background prefetching and staging around the current slide.
    pub fn trigger_prefetch(&self) {
        if self.state.total_slides() == 0 {
            return;
        }
        if self.state.base_mode() == AppMode::Reader {
            // Reader prefetching is coordinated exclusively by ReaderSyncer
            // against the exact page aspect ratio and placement targets.
            return;
        }
        let plan = PrefetchPlan {
            center: self.state.current_slide(),
            behind: 2,
            ahead: 2,
            stage: true,
        };
        self.pipeline.prefetch(&plan, &self.current_target);
    }

    /// Process a keyboard event and perform necessary pipeline actions.
    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let leaving_sorter = self.state.base_mode() == AppMode::Sorter;
        let action = handle_key_event(key, &mut self.state);
        self.prefetch_after_leaving_sorter(leaving_sorter);
        self.apply_action(&action);
        action
    }

    /// Process a mouse event and perform the resulting action.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        let leaving_sorter = self.state.base_mode() == AppMode::Sorter;
        // Reader panes scroll on wheel notches before the default handler,
        // which only routes left-click link hit-testing.
        let action = if self.state.base_mode() == AppMode::Reader
            && matches!(
                mouse.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            )
        {
            reader_events::handle_reader_mouse(mouse, &mut self.state)
        } else {
            handle_mouse_event(mouse, &mut self.state)
        };
        self.prefetch_after_leaving_sorter(leaving_sorter);
        self.apply_action(&action);
        action
    }

    /// Queues a full-canvas render when the sorter hands the screen back.
    ///
    /// Leaving the grid does not always produce a `SlideChanged` action (Esc
    /// returns `Redraw`), yet the full-screen slide must be re-rendered at the
    /// main target before it can be staged again.
    fn prefetch_after_leaving_sorter(&mut self, was_sorter: bool) {
        if was_sorter && self.state.base_mode() != AppMode::Sorter {
            self.trigger_prefetch();
        }
    }

    fn apply_action(&mut self, action: &AppAction) {
        match action {
            AppAction::Quit => {
                self.is_running = false;
            }
            AppAction::SlideChanged => {
                self.trigger_prefetch();
            }
            AppAction::OpenExternal(url) => {
                if launch_external_browser(url).is_err() {
                    self.state
                        .set_toast("[Error: Unable to launch web browser]");
                }
            }
            AppAction::ScaleChanged => self.apply_scale(),
            AppAction::None | AppAction::Redraw => {}
        }
    }

    /// Zoom by an explicit factor. Returns whether the factor changed.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if !self.state.set_scale(scale) {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Step to the next / previous ladder rung.
    pub fn zoom_in(&mut self) -> bool {
        if !self.state.nav_mut().zoom_in() {
            return false;
        }
        self.apply_scale();
        true
    }

    pub fn zoom_out(&mut self) -> bool {
        if !self.state.nav_mut().zoom_out() {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Fit the slide width / whole slide to the current terminal grid.
    pub fn fit_width(&mut self) -> bool {
        let (cols, _) = self.state.layout_size();
        if !self.state.fit_width(cols as f32) {
            return false;
        }
        self.apply_scale();
        true
    }

    pub fn fit_page(&mut self) -> bool {
        let (cols, rows) = self.state.layout_size();
        if !self.state.fit_page(cols as f32, rows as f32) {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Recompute the render target from the latched layout and zoom factor
    /// without changing the terminal grid. Used after a zoom step.
    fn apply_scale(&mut self) {
        let (cols, rows) = self.state.layout_size();
        self.recompute_target(cols, rows);
        self.pipeline.invalidate(InvalidateCause::TargetChanged);
        self.restage_graphics();
        self.trigger_prefetch();
    }

    /// Base target for the terminal grid, multiplied by the latched zoom.
    fn recompute_target(&mut self, cols: u16, rows: u16) {
        if self.state.reader_state().is_some() {
            let (slide_width, slide_height) = self.state.slide_dimensions();
            let slide_aspect = (slide_width as f64 / slide_height.max(1) as f64).max(0.1);
            let target_w = 2560u32;
            let target_h = ((target_w as f64 / slide_aspect).round() as u32).clamp(1024, 8192);
            self.current_target = RenderTargetSpec {
                width_px: target_w,
                height_px: target_h,
            };
            return;
        }

        // Pixel target based on live terminal dimensions and slide aspect ratio.
        let (slide_width, slide_height) = self.state.slide_dimensions();
        let slide_aspect = slide_width as f64 / slide_height.max(1) as f64;
        let metrics = kitty_pptx_display::probe_cell_pixel_size();
        let cell_w = metrics.width_px.max(8.0);
        let cell_h = metrics.height_px.max(16.0);
        let screen_w = (cols as f64 * cell_w * 2.0).round() as u32;
        let screen_h = (rows as f64 * cell_h * 2.0).round() as u32;
        let max_width = screen_w.clamp(1920, 3840);
        let max_height = screen_h.clamp(1080, 2160);

        let base_width = max_width
            .min((max_height as f64 * slide_aspect).round() as u32)
            .max(640);
        let scale = self.state.scale().max(0.01);
        let width_px = ((base_width as f32 * scale).round() as u32).max(1);
        let height_px = ((width_px as f64 / slide_aspect).round().max(1.0)) as u32;
        self.current_target = RenderTargetSpec {
            width_px,
            height_px,
        };
    }

    /// Handle terminal window resize cleanly, adjusting target resolution and re-staging.
    pub fn handle_resize(&mut self, cols: u16, rows: u16) {
        // Cell pixel metrics changed with the grid: drop the cached probe so
        // reader layout math re-measures on the next frame.
        kitty_pptx_display::invalidate_cell_metrics();
        self.recompute_target(cols, rows);
        self.state.update_layout_dimensions(cols, rows);
        self.pipeline.invalidate(InvalidateCause::Resize);
        // Re-stage at the new target resolution and re-place at the new rect.
        self.restage_graphics();
        self.trigger_prefetch();
    }

    /// Drops every placement before re-staging on a new target.
    ///
    /// Forgetting the bookkeeping alone is not enough: sorter thumbnails are
    /// re-allocated from a free-list, so a card that moves to a lower image id
    /// would otherwise leave its old placement behind at the previous geometry.
    /// The sorter's downscaled-card cache survives the reset: its target is
    /// aspect-derived, so a resize never invalidates the cached cards.
    fn restage_graphics(&mut self) {
        let _ = self.driver.release_all();
        self.graphics.reset_preserving_thumbnail_cache();
    }

    /// Stages and places terminal graphics for the frame that was just drawn,
    /// using the geometry the render pass recorded into `bindings`.
    /// Safe to call every frame: the syncer only talks to the driver when the
    /// slide, its rect, or the blank state actually changed.
    ///
    /// Returns how many compile/expansion diagnostics were drained from the
    /// pipeline and filed into the warnings drawer: the drain runs on every
    /// sync (PRD 13 §10.5), and a nonzero count means the drawer content
    /// changed, so the caller schedules one more frame.
    pub fn sync_graphics(&mut self, bindings: &FrameBindings) -> Result<usize, kitty_pptx_core::DisplayError> {
        let result = self.graphics.sync(
            &mut self.state,
            self.pipeline.as_ref(),
            self.driver.as_mut(),
            &self.current_target,
            bindings,
        );
        let drained = self.pipeline.drain_warnings();
        let surfaced = drained.len();
        for (slide, event) in drained {
            self.state.add_slide_warning(slide, event);
        }
        result.map(|_| surfaced)
    }

    /// Snapshot of the render-failure note: the event loop compares it
    /// across a graphics sync so failures filed (or cleared) during the
    /// sync reach the canvas on the immediately following frame.
    fn render_failure_note(&self) -> Option<(usize, String)> {
        self.state
            .render_failure()
            .map(|(slide, message)| (slide, message.to_owned()))
    }

    /// Runs the interactive presentation event loop.
    ///
    /// Deadline-driven: each iteration first checks whether anything changed
    /// (input events, worker completions, clock/toast boundaries, reader scroll
    /// animation) and only then runs the expensive half — the ratatui frame,
    /// the graphics sync against that frame's `FrameBindings`, and the
    /// publication of those bindings for input-side queries. The poll
    /// timeout is the minimum over the applicable wake categories
    /// ([`loop_wake::next_wake`]); a fully idle presentation blocks on input
    /// instead of redrawing at ~60 fps.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = TerminalGuard::enter()?;
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Establish the aspect-correct target before the first prefetch.
        let area = terminal.size()?;
        self.handle_resize(area.width, area.height);

        // Bounded first-frame render: interactive pipelines are worker-async
        // from here on (sync fallback is off); page turns keep the previous
        // slide on screen until the worker completion event lands. The very
        // first paint is the one place a synchronous render is worth its
        // pause — it guarantees the deck opens on a visible slide instead of
        // a blank-then-pop. See `paint_first_frame` for the reader-mode and
        // duplicate-work reasoning.
        self.paint_first_frame();

        // Dirty-check bookkeeping; latches refresh only on drawn frames.
        let mut tracker = loop_wake::RedrawTracker::new();

        while self.is_running {
            // ── Dirty evaluation ────────────────────────────────────
            // Time-driven categories (reader animation, displayed 1 s
            // clocks, toast TTL, released worker slots) versus the latches
            // from the last drawn frame.
            let in_flight = self.pipeline.in_flight();
            tracker.evaluate(&self.state, in_flight);

            // ── Frame pass ────────────────────────────────────────
            if tracker.needs_redraw() {
                // The render pass is pure: it fills a fresh FrameBindings
                // value instead of mutating state during the draw.
                let mut bindings = FrameBindings::default();
                terminal.draw(|f| {
                    ui::render_app(&self.state, f, &mut bindings);
                })?;

                // Place staged images after ratatui's frame, against the
                // geometry that frame just produced. Best-effort: a failed
                // placement must not terminate the presentation.
                let failure_before = self.render_failure_note();
                let fresh_warnings = self.sync_graphics(&bindings).unwrap_or(0);
                let failure_now = self.render_failure_note();

                // Publish the drawn geometry so input-side queries (link
                // hit-testing, reader pan bounds) see what is on screen.
                self.state.publish_frame_bindings(bindings);

                tracker.mark_drawn(&self.state);
                // Diagnostics that surfaced during the sync — a first render
                // failure for the visible slide, freshly drained compile
                // warnings — only become visible on the next frame.
                if fresh_warnings > 0 || failure_now != failure_before {
                    tracker.mark_dirty();
                }
            }

            // ── Completion drain ────────────────────────────────
            // One event per worker-completed raster, drained on every wake
            // (including skipped-draw iterations): a worker finished
            // publishing fresh results, so the next frame pass re-syncs
            // placements for them.
            if let Some(rx) = self.render_events.as_ref() {
                while rx.try_recv().is_ok() {
                    tracker.mark_dirty();
                }
            }

            // ── Wake ─────────────────────────────────────────
            // Work is already pending (a completion, a surfaced
            // diagnostic): render it without sleeping. Otherwise wait for
            // input up to the next deadline — or block when fully idle;
            // keys, mouse moves and resizes all interrupt the poll.
            let deadline = loop_wake::next_wake(&self.state, in_flight, Instant::now());
            let timeout = if tracker.needs_redraw() {
                Duration::ZERO
            } else {
                deadline.unwrap_or(loop_wake::IDLE_POLL)
            };

            if event::poll(timeout)? {
                let mut ev_opt = Some(event::read()?);
                while let Some(e) = ev_opt {
                    match e {
                        Event::Key(key) => {
                            self.handle_key(key);
                            tracker.mark_dirty();
                        }
                        Event::Resize(cols, rows) => {
                            self.handle_resize(cols, rows);
                            tracker.mark_dirty();
                        }
                        Event::Mouse(mouse) => {
                            self.handle_mouse(mouse);
                            tracker.mark_dirty();
                        }
                        _ => {}
                    }
                    if event::poll(Duration::from_millis(0))? {
                        ev_opt = Some(event::read()?);
                    } else {
                        ev_opt = None;
                    }
                }
            }
        }

        // The guard restores raw mode, mouse capture and the alternate screen
        // on every exit path (normal return, `?` error, unwind, signal).
        self.driver.release_all().ok();

        Ok(())
    }

    /// One-shot synchronous first paint, issued once from `run()` right
    /// after `handle_resize` computed the target and scheduled the prefetch.
    ///
    /// The readiness check resolves the initial slide against the exact
    /// `current_target` (`entry_at`, not `entry`): reader mode hands scheduling
    /// to `ReaderSyncer` without latching the pipeline's last target, so a
    /// plain `entry` could consult a stale latch here. `recompute_target`'s
    /// reader branch has just set `current_target` to the canonical
    /// 2560-px-wide target the reader panes render at, and `current_slide`
    /// mirrors the focused page, so this one code path covers reader decks
    /// too. A worker that already won the race answers `Ready` and the sync
    /// render is skipped.
    ///
    /// `trigger_prefetch` scheduled the same slide at top priority during
    /// `handle_resize`; when the sync render wins instead, the worker's
    /// later result hits the raster-cache fast path (idempotent, cheap) —
    /// acceptable duplicate work for exactly one slide. A render failure is
    /// deliberately dropped: the syncer's failure-note path reports it on the
    /// first frame exactly like a failed worker render would.
    fn paint_first_frame(&self) {
        let initial = self.state.current_slide();
        let entry = self.pipeline.entry_at(initial, &self.current_target);
        if first_frame_needs_sync(&entry) {
            let _ = self.pipeline.render(initial, &self.current_target);
        }
    }
}

/// Whether the bounded first-frame paint needs a synchronous render.
///
/// Only `Ready` skips it — a cached raster (or a worker result that won the
/// race against the first paint) can be staged directly, so the deck opens
/// without paying any pause. `Rendering` still renders: the deck must not
/// open blank while a worker is mid-flight, and the duplicate result is
/// idempotent. `Failed` still renders: the retry either clears the failure
/// latch or re-latches it, and the syncer's failure-note path surfaces the
/// outcome on the first frame either way.
fn first_frame_needs_sync(entry: &PipelineEntry) -> bool {
    !matches!(entry, PipelineEntry::Ready { .. })
}
