//! The help modal (PRD 14a §10): one table per base mode, sourced from that
//! mode's real keymap — no mode-blind list, no dead hints.
//!
//! Each mode's sections are data (`HelpSection` tables); the renderer lays
//! any of them out uniformly. A hint can only appear here if its key is
//! bound in the handler that serves that mode — the reader table comes from
//! `reader_events::handle_reader_key`, the sorter table from
//! `events::handle_sorter_key`, the slides table from the base handler.

use crate::app::mode::AppMode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// One help row: (keys, action) as bound in the mode's key handler.
type HelpRow = (&'static str, &'static str);

/// A titled block of help rows.
struct HelpSection {
    title: &'static str,
    rows: &'static [HelpRow],
}

/// The reader keymap (`reader_events::handle_reader_key`).
const READER_HELP: &[HelpSection] = &[
    HelpSection {
        title: "Reading:",
        rows: &[
            ("j / k, Down / Up", "continuous column scroll"),
            (
                "PgDn / PgUp, n / p",
                "next / previous row (Space, Enter, Backspace too)",
            ),
            ("Right / Left", "slide zoomed row / advance row at fit"),
            ("d / u", "half-page scroll"),
        ],
    },
    HelpSection {
        title: "Pan, Focus & Layout:",
        rows: &[
            ("h / l", "horizontal pan"),
            ("BackTab / w / \\", "cycle slot focus (the current page)"),
            ("2 / H / !", "toggle Spread-Single / Horizontal / cover-alone"),
        ],
    },
    HelpSection {
        title: "Fit & Zoom:",
        rows: &[
            ("f", "cycle fit mode (page / width)"),
            ("+ / -", "zoom in / out — the last - opens the sorter"),
            ("= / Z", "reset view / apply zoom to every page"),
        ],
    },
    HelpSection {
        title: "Jump, Views & Exit:",
        rows: &[
            ("[0-9] + Enter", "jump to page number"),
            ("g / G, Home / End", "first / last page"),
            ("Tab / t / s", "page sorter grid"),
            ("/  ·  Ctrl+F", "TOC search · find in document"),
            ("b", "blank screen"),
            ("?", "toggle help"),
            ("q / Esc, Ctrl-C", "quit"),
        ],
    },
];

/// The slide-deck keymap (the base handler in `events`).
const SLIDES_HELP: &[HelpSection] = &[
    HelpSection {
        title: "Navigation:",
        rows: &[
            ("Space, Right, Down, PgDn, n, j", "next slide"),
            ("Backspace, Left, Up, PgUp, k, P", "previous slide"),
            ("+ / - / =", "zoom in / out / reset (arrows pan zoomed)"),
        ],
    },
    HelpSection {
        title: "Jump to Slide:",
        rows: &[
            ("[0-9] + Enter", "jump to slide number"),
            ("g / G", "first / last slide"),
            ("/  ·  Ctrl+F", "TOC search · find in document"),
        ],
    },
    HelpSection {
        title: "Views & Overlays:",
        rows: &[
            ("p", "toggle presenter mode"),
            ("Tab, s", "toggle slide sorter grid"),
            ("c", "toggle reviewer comments"),
            ("W", "toggle fidelity warnings"),
            ("m", "play current slide media"),
            ("o", "open slide hyperlink picker"),
            ("t", "toggle minimal status bar"),
            ("b / w", "blank screen (black / white)"),
        ],
    },
    HelpSection {
        title: "Help & Exit:",
        rows: &[("? / h", "toggle help"), ("q / Esc, Ctrl-C", "quit presentation")],
    },
];

/// The sorter keymap (`events::handle_sorter_key`).
const SORTER_HELP: &[HelpSection] = &[HelpSection {
    title: "Page Sorter:",
    rows: &[
        ("Arrows / h j k l", "move the selection"),
        ("Enter", "open the selected page"),
        ("Esc / Tab / s", "return without selecting"),
        ("PgUp / PgDn, Home / End", "page / jump the selection"),
        ("m / c", "slide media / reviewer comments"),
        ("/  ·  Ctrl+F", "TOC search · find in document"),
        ("?", "toggle help"),
        ("q / Esc", "quit"),
    ],
}];

/// The help table for the base view that opened the modal.
fn sections_for(base: AppMode) -> &'static [HelpSection] {
    match base {
        AppMode::Reader => READER_HELP,
        AppMode::Sorter => SORTER_HELP,
        _ => SLIDES_HELP,
    }
}

/// Render a centered help popup modal with the keybindings of the base
/// view that opened it.
pub fn render_help_modal(f: &mut Frame, area: Rect, base: AppMode) {
    let sections = sections_for(base);
    let body_rows: usize = sections
        .iter()
        .map(|s| 1 + s.rows.len() + 1) // header + rows + blank separator
        .sum();
    let popup_width = 72.min(area.width.saturating_sub(4));
    let popup_height = ((body_rows + 2).min(34) as u16).min(area.height.saturating_sub(2));

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
        .border_style(
            Style::default()
                .bg(Color::Black)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let header_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    // Base style for every cell the paragraph covers: an opaque background so
    // the slide image, placed below the terminal's cell-background layer,
    // never bleeds through the overlay.
    // Span styles patch on top (fg-only spans keep this bg).
    let body_style = Style::default().bg(Color::Black);

    let mut text = Vec::new();
    for section in sections {
        text.push(Line::from(vec![Span::styled(section.title, header_style)]));
        for &(keys, action) in section.rows {
            text.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(keys, key_style),
                Span::styled(format!(" : {action}"), desc_style),
            ]));
        }
        text.push(Line::from(""));
    }

    let p = Paragraph::new(text).block(block).style(body_style);
    f.render_widget(p, popup_area);
}
