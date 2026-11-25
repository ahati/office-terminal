use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use office_terminal_tui::app::events::{handle_key_event, AppAction};
use office_terminal_tui::app::media::{
    find_slide_media, has_slide_media, is_executable_in_path, try_play_current_slide_media, try_play_with_player,
};
use office_terminal_tui::app::state::PresentationState;
use pptx_model::{MediaElement, ShapeElement, Slide, SlideElement};

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn empty_slide() -> Slide {
    Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: None,
        background: None,
        elements: Vec::new(),
        element_sources: Vec::new(),
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

fn slide_with_shape() -> Slide {
    let mut slide = empty_slide();
    let shape_json = r#"{
        "x": 0, "y": 0, "width": 100, "height": 100,
        "rotation": 0.0, "flipH": false, "flipV": false,
        "geometry": "rect"
    }"#;
    let shape: ShapeElement = serde_json::from_str(shape_json).unwrap();
    slide.elements.push(SlideElement::Shape(shape));
    slide
}

fn slide_with_media(path: &str) -> Slide {
    let mut slide = empty_slide();
    slide.elements.push(SlideElement::Media(MediaElement {
        id: Some("media1".to_string()),
        x: 100,
        y: 100,
        width: 640,
        height: 360,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        media_kind: "video".to_string(),
        poster_path: "ppt/media/poster.png".to_string(),
        poster_mime_type: "image/png".to_string(),
        media_path: path.to_string(),
        mime_type: "video/mp4".to_string(),
    }));
    slide
}

#[test]
fn test_media_detection_empty_and_shapes() {
    let empty = empty_slide();
    assert!(!has_slide_media(&empty));
    assert!(find_slide_media(&empty).is_none());
    assert!(try_play_current_slide_media(&empty).is_none());

    let shape_slide = slide_with_shape();
    assert!(!has_slide_media(&shape_slide));
    assert!(find_slide_media(&shape_slide).is_none());
    assert!(try_play_current_slide_media(&shape_slide).is_none());
}

#[test]
fn test_media_detection_media_element() {
    let media_slide = slide_with_media("ppt/media/sample_video.mp4");
    assert!(has_slide_media(&media_slide));

    let media = find_slide_media(&media_slide).expect("Media element must be found");
    assert_eq!(media.media_kind, "video");
    assert_eq!(media.media_path, "ppt/media/sample_video.mp4");
    assert_eq!(media.mime_type, "video/mp4");

    // try_play_current_slide_media returns status message (either launched or no player installed)
    let play_res = try_play_current_slide_media(&media_slide);
    assert!(play_res.is_some());
    let msg = play_res.unwrap();
    assert!(
        msg.contains("media") || msg.contains("player") || msg.contains("mpv") || msg.contains("Playing"),
        "Status message should mention playback status: {}",
        msg
    );
}

#[test]
fn test_try_play_with_explicit_player() {
    let media_slide = slide_with_media("test_video.mp4");

    // Test with a definitely non-existent player
    let res = try_play_with_player(&media_slide, "nonexistent_media_player_xyz_9999");
    assert!(res.is_some());
    let inner = res.unwrap();
    assert!(inner.is_err(), "Non-existent player should return error");

    // Test with slide without media
    let empty = empty_slide();
    assert!(try_play_with_player(&empty, "mpv").is_none());

    // Test with empty media path and empty poster path
    let mut no_path_slide = empty_slide();
    no_path_slide.elements.push(SlideElement::Media(MediaElement {
        id: None,
        x: 0,
        y: 0,
        width: 10,
        height: 10,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        media_kind: "audio".to_string(),
        poster_path: String::new(),
        poster_mime_type: String::new(),
        media_path: String::new(),
        mime_type: "audio/mp3".to_string(),
    }));
    let empty_path_res = try_play_with_player(&no_path_slide, "mpv");
    assert!(empty_path_res.is_some());
    assert!(empty_path_res.unwrap().is_err());
}

#[test]
fn test_is_executable_in_path() {
    // Standard Unix commands that must exist in PATH on Linux
    assert!(is_executable_in_path("sh") || is_executable_in_path("bash") || is_executable_in_path("ls"));

    // Non-existent command
    assert!(!is_executable_in_path("this_executable_does_not_exist_at_all_12345"));
}

#[test]
fn test_media_key_event_and_toast() {
    let slides = vec![empty_slide(), slide_with_media("ppt/media/clip.mp4")];
    let mut state = PresentationState::new(2, vec![]).with_slides(slides);

    // Slide 0: No media
    assert_eq!(state.current_slide(), 0);
    assert!(!state.current_slide_has_media());

    let action = handle_key_event(make_key(KeyCode::Char('m')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.toast(), Some("No media found on current slide"));

    state.clear_toast();
    assert_eq!(state.toast(), None);

    // Slide 1: Has media
    state.set_current_slide(1);
    assert!(state.current_slide_has_media());

    let action2 = handle_key_event(make_key(KeyCode::Char('m')), &mut state);
    assert_eq!(action2, AppAction::Redraw);
    assert!(state.toast().is_some());
    let toast = state.toast().unwrap();
    assert!(toast.contains("media") || toast.contains("player") || toast.contains("Playing"));
}
