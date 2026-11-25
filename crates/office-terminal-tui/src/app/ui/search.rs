use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

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

    // Clear and fill background behind modal so underlying graphics don't bleed through
    f.render_widget(Clear, modal_area);
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), modal_area);

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
        Span::styled(
            search.query(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
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
    let inner_width = modal_chunks[1].width.saturating_sub(2) as usize;
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

        // Numbered outlines (PDF bookmarks) lead with the tree number and trail
        // a right-aligned page column; flat decks keep the leading `[page]`
        // prefix so their rows are unchanged. The page column shows the
        // document's logical label (PDF `/PageLabels`) when defined, else the
        // 1-based physical page number.
        let numbered = search.entries().iter().any(|entry| !entry.number.is_empty());
        let page_text = |entry: &crate::app::search_state::SlideSearchEntry| -> String {
            if entry.page_label.is_empty() {
                entry.slide_number.to_string()
            } else {
                entry.page_label.clone()
            }
        };
        let page_width = search
            .entries()
            .iter()
            .map(|entry| page_text(entry).chars().count())
            .max()
            .unwrap_or(1);

        for row in 0..visible_count {
            let item_idx = scroll_top + row;
            let entry_idx = matched[item_idx];
            let is_selected = item_idx == cursor;

            if let Some(entry) = search.entry(entry_idx) {
                let indent = "  ".repeat(entry.level);
                let icon = if entry.has_children {
                    if entry.expanded {
                        "▾ "
                    } else {
                        "▸ "
                    }
                } else if entry.level > 0 {
                    "• "
                } else {
                    "  "
                };
                let lead = if is_selected { " ▶ " } else { "   " };

                let (prefix, suffix) = if numbered {
                    let number = if entry.number.is_empty() {
                        String::new()
                    } else {
                        format!("{} ", entry.number)
                    };
                    let page = page_text(entry);
                    (
                        format!("{lead}{indent}{icon}{number}"),
                        format!(" [{:>width$}]", page, width = page_width),
                    )
                } else {
                    let page = page_text(entry);
                    (
                        format!("{lead}{indent}{icon}[{:>width$}] ", page, width = page_width.max(3)),
                        String::new(),
                    )
                };
                let prefix_len = prefix.chars().count();
                let suffix_len = suffix.chars().count();

                let q = search.query().trim().to_lowercase();
                let is_direct_match = q.is_empty() || crate::app::search_state::calculate_match_score(&q, entry) > 0;

                let notes_span_text = if !entry.notes.is_empty() {
                    let budget = 20.min(inner_width.saturating_sub(prefix_len + 15));
                    if budget >= 8 {
                        Some(format!(
                            " (Notes: {})",
                            truncate_str(&entry.notes, budget.saturating_sub(9))
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let notes_len = notes_span_text.as_ref().map(|n| n.chars().count()).unwrap_or(0);
                // Reserve the trailing page column before truncating the title.
                let max_title_len = inner_width.saturating_sub(prefix_len + notes_len + suffix_len).max(10);
                let title_text = truncate_str(&entry.title, max_title_len);
                let title_len = title_text.chars().count();

                let (chrome_style, title_style, notes_style) = if is_selected {
                    let selected = Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                    (selected, selected, Style::default().bg(Color::Cyan).fg(Color::DarkGray))
                } else if is_direct_match {
                    (
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        Style::default().fg(Color::White),
                        Style::default().fg(Color::DarkGray),
                    )
                } else {
                    // Ancestor context node shown dimmed
                    (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                    )
                };

                let mut spans = vec![
                    Span::styled(prefix, chrome_style),
                    Span::styled(title_text, title_style),
                ];
                if let Some(notes_text) = notes_span_text {
                    spans.push(Span::styled(notes_text, notes_style));
                }
                if numbered {
                    let padding = inner_width.saturating_sub(prefix_len + title_len + notes_len + suffix_len);
                    spans.push(Span::styled(" ".repeat(padding), chrome_style));
                    spans.push(Span::styled(suffix, chrome_style));
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
    let sanitized: String = s
        .chars()
        .filter(|&c| c != '\0' && c != '\u{FEFF}' && c != '\u{FFFD}' && (!c.is_control() || c == ' ' || c == '\t'))
        .collect();
    let clean = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > max_len {
        let truncated: String = clean.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        clean
    }
}
