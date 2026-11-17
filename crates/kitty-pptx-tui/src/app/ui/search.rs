use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::state::PresentationState;

/// Render the centered Fuzzy Table of Contents search modal dialog.
pub fn render_search_modal(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let modal_width = 72.min(area.width.saturating_sub(4));
    let modal_height = 20.min(area.height.saturating_sub(2));

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

    // Clear background behind modal
    f.render_widget(Clear, modal_area);

    let modal_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Input query box
            Constraint::Min(4),    // Matched results list
        ])
        .split(modal_area);

    let search = state.search_state();

    // 1. Search Input Box
    let query_block = Block::default()
        .title(" Fuzzy TOC Search (/ or Ctrl+F) ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .bg(Color::Black)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let query_line = Line::from(vec![
        Span::styled(" 🔍 ", Style::default().fg(Color::Yellow)),
        Span::styled(search.query(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("█", Style::default().fg(Color::Cyan)), // Blinking input cursor simulation
    ]);
    f.render_widget(
        Paragraph::new(query_line)
            .block(query_block)
            .style(Style::default().bg(Color::Black)),
        modal_chunks[0],
    );

    // 2. Results List
    let matched = search.matched_indices();
    let list_title = format!(" Matched Slides ({}) ", matched.len());
    let list_block = Block::default()
        .title(list_title)
        .borders(Borders::ALL)
        .border_style(Style::default().bg(Color::Black).fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let inner_height = modal_chunks[1].height.saturating_sub(2) as usize;
    let mut list_lines = Vec::new();

    if matched.is_empty() {
        list_lines.push(Line::from(""));
        list_lines.push(Line::from(Span::styled(
            "   No slides matched your search query.",
            Style::default().fg(Color::DarkGray),
        )));
        list_lines.push(Line::from(""));
        list_lines.push(Line::from(Span::styled(
            "   [Esc] Dismiss Search",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let cursor = search.selected_cursor();
        let scroll_top = if cursor >= inner_height {
            cursor - inner_height + 1
        } else {
            0
        };

        let visible_count = inner_height.min(matched.len().saturating_sub(scroll_top));

        for row in 0..visible_count {
            let item_idx = scroll_top + row;
            let entry_idx = matched[item_idx];
            let is_selected = item_idx == cursor;

            if let Some(entry) = search.entry(entry_idx) {
                let mut spans = Vec::new();
                let indent = "  ".repeat(entry.level);
                let icon = if entry.has_children {
                    if entry.expanded { "▾ " } else { "▸ " }
                } else if entry.level > 0 {
                    "• "
                } else {
                    "  "
                };

                let q = search.query().trim().to_lowercase();
                let is_direct_match = q.is_empty() || {
                    let title_lower = entry.title.to_lowercase();
                    let num_str = entry.slide_number.to_string();
                    title_lower.contains(&q) || num_str.contains(&q)
                };

                let max_title_len = 40usize.saturating_sub(entry.level * 2).max(15);

                if is_selected {
                    spans.push(Span::styled(
                        format!(" ▶ {}{}[{:>2}] ", indent, icon, entry.slide_number),
                        Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        truncate_str(&entry.title, max_title_len),
                        Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD),
                    ));

                    if !entry.notes.is_empty() {
                        spans.push(Span::styled(
                            format!(" (Notes: {})", truncate_str(&entry.notes, 16)),
                            Style::default().bg(Color::Cyan).fg(Color::DarkGray),
                        ));
                    }
                } else if is_direct_match {
                    spans.push(Span::styled(
                        format!("   {}{}[{:>2}] ", indent, icon, entry.slide_number),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        truncate_str(&entry.title, max_title_len),
                        Style::default().fg(Color::White),
                    ));

                    if !entry.notes.is_empty() {
                        spans.push(Span::styled(
                            format!(" (Notes: {})", truncate_str(&entry.notes, 16)),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                } else {
                    // Ancestor context node shown dimmed
                    spans.push(Span::styled(
                        format!("   {}{}[{:>2}] ", indent, icon, entry.slide_number),
                        Style::default().fg(Color::DarkGray),
                    ));
                    spans.push(Span::styled(
                        truncate_str(&entry.title, max_title_len),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                list_lines.push(Line::from(spans));
            }
        }
    }

    f.render_widget(
        Paragraph::new(list_lines)
            .block(list_block)
            .style(Style::default().bg(Color::Black)),
        modal_chunks[1],
    );
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let clean = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > max_len {
        let truncated: String = clean.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        clean
    }
}
