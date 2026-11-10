use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use kitty_pptx_core::WarningEvent;
use crate::app::state::PresentationState;

/// Render the fidelity warnings drawer sliding up from the bottom edge.
pub fn render_warning_drawer(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let drawer_height = 10.min(area.height.saturating_sub(2));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(drawer_height),
        ])
        .split(area);

    let drawer_area = chunks[1];

    // Clear background behind drawer
    f.render_widget(Clear, drawer_area);

    let warnings = state.warnings();
    let title = format!(" Fidelity Warnings ({}) ", warnings.len());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let mut lines = Vec::new();

    if warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" ✓ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(
                "No fidelity warnings reported. Presentation rendered at full fidelity.",
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " [W/q/Esc] Close Drawer",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " [↑/↓, j/k] Scroll   [W/q/Esc] Close Drawer",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        for (i, warning) in warnings.iter().enumerate() {
            let mut spans = Vec::new();

            // Index
            spans.push(Span::styled(
                format!("{}. ", i + 1),
                Style::default().fg(Color::DarkGray),
            ));

            // Slide tag
            if let Some(slide_idx) = warning.slide_index {
                spans.push(Span::styled(
                    format!("[Slide {}] ", slide_idx + 1),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    "[Session] ",
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                ));
            }

            // Warning event details
            match &warning.event {
                WarningEvent::FontSubstituted { requested, substitute } => {
                    spans.push(Span::styled(
                        "FONT: ",
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(format!(
                        "Substituted \"{}\" with fallback \"{}\"",
                        requested, substitute
                    )));
                }
                WarningEvent::UnsupportedPreset { name } => {
                    spans.push(Span::styled(
                        "PRESET: ",
                        Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(format!(
                        "Shape preset \"{}\" fell back to approximated bounds",
                        name
                    )));
                }
                WarningEvent::MediaFailed { rel_id, reason } => {
                    spans.push(Span::styled(
                        "MEDIA: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(format!(
                        "Media element \"{}\" failed: {}",
                        rel_id, reason
                    )));
                }
                WarningEvent::FidelityDegraded { element_id, feature, class } => {
                    spans.push(Span::styled(
                        "FIDELITY: ",
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(format!(
                        "Element \"{}\" feature \"{}\" degraded ({:?})",
                        element_id, feature, class
                    )));
                }
                WarningEvent::ResourceLimitHit { limit } => {
                    spans.push(Span::styled(
                        "LIMIT: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(format!("Resource limit reached: {}", limit)));
                }
            }

            lines.push(Line::from(spans));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((state.warnings_scroll() as u16, 0));

    f.render_widget(paragraph, drawer_area);
}
