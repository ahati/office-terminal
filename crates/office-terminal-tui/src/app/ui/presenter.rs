use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};

use crate::app::bindings::FrameBindings;
use crate::app::state::PresentationState;
use crate::app::ui::{current_slide_widget, preview_slide_widget, render_render_failure_note};
use crate::widget::layout::DEFAULT_CELL_ASPECT_RATIO;
use crate::widget::zoom_fit::compute_zoomed_slide_fit;

/// Render the split-pane presenter mode console.
pub fn render_presenter(
    state: &PresentationState,
    f: &mut Frame,
    area: Rect,
    bindings: &mut FrameBindings,
) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    let main_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(vert[0]);

    // 1. Left pane: Current slide (large)
    let current_slide_area = main_split[0];
    let slide_block = Block::default()
        .title(format!(" Current Slide ({}/{}) ", state.current_slide() + 1, state.total_slides()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner_slide_area = slide_block.inner(current_slide_area);
    f.render_widget(slide_block, current_slide_area);

    let current_fit = compute_zoomed_slide_fit(
        inner_slide_area,
        state.aspect_ratio(),
        DEFAULT_CELL_ASPECT_RATIO,
        state.scale(),
        state.zoom_pan().0,
        state.zoom_pan().1,
    );
    bindings.record_current_fit(&current_fit, state.scale());
    let slide_widget = current_slide_widget(state).with_fit(current_fit);
    f.render_widget(slide_widget, inner_slide_area);
    render_render_failure_note(state, f, inner_slide_area);

    // 2. Right pane: Next slide preview + Speaker notes
    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(main_split[1]);

    // Right top: Next slide preview thumbnail
    let next_preview_area = right_split[0];
    let preview_title = match state.next_slide_index() {
        Some(idx) => format!(" Next Slide ({}/{}) ", idx + 1, state.total_slides()),
        None => " Next Slide (End of Deck) ".to_string(),
    };
    let preview_block = Block::default()
        .title(preview_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner_preview_area = preview_block.inner(next_preview_area);
    f.render_widget(preview_block, next_preview_area);

    if let Some(next_id) = state.next_image_id() {
        let preview_fit = compute_zoomed_slide_fit(
            inner_preview_area,
            state.aspect_ratio(),
            DEFAULT_CELL_ASPECT_RATIO,
            1.0,
            0.0,
            0.0,
        );
        bindings.record_preview_fit(&preview_fit);
        let next_widget = preview_slide_widget(state, next_id).with_fit(preview_fit);
        f.render_widget(next_widget, inner_preview_area);
    } else {
        let end_msg = Paragraph::new("End of presentation")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(end_msg, inner_preview_area);
    }

    // Right bottom: Speaker notes
    let notes_area = right_split[1];
    let notes_block = Block::default()
        .title(" Speaker Notes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner_notes_area = notes_block.inner(notes_area);
    f.render_widget(notes_block, notes_area);

    let notes_text = state.current_notes()
        .unwrap_or("No speaker notes for this slide.");
    let notes_widget = Paragraph::new(notes_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));
    f.render_widget(notes_widget, inner_notes_area);

    // 3. Bottom bar: Timers, slide indicator, and progress gauge
    let bottom_area = vert[1];
    let bottom_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(bottom_area);

    let timer_line = if !state.digit_buffer().is_empty() {
        Line::from(vec![
            Span::styled(
                format!(" Jump to: {}_ ", state.digit_buffer()),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Enter] Go | [Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let mut spans = vec![
            Span::styled(" PRESENTER ", Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)),
            Span::raw(format!(" Slide {} / {} | ", state.current_slide() + 1, state.total_slides())),
            Span::styled(format!("Elapsed: {} ", state.elapsed_formatted()), Style::default().fg(Color::Green)),
            Span::raw("| "),
            Span::styled(format!("Clock: {} ", state.wall_clock_formatted()), Style::default().fg(Color::Cyan)),
            Span::raw("| "),
        ];

        if state.current_slide_has_media() {
            spans.push(Span::styled("[m] Media | ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));
        }

        if state.current_slide_link_count() > 0 {
            spans.push(Span::styled(
                format!("[o: {} Links] | ", state.current_slide_link_count()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
        }

        if let Some(toast) = state.toast() {
            spans.push(Span::styled(format!("📢 {} | ", toast), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        }

        spans.push(Span::styled("[p] Fullscreen | [Tab/s] Sorter | [?] Help | [b/w] Blank | [q] Quit", Style::default().fg(Color::DarkGray)));
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(timer_line), bottom_split[0]);

    // Progress gauge
    let ratio = state.progress_ratio();
    let gauge_label = format!("{}/{} ({:.0}%)", state.current_slide() + 1, state.total_slides(), ratio * 100.0);
    let gauge = Gauge::default()
        .ratio(ratio)
        .label(gauge_label)
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray));
    f.render_widget(gauge, bottom_split[1]);
}
