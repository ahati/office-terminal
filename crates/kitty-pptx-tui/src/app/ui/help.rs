use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Render a centered help popup modal displaying all keybindings.
pub fn render_help_modal(f: &mut Frame, area: Rect) {
    let popup_width = 68.min(area.width.saturating_sub(4));
    let popup_height = 20.min(area.height.saturating_sub(2));

    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(popup_width) / 2),
            Constraint::Length(popup_width),
            Constraint::Min(0),
        ])
        .split(area);

    let popup_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(popup_height) / 2),
            Constraint::Length(popup_height),
            Constraint::Min(0),
        ])
        .split(horiz[1])[1];

    // Clear background behind popup
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Keyboard Shortcuts & Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let header_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);

    let text = vec![
        Line::from(vec![Span::styled("Navigation:", header_style)]),
        Line::from(vec![
            Span::styled("  Space, Right, Down, PgDn, n, j ", key_style),
            Span::styled(": Next slide", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Backspace, Left, Up, PgUp, k, P", key_style),
            Span::styled(": Previous slide", desc_style),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Jump to Slide:", header_style)]),
        Line::from(vec![
            Span::styled("  [0-9] + Enter                  ", key_style),
            Span::styled(": Jump to slide number", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  g                              ", key_style),
            Span::styled(": First slide", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  G                              ", key_style),
            Span::styled(": Last slide", desc_style),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Presentation Views & Modes:", header_style)]),
        Line::from(vec![
            Span::styled("  p                              ", key_style),
            Span::styled(": Toggle Presenter Mode", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  t                              ", key_style),
            Span::styled(": Toggle Minimal Status Bar", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  b / w                          ", key_style),
            Span::styled(": Blank Screen (Black / White)", desc_style),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Help & Exit:", header_style)]),
        Line::from(vec![
            Span::styled("  ?, h                           ", key_style),
            Span::styled(": Toggle Help", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  q, Esc, Ctrl-C                 ", key_style),
            Span::styled(": Quit Presentation", desc_style),
        ]),
    ];

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup_area);
}
