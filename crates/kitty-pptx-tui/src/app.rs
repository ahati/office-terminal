use std::sync::Arc;
use std::time::{Duration, Instant};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use kitty_pptx_core::{RenderPipeline, PrefetchPlan, RenderTargetSpec};
use kitty_pptx_display::{TerminalGraphicsDriver, ImageId, PlacementMode};
use crate::widget::SlideWidget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    Fullscreen,
    PresenterConsole,
    SlideSorter,
    HelpModal,
}

pub struct PresentationApp {
    pipeline: Arc<dyn RenderPipeline>,
    driver: Box<dyn TerminalGraphicsDriver>,
    current_slide: usize,
    total_slides: usize,
    mode: AppMode,
    start_time: Instant,
    notes: Vec<Option<String>>,
    is_running: bool,
}

impl PresentationApp {
    pub fn new(
        pipeline: Arc<dyn RenderPipeline>,
        driver: Box<dyn TerminalGraphicsDriver>,
        total_slides: usize,
        notes: Vec<Option<String>>,
    ) -> Self {
        Self {
            pipeline,
            driver,
            current_slide: 0,
            total_slides,
            mode: AppMode::Fullscreen,
            start_time: Instant::now(),
            notes,
            is_running: true,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen, crossterm::cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let target = RenderTargetSpec { width_px: 1920, height_px: 1080 };
        self.pipeline.prefetch(&PrefetchPlan { center: 0, behind: 2, ahead: 2, stage: true }, &target);

        while self.is_running {
            terminal.draw(|f| {
                let size = f.area();
                match self.mode {
                    AppMode::Fullscreen => {
                        let chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([Constraint::Min(3), Constraint::Length(1)])
                            .split(size);

                        // Slide canvas
                        let slide_widget = SlideWidget {
                            image_id: ImageId((self.current_slide % 8 + 1) as u32),
                            mode: PlacementMode::Interactive,
                        };
                        f.render_widget(slide_widget, chunks[0]);

                        // Status bar
                        let elapsed = self.start_time.elapsed().as_secs();
                        let status_text = format!(
                            " Slide {} / {} | Elapsed: {:02}:{:02}:{:02} | [?] Help | [p] Presenter | [q] Quit ",
                            self.current_slide + 1, self.total_slides,
                            elapsed / 3600, (elapsed % 3600) / 60, elapsed % 60
                        );
                        let status = Paragraph::new(Line::from(vec![
                            Span::styled(status_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                        ])).block(Block::default().borders(Borders::NONE));
                        f.render_widget(status, chunks[1]);
                    }
                    AppMode::PresenterConsole => {
                        let vert = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([Constraint::Min(3), Constraint::Length(1)])
                            .split(size);

                        let split = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                            .split(vert[0]);

                        // Left: Current slide
                        let slide_widget = SlideWidget {
                            image_id: ImageId((self.current_slide % 8 + 1) as u32),
                            mode: PlacementMode::Interactive,
                        };
                        f.render_widget(slide_widget, split[0]);

                        // Right: Notes
                        let notes_text = self.notes.get(self.current_slide)
                            .and_then(|n| n.as_deref())
                            .unwrap_or("No speaker notes for this slide.");
                        let notes_widget = Paragraph::new(notes_text)
                            .block(Block::default().title(" Speaker Notes ").borders(Borders::ALL));
                        f.render_widget(notes_widget, split[1]);

                        // Bottom status bar
                        let elapsed = self.start_time.elapsed().as_secs();
                        let status_text = format!(
                            " PRESENTER MODE | Slide {} / {} | {:02}:{:02}:{:02} | [p] Normal View ",
                            self.current_slide + 1, self.total_slides,
                            elapsed / 3600, (elapsed % 3600) / 60, elapsed % 60
                        );
                        let status = Paragraph::new(status_text)
                            .style(Style::default().fg(Color::Yellow));
                        f.render_widget(status, vert[1]);
                    }
                    AppMode::HelpModal => {
                        let block = Block::default()
                            .title(" Help / Keybindings ")
                            .borders(Borders::ALL)
                            .style(Style::default().bg(Color::Black).fg(Color::White));
                        let text = vec![
                            Line::from("Space / Enter / Right / j : Next Slide"),
                            Line::from("Backspace / Left / k    : Previous Slide"),
                            Line::from("p                       : Toggle Presenter View"),
                            Line::from("Tab                     : Toggle Slide Sorter"),
                            Line::from("?                       : Toggle Help"),
                            Line::from("q / Esc                 : Quit"),
                        ];
                        let p = Paragraph::new(text).block(block);
                        f.render_widget(p, size);
                    }
                    _ => {}
                }
            })?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }
        }

        // Cleanup
        self.driver.release_all().ok();
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen, crossterm::cursor::Show)?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.mode == AppMode::HelpModal {
                    self.mode = AppMode::Fullscreen;
                } else {
                    self.is_running = false;
                }
            }
            KeyCode::Char('?') => {
                self.mode = if self.mode == AppMode::HelpModal {
                    AppMode::Fullscreen
                } else {
                    AppMode::HelpModal
                };
            }
            KeyCode::Char('p') => {
                self.mode = if self.mode == AppMode::PresenterConsole {
                    AppMode::Fullscreen
                } else {
                    AppMode::PresenterConsole
                };
            }
            KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('j') => {
                if self.current_slide + 1 < self.total_slides {
                    self.current_slide += 1;
                    let target = RenderTargetSpec { width_px: 1920, height_px: 1080 };
                    self.pipeline.prefetch(
                        &PrefetchPlan { center: self.current_slide, behind: 2, ahead: 2, stage: true },
                        &target,
                    );
                }
            }
            KeyCode::Left | KeyCode::Backspace | KeyCode::Char('k') => {
                if self.current_slide > 0 {
                    self.current_slide -= 1;
                    let target = RenderTargetSpec { width_px: 1920, height_px: 1080 };
                    self.pipeline.prefetch(
                        &PrefetchPlan { center: self.current_slide, behind: 2, ahead: 2, stage: true },
                        &target,
                    );
                }
            }
            _ => {}
        }
    }
}
