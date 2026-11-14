pub mod events;
mod graphics;
#[cfg(test)]
mod graphics_tests;
pub mod link_state;
pub mod media;
pub mod mode;
pub mod nav_state;
pub mod search_state;
pub mod sorter_state;
pub mod state;
mod state_timing;
mod terminal_guard;
pub mod ui;
pub mod warnings_state;

use crossterm::event::{self, Event, KeyEvent, MouseEvent};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::sync::Arc;
use std::time::Duration;

use kitty_pptx_core::{InvalidateCause, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::TerminalGraphicsDriver;

pub use events::{handle_key_event, handle_resize_event, AppAction};
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
        let action = handle_key_event(key, &mut self.state);
        self.apply_action(&action);
        action
    }

    /// Process a mouse event and perform the resulting action.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> AppAction {
        let action = handle_mouse_event(mouse, &mut self.state);
        self.apply_action(&action);
        action
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
            AppAction::None | AppAction::Redraw => {}
        }
    }

    /// Handle terminal window resize cleanly, adjusting target resolution and re-staging.
    pub fn handle_resize(&mut self, cols: u16, rows: u16) {
        // Approximate pixel target based on terminal dimensions (typical cell is ~10x20 px).
        // The slide aspect ratio is authoritative: fit the slide inside the terminal
        // pixel budget so resizing never stretches it.
        let (slide_width, slide_height) = self.state.slide_dimensions();
        let slide_aspect = slide_width as f64 / slide_height as f64;
        let max_width = (cols as u32 * 10).max(640);
        let max_height = (rows as u32 * 20).max(360);
        let width_px = max_width
            .min((max_height as f64 * slide_aspect).round() as u32)
            .max(1);
        let height_px = (width_px as f64 / slide_aspect).round().max(1.0) as u32;
        self.current_target = RenderTargetSpec {
            width_px,
            height_px,
        };

        self.state.update_layout_dimensions(cols, rows);
        self.pipeline.invalidate(InvalidateCause::Resize);
        // Re-stage at the new target resolution and re-place at the new rect.
        self.graphics.reset();
        self.trigger_prefetch();
    }

    /// Stages and places terminal graphics for the frame that was just drawn.
    ///
    /// Safe to call every frame: the syncer only talks to the driver when the
    /// slide, its rect, or the blank state actually changed.
    pub fn sync_graphics(&mut self) -> Result<(), kitty_pptx_core::DisplayError> {
        self.graphics
            .sync(&mut self.state, self.pipeline.as_ref(), self.driver.as_mut())
    }

    /// Runs the interactive presentation event loop.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = TerminalGuard::enter()?;
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Establish the aspect-correct target before the first prefetch.
        let area = terminal.size()?;
        self.handle_resize(area.width, area.height);

        while self.is_running {
            self.state.clear_placement_rects();
            terminal.draw(|f| {
                ui::render_app(&self.state, f);
            })?;

            // Place staged images after ratatui's frame so the graphics layer
            // matches the cell rect the views just computed. Best-effort: a
            // failed placement must not terminate the presentation.
            let _ = self.sync_graphics();

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key);
                    }
                    Event::Resize(cols, rows) => {
                        self.handle_resize(cols, rows);
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse);
                    }
                    _ => {}
                }
            }
        }

        // The guard restores raw mode, mouse capture and the alternate screen
        // on every exit path (normal return, `?` error, unwind, signal).
        self.driver.release_all().ok();

        Ok(())
    }
}
