use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use pptx_model::Slide;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::link_state::handle_mouse_event;
use kitty_pptx_tui::app::mode::AppMode;
use kitty_pptx_tui::app::state::PresentationState;
use kitty_pptx_tui::app::ui::render_app;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl_key(ch: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(ch),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn make_slide(
    index: usize,
    part: &str,
    hyperlink: Option<&str>,
    action: Option<&str>,
    full_bleed: bool,
) -> Slide {
    let mut shape = serde_json::json!({
        "type": "shape",
        "x": 0,
        "y": 0,
        "width": if full_bleed { 12_192_000 } else { 4_000_000 },
        "height": if full_bleed { 6_858_000 } else { 2_000_000 },
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect"
    });
    if let Some(link) = hyperlink {
        shape["hyperlink"] = serde_json::Value::String(link.to_string());
    }
    if let Some(action) = action {
        shape["hyperlinkAction"] = serde_json::Value::String(action.to_string());
    }
    serde_json::from_value(serde_json::json!({
        "index": index,
        "slideNumber": index + 1,
        "partName": part,
        "elements": [shape],
        "elementSources": [],
        "comments": []
    }))
    .expect("slide JSON")
}

fn state_with_external_link() -> PresentationState {
    let mut state = PresentationState::new(3, vec![]);
    state.set_slide_dimensions(12_192_000, 6_858_000);
    state.set_layout_dimensions(80, 24);
    state.set_slides(vec![
        make_slide(
            0,
            "ppt/slides/slide1.xml",
            Some("https://www.rust-lang.org"),
            None,
            true,
        ),
        make_slide(1, "ppt/slides/slide2.xml", None, None, false),
        make_slide(2, "ppt/slides/slide3.xml", None, None, false),
    ]);
    state
}

#[test]
fn link_picker_opens_from_o_and_ctrl_o_and_closes_with_esc() {
    let mut state = state_with_external_link();
    assert_eq!(state.current_slide_link_count(), 1);

    assert_eq!(handle_key_event(key(KeyCode::Char('o')), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::LinkPickerModal);
    assert_eq!(state.link_picker().len(), 1);
    assert_eq!(state.previous_mode(), AppMode::Fullscreen);

    assert_eq!(handle_key_event(key(KeyCode::Esc), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    assert_eq!(handle_key_event(ctrl_key('o'), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::LinkPickerModal);
    assert_eq!(handle_key_event(ctrl_key('o'), &mut state), AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn quick_key_activates_external_link_and_closes_modal() {
    let mut state = state_with_external_link();
    handle_key_event(key(KeyCode::Char('o')), &mut state);
    let action = handle_key_event(key(KeyCode::Char('1')), &mut state);
    assert_eq!(
        action,
        AppAction::OpenExternal("https://www.rust-lang.org".to_string())
    );
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn quick_key_internal_jump_navigates_to_slide_index() {
    let mut state = PresentationState::new(3, vec![]);
    state.set_slides(vec![
        make_slide(
            0,
            "ppt/slides/slide1.xml",
            Some("../slides/slide3.xml"),
            Some("ppaction://hlinksldjump"),
            false,
        ),
        make_slide(1, "ppt/slides/slide2.xml", None, None, false),
        make_slide(2, "ppt/slides/slide3.xml", None, None, false),
    ]);
    handle_key_event(key(KeyCode::Char('o')), &mut state);
    let action = handle_key_event(key(KeyCode::Char('1')), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn enter_activates_selected_action_and_end_show_quits() {
    let mut state = PresentationState::new(3, vec![]);
    state.set_slides(vec![
        make_slide(
            0,
            "ppt/slides/slide1.xml",
            None,
            Some("ppaction://hlinkshowjump?jump=nextslide"),
            false,
        ),
        make_slide(1, "ppt/slides/slide2.xml", None, None, false),
        make_slide(2, "ppt/slides/slide3.xml", None, None, false),
    ]);
    handle_key_event(key(KeyCode::Char('o')), &mut state);
    assert_eq!(handle_key_event(key(KeyCode::Enter), &mut state), AppAction::SlideChanged);
    assert_eq!(state.current_slide(), 1);

    let mut end_state = PresentationState::new(3, vec![]);
    end_state.set_slides(vec![
        make_slide(
            0,
            "ppt/slides/slide1.xml",
            None,
            Some("ppaction://hlinkshowjump?jump=endshow"),
            false,
        ),
        make_slide(1, "ppt/slides/slide2.xml", None, None, false),
        make_slide(2, "ppt/slides/slide3.xml", None, None, false),
    ]);
    handle_key_event(key(KeyCode::Char('o')), &mut end_state);
    assert_eq!(handle_key_event(key(KeyCode::Enter), &mut end_state), AppAction::Quit);
}

#[test]
fn mouse_click_hits_link_and_miss_advances_slide() {
    let mut state = state_with_external_link();
    state.set_layout_dimensions(80, 24);

    // The link is full-bleed: the center of the fitted slide must hit it.
    let action = handle_mouse_event(click(40, 11), &mut state);
    assert_eq!(
        action,
        AppAction::OpenExternal("https://www.rust-lang.org".to_string())
    );
    assert_eq!(state.current_slide(), 0);

    // Clicks outside the fitted slide rect fall through to next-slide.
    let mut miss_state = PresentationState::new(3, vec![]);
    miss_state.set_layout_dimensions(80, 24);
    let action = handle_mouse_event(click(79, 23), &mut miss_state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(miss_state.current_slide(), 1);
}

#[test]
fn non_left_mouse_events_are_ignored() {
    let mut state = state_with_external_link();
    let mut event = click(40, 11);
    event.kind = MouseEventKind::Moved;
    assert_eq!(handle_mouse_event(event, &mut state), AppAction::None);
    assert_eq!(state.current_slide(), 0);
}

#[test]
fn link_picker_modal_renders_in_smoke_test() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut state = state_with_external_link();
    state.open_link_picker();
    terminal
        .draw(|frame| render_app(&state, frame))
        .expect("render link picker");
    assert_eq!(state.mode(), AppMode::LinkPickerModal);
}
