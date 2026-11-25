use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::state::PresentationState;

/// Render the slide reviewer comments drawer sliding in from the right edge.
pub fn render_comments_drawer(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let drawer_width = 46.min(area.width.saturating_sub(2));
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(drawer_width)])
        .split(area);

    let drawer_area = chunks[1];

    // Clear background behind drawer
    f.render_widget(Clear, drawer_area);

    let comments = state.current_slide_comments();
    let title = format!(" Reviewer Comments ({}) ", comments.len());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .bg(Color::Black)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let mut lines = Vec::new();

    if comments.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " No reviewer comments on this slide.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " [c/q/Esc] Close Drawer",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " [↑/↓, j/k] Scroll   [c/q/Esc] Close",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        for (i, comment) in comments.iter().enumerate() {
            // Author + date + status header
            let mut header_spans = vec![
                Span::styled(
                    format!("#{}. ", i + 1),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    comment.author.as_deref().unwrap_or("Anonymous"),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ];

            if let Some(ref date) = comment.date {
                header_spans.push(Span::styled(
                    format!(" • {}", date),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            if let Some(ref status) = comment.status {
                let status_color = if status.eq_ignore_ascii_case("resolved") {
                    Color::Green
                } else {
                    Color::LightYellow
                };
                header_spans.push(Span::styled(
                    format!(" [{}]", status),
                    Style::default().fg(status_color),
                ));
            }

            lines.push(Line::from(header_spans));

            // Comment text
            for text_line in comment.text.lines() {
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(text_line, Style::default().fg(Color::White)),
                ]));
            }

            // Threaded replies
            for reply in &comment.replies {
                let mut reply_header = vec![
                    Span::raw("   ↳ "),
                    Span::styled(
                        reply.author.as_deref().unwrap_or("Anonymous"),
                        Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
                    ),
                ];
                if let Some(ref date) = reply.date {
                    reply_header.push(Span::styled(
                        format!(" • {}", date),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                lines.push(Line::from(reply_header));

                for reply_line in reply.text.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(reply_line, Style::default().fg(Color::Gray)),
                    ]));
                }
            }

            lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().bg(Color::Black))
        .scroll((state.comments_scroll() as u16, 0));

    f.render_widget(paragraph, drawer_area);
}
