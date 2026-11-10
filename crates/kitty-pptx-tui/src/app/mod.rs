pub mod state;
pub mod events;
pub mod ui;

use std::sync::Arc;
use std::time::Duration;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use kitty_pptx_core::{InvalidateCause, PrefetchPlan, RenderPipeline, RenderTargetSpec};
use kitty_pptx_display::TerminalGraphicsDriver;

pub use state::{AppMode, BlankMode, PresentationState};
pub use events::{handle_key_event, handle_resize_event, AppAction};
pub use ui::render_app;

/// Full interactive slide presentation runner.
pub struct PresentationApp {
    pipeline: Arc<dyn RenderPipeline>,
    driver: Box<dyn TerminalGraphicsDriver>,
    state: PresentationState,
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
            is_running: true,
            current_target: RenderTargetSpec { width_px: 1920, height_px: 1080 },
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
        match action {
            AppAction::Quit => {
                self.is_running = false;
            }
            AppAction::SlideChanged => {
                self.trigger_prefetch();
            }
            _ => {}
        }
        action
    }

    /// Handle terminal window resize cleanly, adjusting target resolution and re-staging.
    pub fn handle_resize(&mut self, cols: u16, rows: u16) {
        // Approximate pixel target based on terminal dimensions (typical cell is ~10x20 px)
        let width_px = (cols as u32 * 10).max(640);
        let height_px = (rows as u32 * 20).max(360);
        self.current_target = RenderTargetSpec { width_px, height_px };

        self.pipeline.invalidate(InvalidateCause::Resize);
        self.trigger_prefetch();
    }

    /// Runs the interactive presentation event loop.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::cursor::Hide
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Staging initial slides
        self.trigger_prefetch();

        while self.is_running {
            terminal.draw(|f| {
                ui::render_app(&self.state, f);
            })?;

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key);
                    }
                    Event::Resize(cols, rows) => {
                        self.handle_resize(cols, rows);
                    }
                    _ => {}
                }
            }
        }

        // Cleanup
        self.driver.release_all().ok();
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;

        Ok(())
    }
}
