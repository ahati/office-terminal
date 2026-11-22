//! The help modal is mode-aware (PRD 14a §10): it renders the keybindings
//! of the base view that opened it, sourced from that mode's real keymap —
//! no mode-blind list, no dead hints.

use office_terminal_tui::app::mode::AppMode;
use office_terminal_tui::app::ui::render_help_modal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

/// The modal's rendered text, one line per row.
fn rendered_lines(base: AppMode) -> Vec<String> {
    let area = Rect::new(0, 0, 100, 40);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|f| render_help_modal(f, area, base))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut lines = Vec::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

fn text_of(base: AppMode) -> String {
    rendered_lines(base).join("\n")
}

#[test]
fn reader_help_lists_only_reader_bindings() {
    let text = text_of(AppMode::Reader);
    // The reader's own bindings, described as reader actions.
    assert!(text.contains("continuous column scroll"));
    assert!(text.contains("page sorter grid"));
    assert!(text.contains("zoom in / out — the last - opens the sorter"));
    assert!(text.contains("cycle slot focus"));
    // The slides-only keys the old mode-blind list advertised.
    assert!(!text.contains("presenter mode"), "p is not a reader key");
    assert!(!text.contains("slide media"), "m is not a reader key");
    assert!(!text.contains("hyperlink picker"), "o is not a reader key");
    assert!(!text.contains("? / h"), "h pans in reader mode; only ? opens help");
}

#[test]
fn slides_help_keeps_its_own_bindings() {
    let text = text_of(AppMode::Fullscreen);
    assert!(text.contains("next slide"));
    assert!(text.contains("presenter mode"));
    assert!(text.contains("? / h"), "h does toggle help in slides mode");
    // The Ctrl+F mislabel is gone: TOC search and find are separate rows.
    assert!(text.contains("TOC search · find in document"));
    assert!(!text.contains("Fuzzy TOC Search"), "Ctrl+F was never TOC search");
    // Reader rows never leak into the slides view.
    assert!(!text.contains("continuous column scroll"));
}

#[test]
fn sorter_help_lists_the_grid_bindings() {
    let text = text_of(AppMode::Sorter);
    assert!(text.contains("open the selected page"));
    assert!(text.contains("return without selecting"));
    // Slides and reader rows never leak into the sorter view.
    assert!(!text.contains("zoom in / out"));
    assert!(!text.contains("next slide"));
}

#[test]
fn every_mode_lists_help_and_quit() {
    for mode in [AppMode::Reader, AppMode::Fullscreen, AppMode::Sorter] {
        let text = text_of(mode);
        assert!(text.contains("toggle help"), "{mode:?} lists the help key");
        assert!(text.contains("quit"), "{mode:?} lists the quit key");
    }
}
