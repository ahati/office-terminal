use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use office_terminal_core::{HyperlinkTarget, SlideAction, SlideLink};

use crate::app::state::PresentationState;

/// Render the centered active-slide hyperlink picker modal (`o` / Ctrl+O).
pub fn render_link_picker(state: &PresentationState, f: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let modal_width = 78.min(area.width.saturating_sub(4));
    let link_count = state.link_picker().len();
    let desired_height = 5usize.saturating_add(link_count.saturating_mul(2));
    let modal_height = (desired_height as u16).min(area.height.saturating_sub(2));

    let horizontal = Layout::default()
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
        .split(horizontal[1])[1];

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Active Slide Hyperlinks ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .bg(Color::Black)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            " Slide {} of {} · [1..=N]/[Enter] open · [Up/Down] move · [Esc] dismiss ",
            state.current_slide() + 1,
            state.total_slides()
        ),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let picker = state.link_picker();
    if picker.is_empty() {
        lines.push(Line::from(Span::styled(
            "   No hyperlinks on this slide.",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(
            Paragraph::new(lines).style(Style::default().bg(Color::Black)),
            inner,
        );
        return;
    }

    let selected = picker.selected_index();
    let available_rows = inner.height.saturating_sub(2) as usize;
    let rows_per_link = 2usize;
    let visible_links = (available_rows / rows_per_link).max(1);
    let scroll_top = if selected >= visible_links {
        selected + 1 - visible_links
    } else {
        0
    };

    for (index, link) in picker
        .links()
        .iter()
        .enumerate()
        .skip(scroll_top)
        .take(visible_links)
    {
        let highlighted = index == selected;
        let (kind, title) = describe_target(link);
        let key = if index < 9 {
            (index + 1).to_string()
        } else if index == 9 {
            "0".to_string()
        } else {
            " ".to_string()
        };

        let head_style = if highlighted {
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let key_style = if highlighted {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        };
        let kind_style = if highlighted {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" [{key}] "), key_style),
            Span::styled(format!("[{kind}] "), kind_style),
            Span::styled(truncate(&title, 58), head_style),
        ]));

        let detail = match &link.target {
            HyperlinkTarget::ExternalUri(url) => url.clone(),
            HyperlinkTarget::SlideIndex(slide) => format!("Slide #{}", slide + 1),
            HyperlinkTarget::Action(action) => format!("Action: {}", action_name(*action)),
        };
        let detail_style = if highlighted {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let tooltip = link
            .tooltip
            .as_deref()
            .map(|text| format!(" · {text}"))
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("     {}{}", truncate(&detail, 66), truncate(&tooltip, 30)),
            detail_style,
        )));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Color::Black)),
        inner,
    );
}

fn describe_target(link: &SlideLink) -> (&'static str, String) {
    match &link.target {
        HyperlinkTarget::ExternalUri(url) if url.to_ascii_lowercase().starts_with("mailto:") => {
            ("MAIL", url.clone())
        }
        HyperlinkTarget::ExternalUri(url) => ("WEB", url.clone()),
        HyperlinkTarget::SlideIndex(index) => {
            ("SLIDE", format!("Jump to Slide {}", index + 1))
        }
        HyperlinkTarget::Action(action) => {
            ("ACTION", format!("Jump to {}", action_name(*action)))
        }
    }
}

fn action_name(action: SlideAction) -> &'static str {
    match action {
        SlideAction::NextSlide => "NextSlide",
        SlideAction::PreviousSlide => "PreviousSlide",
        SlideAction::FirstSlide => "FirstSlide",
        SlideAction::LastSlide => "LastSlide",
        SlideAction::EndShow => "EndShow",
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let kept: String = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect();
    format!("{kept}...")
}
