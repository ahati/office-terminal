use super::*;
use crate::app::bindings::{FrameBindings, SlidePlacement};
use crossterm::event::{KeyEventKind, KeyEventState};

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
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

#[test]
fn test_navigation_next_keys() {
    let next_keys = [
        KeyCode::Char(' '),
        KeyCode::Right,
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::Char('n'),
        KeyCode::Char('j'),
    ];

    for code in next_keys {
        let mut state = PresentationState::new(10, vec![]);
        let action = handle_key_event(make_key(code), &mut state);
        assert_eq!(action, AppAction::SlideChanged, "Key {:?} should advance slide", code);
        assert_eq!(state.current_slide(), 1);
    }
}

#[test]
fn test_navigation_prev_keys() {
    let prev_keys = [
        KeyCode::Left,
        KeyCode::Up,
        KeyCode::Backspace,
        KeyCode::PageUp,
        KeyCode::Char('k'),
        KeyCode::Char('P'),
    ];

    for code in prev_keys {
        let mut state = PresentationState::new(10, vec![]);
        state.set_current_slide(5);
        let action = handle_key_event(make_key(code), &mut state);
        assert_eq!(
            action,
            AppAction::SlideChanged,
            "Key {:?} should go to previous slide",
            code
        );
        assert_eq!(state.current_slide(), 4);
    }
}

#[test]
fn test_jump_and_digits() {
    let mut state = PresentationState::new(10, vec![]);
    handle_key_event(make_key(KeyCode::Char('3')), &mut state);
    handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(state.current_slide(), 2);

    handle_key_event(make_key(KeyCode::Char('g')), &mut state);
    assert_eq!(state.current_slide(), 0);

    handle_key_event(make_key(KeyCode::Char('G')), &mut state);
    assert_eq!(state.current_slide(), 9);
}

#[test]
fn test_toggle_presenter_and_help() {
    let mut state = PresentationState::new(5, vec![]);
    let action = handle_key_event(make_key(KeyCode::Char('p')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    let action2 = handle_key_event(make_key(KeyCode::Char('?')), &mut state);
    assert_eq!(action2, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::HelpModal);

    let action3 = handle_key_event(make_key(KeyCode::Esc), &mut state);
    assert_eq!(action3, AppAction::Redraw);
    assert_eq!(state.mode(), AppMode::PresenterConsole);
}

#[test]
fn test_quit_events() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(
        handle_key_event(make_key(KeyCode::Char('q')), &mut state),
        AppAction::Quit
    );
    assert_eq!(handle_key_event(make_key(KeyCode::Esc), &mut state), AppAction::Quit);
    assert_eq!(handle_key_event(make_ctrl_key('c'), &mut state), AppAction::Quit);
}

// ─── Slide-mode zoom keys (factor, reset, pan-while-zoomed) ─────────────────

/// Frame whose recorded placement bounds a zoomed view (zoom 2 on 80x24).
fn zoomed_frame_bindings() -> FrameBindings {
    let fit = crate::widget::zoom_fit::compute_zoomed_slide_fit(
        ratatui::layout::Rect::new(0, 0, 80, 24),
        16.0 / 9.0,
        crate::widget::layout::DEFAULT_CELL_ASPECT_RATIO,
        2.0,
        0.0,
        0.0,
    );
    FrameBindings {
        current: Some(SlidePlacement {
            rect: office_terminal_display::CellRect {
                row: 0,
                col: 0,
                rows: 24,
                cols: 80,
            },
            crop: fit.crop,
            zoom: 2.0,
            overflow_cols: fit.overflow_cols,
            overflow_rows: fit.overflow_rows,
        }),
        ..FrameBindings::default()
    }
}

#[test]
fn plus_and_minus_step_the_scale_and_equals_resets() {
    let mut state = PresentationState::new(1, vec![None]);
    assert_eq!(
        handle_key_event(make_key(KeyCode::Char('+')), &mut state),
        AppAction::ScaleChanged
    );
    assert!((state.scale() - 1.1).abs() < 1e-6);

    assert_eq!(
        handle_key_event(make_key(KeyCode::Char('-')), &mut state),
        AppAction::ScaleChanged
    );
    assert!((state.scale() - 1.0).abs() < 1e-6);

    // `=` is the reset (matching reader mode), not a second zoom-in key.
    state.set_scale(2.0);
    assert_eq!(
        handle_key_event(make_key(KeyCode::Char('=')), &mut state),
        AppAction::ScaleChanged
    );
    assert!((state.scale() - 1.0).abs() < 1e-6);
    assert_eq!(state.zoom_pan(), (0.0, 0.0), "reset also re-centers the view");
}

#[test]
fn arrows_navigate_when_the_view_is_not_zoomed() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_current_slide(5);
    assert_eq!(
        handle_key_event(make_key(KeyCode::Right), &mut state),
        AppAction::SlideChanged
    );
    assert_eq!(state.current_slide(), 6);
}

#[test]
fn arrows_pan_instead_of_navigating_while_zoomed() {
    let mut state = PresentationState::new(10, vec![]);
    state.set_current_slide(5);
    state.set_scale(2.0);
    state.publish_frame_bindings(zoomed_frame_bindings());

    for code in [KeyCode::Right, KeyCode::Left, KeyCode::Down, KeyCode::Up] {
        let action = handle_key_event(make_key(code), &mut state);
        assert_eq!(action, AppAction::Redraw, "{code:?} pans while zoomed");
        assert_eq!(state.current_slide(), 5, "{code:?} must not change slides");
    }

    // Each direction moved (Right then Left returned to the start, ditto
    // Down/Up, so probe the net state per axis with single presses).
    let pan_x_before = state.zoom_pan().0;
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(state.zoom_pan().0 > pan_x_before, "Right pans right");

    let pan_y_before = state.zoom_pan().1;
    handle_key_event(make_key(KeyCode::Down), &mut state);
    assert!(state.zoom_pan().1 > pan_y_before, "Down pans down");

    // Clamped at the bound the key reports None, exactly like navigation.
    state.publish_frame_bindings(zoomed_frame_bindings());
    for _ in 0..50 {
        let action = handle_key_event(make_key(KeyCode::Right), &mut state);
        state.publish_frame_bindings(zoomed_frame_bindings());
        if action == AppAction::None {
            break;
        }
    }
    assert_eq!(
        handle_key_event(make_key(KeyCode::Right), &mut state),
        AppAction::None,
        "at the right bound Right is a no-op"
    );

    // Navigation still works while zoomed through the non-arrow keys.
    assert_eq!(
        handle_key_event(make_key(KeyCode::PageDown), &mut state),
        AppAction::SlideChanged
    );
    assert_eq!(state.current_slide(), 6);
}

#[test]
fn zooming_back_to_the_fit_recenters_the_view() {
    let mut state = PresentationState::new(1, vec![None]);
    state.set_scale(2.0);
    state.publish_frame_bindings(zoomed_frame_bindings());
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert!(state.zoom_pan() != (0.0, 0.0));

    // Two ` -` steps: 2.0 -> 1.5 -> 1.25... then walk down to the fit.
    while state.is_zoomed() {
        handle_key_event(make_key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.zoom_pan(), (0.0, 0.0), "unzoomed view is re-centered");
}
