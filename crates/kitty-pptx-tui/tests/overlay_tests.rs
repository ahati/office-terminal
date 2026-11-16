use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_core::{FidelityClass, WarningEvent};
use pptx_model::Slide;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use kitty_pptx_tui::app::mode::AppMode;
use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::state::PresentationState;
use kitty_pptx_tui::app::ui::render_app;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn make_shift_key(ch: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(ch),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn make_ctrl_key(ch: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(ch),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn create_sample_slide(index: usize, title: &str, comment_text: Option<&str>) -> Slide {
    let comments_json = match comment_text {
        Some(text) => format!(
            r#"[
                {{
                    "authorId": 1,
                    "modernAuthorId": "auth-1",
                    "id": "c1",
                    "index": 1,
                    "author": "Reviewer Alice",
                    "date": "2026-11-10T12:00:00Z",
                    "status": "active",
                    "text": "{text}",
                    "replies": [
                        {{
                            "id": "r1",
                            "authorId": "auth-2",
                            "author": "Bob",
                            "date": "2026-11-10T12:05:00Z",
                            "text": "Addressed in v2"
                        }}
                    ]
                }}
            ]"#
        ),
        None => "[]".to_string(),
    };

    let slide_num = index + 1;
    let json = format!(
        r#"{{
            "index": {index},
            "slideNumber": {slide_num},
            "elements": [
                {{
                    "type": "shape",
                    "x": 0, "y": 0, "width": 1000, "height": 200,
                    "rotation": 0.0, "flipH": false, "flipV": false,
                    "geometry": "rect",
                    "placeholderType": "title",
                    "textBody": {{
                        "verticalAnchor": "top",
                        "wrap": "square",
                        "vert": "horz",
                        "autoFit": "none",
                        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
                        "paragraphs": [
                            {{
                                "alignment": "left",
                                "marL": 0, "marR": 0, "indent": 0,
                                "lvl": 0,
                                "bullet": {{ "type": "none" }},
                                "rtl": false,
                                "eaLnBrk": false,
                                "tabStops": [],
                                "runs": [
                                    {{
                                        "type": "text",
                                        "text": "{title}",
                                        "bold": false,
                                        "italic": false,
                                        "underline": false,
                                        "strikethrough": false,
                                        "strikeDouble": false,
                                        "hyperlinkUsesTextFill": false,
                                        "fieldType": null
                                    }}
                                ]
                            }}
                        ]
                    }}
                }}
            ],
            "elementSources": [],
            "comments": {comments_json}
        }}"#
    );

    serde_json::from_str(&json).expect("Slide JSON desers correctly")
}

