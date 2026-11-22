use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph};

use crate::app::state::BlankMode;

/// Render blank screen mode (black or white) for pausing talks.
pub fn render_blank_screen(mode: BlankMode, f: &mut Frame, area: Rect) {
    let (bg_color, fg_color, label) = match mode {
        BlankMode::Black => (
            Color::Black,
            Color::DarkGray,
            "[Screen Blanked - Press 'b', 'w', or any key to resume]",
        ),
        BlankMode::White => (
            Color::White,
            Color::Gray,
            "[Screen Blanked - Press 'b', 'w', or any key to resume]",
        ),
        BlankMode::None => return,
    };

    // Fill screen with solid color
    let block = Block::default().style(Style::default().bg(bg_color));
    f.render_widget(block, area);

    // Bottom subtle hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let hint = Paragraph::new(label)
        .alignment(Alignment::Center)
        .style(Style::default().bg(bg_color).fg(fg_color));
    f.render_widget(hint, chunks[1]);
}
