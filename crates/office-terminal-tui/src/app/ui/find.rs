//! In-document find modal (`Ctrl+F`, V2).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::state::PresentationState;

/// Render the find query and its slide matches.
pub fn render_find_modal(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let modal_width = 76.min(area.width.saturating_sub(4));
    let modal_height = 22.min(area.height.saturating_sub(2));
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(modal_width) / 2),
            Constraint::Length(modal_width),
            Constraint::Min(0),
        ])
        .split(area);
    let modal_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(modal_height) / 2),
            Constraint::Length(modal_height),
            Constraint::Min(0),
        ])
        .split(horiz[1])[1];
    f.render_widget(Clear, modal_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(modal_area);
    let find = state.find_state();

    let query_block = Block::default()
        .title(" Find in slides (Ctrl+F) ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .bg(Color::Black)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" 🔎 ", Style::default().fg(Color::Yellow)),
            Span::styled(
                find.query(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]))
        .block(query_block)
        .style(Style::default().bg(Color::Black)),
        chunks[0],
    );

    let matches = find.matches();
    let selected = find.selected();
    let list_block = Block::default()
        .title(format!(" Matches ({}) ", matches.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().bg(Color::Black).fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner_height = chunks[1].height.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    if matches.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            if find.query().is_empty() {
                "   Type to search this deck."
            } else {
                "   No matches."
            },
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   [Enter] Go to match   [Esc] Close",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let scroll_top = selected.saturating_sub(inner_height.saturating_sub(1));
        for (index, matched) in matches.iter().enumerate().skip(scroll_top).take(inner_height) {
            let active = index == selected;
            let text = truncate(&matched.text.replace('\n', " "), 52);
            let mut line = vec![
                Span::styled(
                    format!(" slide {:>3}  ", matched.slide + 1),
                    if active {
                        Style::default()
                            .bg(Color::Cyan)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ),
                Span::styled(
                    text,
                    if active {
                        Style::default()
                            .bg(Color::Cyan)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ];
            if active {
                line.insert(0, Span::raw("▶"));
            }
            lines.push(Line::from(line));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(list_block)
            .style(Style::default().bg(Color::Black)),
        chunks[1],
    );
}

fn truncate(text: &str, max: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > max {
        let prefix: String = clean.chars().take(max.saturating_sub(3)).collect();
        format!("{prefix}...")
    } else {
        clean
    }
}
