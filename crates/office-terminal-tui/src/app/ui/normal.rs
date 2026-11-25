use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::bindings::FrameBindings;
use crate::app::state::PresentationState;
use crate::app::ui::{current_slide_widget, render_render_failure_note};
use crate::widget::layout::DEFAULT_CELL_ASPECT_RATIO;
use crate::widget::zoom_fit::compute_zoomed_slide_fit;

/// Render the standard fullscreen presentation mode.
pub fn render_normal(state: &PresentationState, f: &mut Frame, area: Rect, bindings: &mut FrameBindings) {
    let canvas_area = if !state.status_line_visible() {
        area
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);
        render_status_bar(state, f, chunks[1]);
        chunks[0]
    };

    let fit = compute_zoomed_slide_fit(
        canvas_area,
        state.aspect_ratio(),
        DEFAULT_CELL_ASPECT_RATIO,
        state.scale(),
        state.zoom_pan().0,
        state.zoom_pan().1,
    );
    bindings.record_current_fit(&fit, state.scale());
    f.render_widget(current_slide_widget(state).with_fit(fit), canvas_area);
    render_render_failure_note(state, f, canvas_area);
}

fn render_status_bar(state: &PresentationState, f: &mut Frame, area: Rect) {
    let status_line = if !state.digit_buffer().is_empty() {
        Line::from(vec![
            Span::styled(
                format!(" Jump to slide: {}_ ", state.digit_buffer()),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[Enter] Jump | [Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let mut spans = Vec::new();
        if let Some(toast) = state.toast() {
            spans.push(Span::styled(
                format!(" 📢 {} | ", toast),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }

        let media_badge = if state.current_slide_has_media() {
            " | [m] Play Media"
        } else {
            ""
        };
        let link_badge = if state.current_slide_link_count() > 0 {
            format!(" | [o: {} Links]", state.current_slide_link_count())
        } else {
            String::new()
        };

        let status_text = format!(
            " Slide {} / {}{}{} | Elapsed: {} | [?] Help | [p] Presenter | [Tab/s] Sorter | [t] Hide Bar | [q] Quit ",
            state.current_slide() + 1,
            state.total_slides(),
            media_badge,
            link_badge,
            state.elapsed_formatted()
        );
        spans.push(Span::styled(
            status_text,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        Line::from(spans)
    };

    let status = Paragraph::new(status_line).block(Block::default().borders(Borders::NONE));
    f.render_widget(status, area);
}
