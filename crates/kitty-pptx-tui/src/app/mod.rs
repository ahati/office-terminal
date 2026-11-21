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
mod graphics_swap_reader_tests;
#[cfg(test)]
mod graphics_swap_tests;
#[cfg(test)]
mod graphics_tests;
#[cfg(test)]
#[path = "graphics_zoom_tests.rs"]
mod graphics_zoom_tests;
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
#[cfg(test)]
mod reader_boundary_tests;
#[cfg(test)]
mod reader_centering_tests;
#[cfg(test)]
mod reader_mixed_orientation_tests;
#[cfg(test)]
mod reader_focus_marker_tests;
pub mod reader_types;
mod scale;
#[cfg(test)]
#[path = "scale_tests.rs"]
mod scale_tests;
pub mod search_state;
#[cfg(test)]
mod sorter_graphics_tests;
pub mod sorter_state;
#[cfg(test)]
mod sorter_fill_tests;
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
mod zoom_pan;

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

/// What one graphics sync produced, consumed by the run loop's dirty checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsSyncOutcome {
    /// Compile/expansion diagnostics drained from the pipeline and filed
    /// into the warnings drawer.
    pub fresh_warnings: usize,
    /// Sorter cards whose staging was deferred by the per-frame throttle.
    /// Their rasters are already resolvable (warm cache), so neither a
    /// worker nor a completion event will ever wake the loop: only another
    /// drawn frame places them.
    pub deferred_card_stages: usize,
}

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
    /// Returns what the sync produced for the run loop's dirty checks: the
    /// compile/expansion diagnostics drained from the pipeline and filed
    /// into the warnings drawer (the drain runs on every sync, PRD 13
    /// §10.5, and a nonzero count means the drawer content changed), plus
    /// the sorter cards the per-frame staging throttle deferred.
    pub fn sync_graphics(
        &mut self,
        bindings: &FrameBindings,
    ) -> Result<GraphicsSyncOutcome, kitty_pptx_core::DisplayError> {
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
        result.map(|deferred_card_stages| GraphicsSyncOutcome {
            fresh_warnings: surfaced,
            deferred_card_stages,
        })
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
                let outcome = self.sync_graphics(&bindings).unwrap_or_default();
                let failure_now = self.render_failure_note();

                // Publish the drawn geometry so input-side queries (link
                // hit-testing, reader pan bounds) see what is on screen.
                self.state.publish_frame_bindings(bindings);

                tracker.mark_drawn(&self.state);
                // Work that surfaced during the sync and only becomes
                // visible on the next frame: a first render failure for the
                // visible slide, freshly drained compile warnings, and
                // sorter cards the staging throttle deferred. Without the
                // last one, re-entering the grid with a warm cache left it
                // four-cards-full until the next key press (the sorter has
                // no clock, and cached rasters schedule no worker whose
                // completion could wake the loop).
                if outcome.fresh_warnings > 0
                    || outcome.deferred_card_stages > 0
                    || failure_now != failure_before
                {
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
            let timeout = self.poll_timeout(&tracker);

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

    /// Poll timeout for the loop's wake phase.
    ///
    /// The worker count is sampled *here*, after the frame pass and the
    /// completion drain — never before them. The frame pass itself schedules
    /// renders (the sorter issues its visible-window prefetch inside
    /// `sync_graphics`), so a count taken before the pass reads an idle
    /// pipeline and lets the loop block for [`loop_wake::IDLE_POLL`] while
    /// the rasters it just requested are still in flight: the grid then stays
    /// blank/half-filled until the next key press wakes the loop. Worker
    /// completion events land on a channel, not on stdin, so `event::poll`
    /// cannot see them.
    ///
    /// A completion event still queued from a drain race also forbids
    /// sleeping: it means a raster was published and the next pass has pixels
    /// to place.
    fn poll_timeout(&self, tracker: &loop_wake::RedrawTracker) -> Duration {
        let completions_pending = self
            .render_events
            .as_ref()
            .is_some_and(|rx| !rx.is_empty());
        if tracker.needs_redraw() || completions_pending {
            return Duration::ZERO;
        }
        loop_wake::next_wake(&self.state, self.pipeline.in_flight(), Instant::now())
            .unwrap_or(loop_wake::IDLE_POLL)
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
