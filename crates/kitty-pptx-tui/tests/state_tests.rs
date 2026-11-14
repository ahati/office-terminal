use kitty_pptx_tui::app::state::{AppMode, BlankMode, PresentationState};

#[test]
fn test_navigation_clamping() {
    let notes = vec![Some("Note 1".into()), None, Some("Note 3".into())];
    let mut state = PresentationState::new(3, notes);

    assert_eq!(state.current_slide(), 0);
    assert!(!state.prev_slide());
    assert_eq!(state.current_slide(), 0);

    assert!(state.next_slide());
    assert_eq!(state.current_slide(), 1);

    assert!(state.next_slide());
    assert_eq!(state.current_slide(), 2);

    // Clamps at last slide
    assert!(!state.next_slide());
    assert_eq!(state.current_slide(), 2);

    // Jump to first
    assert!(state.first_slide());
    assert_eq!(state.current_slide(), 0);

    // Jump to last
    assert!(state.last_slide());
    assert_eq!(state.current_slide(), 2);
}

#[test]
fn test_digit_jump() {
    let mut state = PresentationState::new(10, vec![]);
    state.push_digit('5');
    assert_eq!(state.digit_buffer(), "5");
    assert!(state.commit_digits());
    assert_eq!(state.current_slide(), 4); // slide 5 is 0-indexed 4
    assert_eq!(state.digit_buffer(), "");

    // Jump beyond max clamps to total - 1
    state.push_digit('9');
    state.push_digit('9');
    assert!(state.commit_digits());
    assert_eq!(state.current_slide(), 9);
}

#[test]
fn test_mode_toggles() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(state.mode(), AppMode::Fullscreen);

    state.toggle_presenter_mode();
    assert_eq!(state.mode(), AppMode::PresenterConsole);

    state.toggle_presenter_mode();
    assert_eq!(state.mode(), AppMode::Fullscreen);

    state.toggle_help();
    assert_eq!(state.mode(), AppMode::HelpModal);

    state.toggle_help();
    assert_eq!(state.mode(), AppMode::Fullscreen);
}

#[test]
fn test_blank_screen_toggle() {
    let mut state = PresentationState::new(5, vec![]);
    assert_eq!(state.blank_mode(), BlankMode::None);

    state.toggle_blank(BlankMode::Black);
    assert_eq!(state.blank_mode(), BlankMode::Black);
    assert!(state.is_blank());

    state.toggle_blank(BlankMode::Black);
    assert_eq!(state.blank_mode(), BlankMode::None);

    state.toggle_blank(BlankMode::White);
    assert_eq!(state.blank_mode(), BlankMode::White);

    state.unblank();
    assert_eq!(state.blank_mode(), BlankMode::None);
}

#[test]
fn test_zoom_fit_uses_the_slide_aspect() {
    let mut state = PresentationState::new(3, vec![None, None, None]);
    state.set_slide_dimensions(9_144_000, 6_858_000); // 960 x 720 px at 9525 EMU/px
    assert!(state.fit_width(480.0));
    assert!((state.scale() - 0.5).abs() < 1e-5);
    state.set_scale(1.0);
    assert!(state.fit_page(960.0, 360.0));
    assert!((state.scale() - 0.5).abs() < 1e-5);
    assert!(!state.fit_width(0.0), "unlaid-out container defers");
}

#[test]
fn test_zoom_steppers_and_change_gate() {
    let mut state = PresentationState::new(1, vec![None]);
    assert!(state.nav_mut().zoom_in());
    assert!((state.scale() - 1.1).abs() < 1e-6);
    assert!(state.nav_mut().zoom_out());
    assert!((state.scale() - 1.0).abs() < 1e-6);
    let changes = state.scale_changes();
    assert!(!state.set_scale(1.0));
    assert_eq!(state.scale_changes(), changes);
}