#[test]
fn test_comments_drawer_toggling_and_dismiss() {
    let mut state = PresentationState::new(3, vec![]);
    let slide0 = create_sample_slide(0, "Intro", Some("Check font size"));
    state.set_slides(vec![slide0]);

    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Toggle 'c' -> CommentsDrawer
    assert_eq!(handle_key_event(make_key(KeyCode::Char('c')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::CommentsDrawer);
    assert_eq!(state.previous_mode(), AppMode::Fullscreen);

    // Dismiss with 'q'
    assert_eq!(handle_key_event(make_key(KeyCode::Char('q')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Re-open in Presenter mode
    state.set_mode(AppMode::PresenterConsole);
    assert_eq!(handle_key_event(make_key(KeyCode::Char('c')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::CommentsDrawer);
    assert_eq!(state.previous_mode(), AppMode::PresenterConsole);

    // Dismiss with Esc
    assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    // Toggle 'c' while already open closes it
    assert_eq!(handle_key_event(make_key(KeyCode::Char('c')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::CommentsDrawer);
    assert_eq!(handle_key_event(make_key(KeyCode::Char('c')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);
}

#[test]
fn test_comments_drawer_scrolling() {
    let mut state = PresentationState::new(3, vec![]);
    state.open_comments();
    assert_eq!(state.comments_scroll(), 0);

    // Down / j increments scroll
    handle_key_event(make_key(KeyCode::Down), &mut state);
    assert_eq!(state.comments_scroll(), 1);
    handle_key_event(make_key(KeyCode::Char('j')), &mut state);
    assert_eq!(state.comments_scroll(), 2);

    // Up / k decrements scroll
    handle_key_event(make_key(KeyCode::Up), &mut state);
    assert_eq!(state.comments_scroll(), 1);
    handle_key_event(make_key(KeyCode::Char('k')), &mut state);
    assert_eq!(state.comments_scroll(), 0);

    // PageDown / PageUp
    handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(state.comments_scroll(), 5);
    handle_key_event(make_key(KeyCode::PageUp), &mut state);
    assert_eq!(state.comments_scroll(), 0);
}

#[test]
fn test_warning_drawer_toggling_and_events() {
    let mut state = PresentationState::new(3, vec![]);

    // Add diverse warnings
    state.add_warning(WarningEvent::FontSubstituted {
        requested: "Calibri".to_string(),
        substitute: "DejaVu Sans".to_string(),
    });
    state.add_warning(WarningEvent::UnsupportedPreset {
        name: "mathDivide".to_string(),
    });
    state.add_session_warning(WarningEvent::MediaFailed {
        rel_id: "rId5".to_string(),
        reason: "Codec not found".to_string(),
    });
    state.add_warning(WarningEvent::FidelityDegraded {
        element_id: "elem-9".to_string(),
        feature: "3D Extrusion",
        class: FidelityClass::Degraded,
    });
    state.add_session_warning(WarningEvent::ResourceLimitHit {
        limit: "Texture cache max 256MB",
    });

    assert_eq!(state.warnings().len(), 5);

    // Toggle via 'W'
    assert_eq!(handle_key_event(make_key(KeyCode::Char('W')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::WarningDrawer);

    // Scroll warnings drawer
    handle_key_event(make_key(KeyCode::Down), &mut state);
    assert_eq!(state.warnings_scroll(), 1);
    handle_key_event(make_key(KeyCode::Up), &mut state);
    assert_eq!(state.warnings_scroll(), 0);

    // Dismiss with 'W'
    assert_eq!(handle_key_event(make_key(KeyCode::Char('W')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Open via Shift-w and dismiss with Esc
    assert_eq!(handle_key_event(make_shift_key('w'), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::WarningDrawer);
    assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_toc_search_toggling_and_navigation() {
    let mut state = PresentationState::new(3, vec![]);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Open via '/'
    assert_eq!(handle_key_event(make_key(KeyCode::Char('/')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::TocSearchModal);

    // Dismiss with Esc
    assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // Ctrl+F opens the in-document find modal; `/` remains the TOC search.
    assert_eq!(handle_key_event(make_ctrl_key('f'), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::FindModal);

    // Dismiss with Ctrl+F
    assert_eq!(handle_key_event(make_ctrl_key('f'), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_toc_search_fuzzy_filtering_and_jumping() {
    let mut state = PresentationState::new(4, vec![
        Some("Financial goals and Q3 targets".to_string()),
        Some("Kitty graphics protocol implementation details".to_string()),
        Some("Raster caching and tiny-skia pipelines".to_string()),
        Some("Future milestones".to_string()),
    ]);

    let slides = vec![
        create_sample_slide(0, "Executive Overview", None),
        create_sample_slide(1, "Terminal Architecture", None),
        create_sample_slide(2, "Performance Benchmarks", None),
        create_sample_slide(3, "Roadmap 2027", None),
    ];
    state.set_slides(slides);

    // Open TOC Search
    handle_key_event(make_key(KeyCode::Char('/')), &mut state);
    assert_eq!(state.mode(), AppMode::TocSearchModal);

    // 1. Initial empty query matches all 4 slides
    assert_eq!(state.search_state().matched_indices().len(), 4);

    // 2. Search by slide number "3"
    handle_key_event(make_key(KeyCode::Char('3')), &mut state);
    assert_eq!(state.search_state().query(), "3");
    assert_eq!(state.search_state().selected_slide_index(), Some(2)); // Slide 3 is index 2

    // Jump with Enter
    let action = handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    // 3. Search by title substring "Terminal"
    handle_key_event(make_key(KeyCode::Char('/')), &mut state);
    for c in "term".chars() {
        handle_key_event(make_key(KeyCode::Char(c)), &mut state);
    }
    assert_eq!(state.search_state().query(), "term");
    assert_eq!(state.search_state().selected_slide_index(), Some(1)); // Terminal Architecture

    // Test Backspace
    handle_key_event(make_key(KeyCode::Backspace), &mut state);
    assert_eq!(state.search_state().query(), "ter");

    // Close search without jumping
    handle_key_event(make_key(KeyCode::Esc), &mut state);
    assert_eq!(state.current_slide(), 2); // Remained at slide 2

    // 4. Search by notes keyword "tiny-skia"
    handle_key_event(make_key(KeyCode::Char('/')), &mut state);
    for c in "skia".chars() {
        handle_key_event(make_key(KeyCode::Char(c)), &mut state);
    }
    assert_eq!(state.search_state().selected_slide_index(), Some(2));
    handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(state.current_slide(), 2);

    // 5. Up and Down cursor navigation in search results
    handle_key_event(make_key(KeyCode::Char('/')), &mut state);
    assert_eq!(state.search_state().matched_indices().len(), 4);
    assert_eq!(state.search_state().selected_cursor(), 2); // initialized to current_slide

    handle_key_event(make_key(KeyCode::Down), &mut state);
    assert_eq!(state.search_state().selected_cursor(), 3);
    handle_key_event(make_key(KeyCode::Up), &mut state);
    assert_eq!(state.search_state().selected_cursor(), 2);
}

#[test]
fn test_ui_rendering_smoke_across_overlays() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("Terminal creates");

    let mut state = PresentationState::new(3, vec![Some("Notes for slide 1".to_string())]);
    let slide0 = create_sample_slide(0, "Demo Slide", Some("Reviewer feedback"));
    state.set_slides(vec![slide0]);

    // 1. Render Normal view
    terminal.draw(|f| render_app(&state, f)).expect("Render normal");

    // 2. Render Comments Drawer overlay
    state.open_comments();
    terminal.draw(|f| render_app(&state, f)).expect("Render comments drawer");

    // 3. Render Warnings Drawer overlay
    state.open_warnings();
    state.add_warning(WarningEvent::UnsupportedPreset { name: "testPreset".to_string() });
    terminal.draw(|f| render_app(&state, f)).expect("Render warnings drawer");

    // 4. Render TOC Search modal overlay
    state.open_search();
    terminal.draw(|f| render_app(&state, f)).expect("Render search modal");
}

/// Regression test: overlays must be opaque edge-to-edge. The slide image is
/// placed below the terminal's cell-background layer, so any popup cell left
/// at the default background lets the slide bleed through and makes the text
/// unreadable.
#[test]
fn test_help_modal_opaque_edge_to_edge() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    let popup = Rect::new((100 - 70) / 2, (32 - 27) / 2, 70, 27);
    let backend = TestBackend::new(100, 32);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| {
            kitty_pptx_tui::app::ui::render_help_modal(f, f.area());
        })
        .expect("draw");

    let buf: Buffer = terminal.backend().buffer().clone();
    let area = buf.area;
    let mut opaque = 0usize;
    let mut total = 0usize;
    for y in 0..area.height {
        for x in 0..area.width {
            // The modal is 70x27 centered in 100x32; everything inside its
            // bounds (borders included) must have a non-default background.
            if popup.contains(ratatui::layout::Position { x, y }) {
                let cell = &buf[(x, y)];
                total += 1;
                if matches!(cell.bg, ratatui::style::Color::Rgb(_, _, _) | ratatui::style::Color::Black | ratatui::style::Color::Indexed(_)) {
                    opaque += 1;
                }
            }
        }
    }
    assert_eq!(opaque, total, "every help-modal cell must carry an opaque bg");
}

#[test]
fn test_content_find_jumps_to_matching_slide() {
    let mut state = PresentationState::new(2, vec![]);
    state.set_slides(vec![
        create_sample_slide(0, "Alpha Report", None),
        create_sample_slide(1, "Beta Findings", None),
    ]);

    handle_key_event(make_ctrl_key('f'), &mut state);
    assert_eq!(state.mode(), AppMode::FindModal);
    for ch in "Beta".chars() {
        handle_key_event(make_key(KeyCode::Char(ch)), &mut state);
    }
    assert_eq!(state.find_state().matches().len(), 1);
    assert_eq!(
        state.find_state().matches()[0].slide,
        1,
        "find indexes the second slide body"
    );
    assert_eq!(
        handle_key_event(make_key(KeyCode::Enter), &mut state),
        AppAction::SlideChanged
    );
    assert_eq!(state.current_slide(), 1);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn metafile_warnings_render_in_the_drawer() {
    let mut state = PresentationState::new(1, vec![]);
    state.set_slides(vec![create_sample_slide(0, "EMF slide", None)]);
    state.add_warning(WarningEvent::FidelityDegraded {
        element_id: "picture/ppt/media/image1.emf".to_string(),
        feature: "metafile_gradient",
        class: FidelityClass::Degraded,
    });
    state.open_warnings();

    let backend = TestBackend::new(160, 40);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| render_app(&state, frame))
        .expect("draw warnings drawer");
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("metafile_gradient"), "{text}");
    assert!(text.contains("image1.emf"), "{text}");
}
