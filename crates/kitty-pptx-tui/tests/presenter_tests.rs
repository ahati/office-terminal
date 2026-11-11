use kitty_pptx_display::ImageId;
use kitty_pptx_tui::app::state::PresentationState;
use kitty_pptx_tui::widget::layout::{
    compute_slide_rect, ASPECT_16_9, ASPECT_4_3, DEFAULT_CELL_ASPECT_RATIO,
};
use ratatui::layout::Rect;

#[test]
fn test_aspect_ratio_16_9_letterbox() {
    // 100 cols by 50 rows
    // cell_ratio = (16/9) / 0.5 = 3.5556
    // Fit by width: 100 / 3.5556 = 28.1 -> 28 rows <= 50 rows
    // Letterbox: height 28, width 100, centered at y = (50 - 28) / 2 = 11
    let area = Rect::new(0, 0, 100, 50);
    let rect = compute_slide_rect(area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
    assert_eq!(rect.width, 100);
    assert_eq!(rect.height, 28);
    assert_eq!(rect.x, 0);
    assert_eq!(rect.y, 11);
}

#[test]
fn test_aspect_ratio_4_3_letterbox() {
    // 100 cols by 50 rows
    // cell_ratio = (4/3) / 0.5 = 2.6667
    // Fit by width: 100 / 2.6667 = 37.5 -> 38 rows <= 50 rows
    // Letterbox: height 38, width 100, centered at y = (50 - 38) / 2 = 6
    let area = Rect::new(0, 0, 100, 50);
    let rect = compute_slide_rect(area, ASPECT_4_3, DEFAULT_CELL_ASPECT_RATIO);
    assert_eq!(rect.width, 100);
    assert_eq!(rect.height, 38);
    assert_eq!(rect.x, 0);
    assert_eq!(rect.y, 6);
}

#[test]
fn test_presenter_progress_ratio() {
    let mut state = PresentationState::new(4, vec![]);
    assert_eq!(state.progress_ratio(), 0.25); // slide 1 of 4

    state.next_slide();
    assert_eq!(state.progress_ratio(), 0.50); // slide 2 of 4

    state.next_slide();
    assert_eq!(state.progress_ratio(), 0.75); // slide 3 of 4

    state.next_slide();
    assert_eq!(state.progress_ratio(), 1.00); // slide 4 of 4
}

#[test]
fn test_speaker_notes_retrieval() {
    let notes = vec![
        Some("Welcome to the presentation".to_string()),
        None,
        Some("Final remarks & Q&A".to_string()),
    ];
    let mut state = PresentationState::new(3, notes);

    assert_eq!(state.current_notes(), Some("Welcome to the presentation"));

    state.next_slide();
    assert_eq!(state.current_notes(), None);

    state.next_slide();
    assert_eq!(state.current_notes(), Some("Final remarks & Q&A"));
}

#[test]
fn test_next_image_id_and_end_of_deck() {
    let mut state = PresentationState::new(3, vec![]);

    // On slide 0, next slide is slide 1
    assert_eq!(state.current_image_id(), ImageId(1));
    assert_eq!(state.next_slide_index(), Some(1));
    assert_eq!(state.next_image_id(), Some(ImageId(10)));

    state.next_slide();
    // On slide 1, next slide is slide 2
    assert_eq!(state.current_image_id(), ImageId(2));
    assert_eq!(state.next_slide_index(), Some(2));
    assert_eq!(state.next_image_id(), Some(ImageId(11)));

    state.next_slide();
    // On slide 2 (last slide), no next slide
    assert_eq!(state.current_image_id(), ImageId(3));
    assert_eq!(state.next_slide_index(), None);
    assert_eq!(state.next_image_id(), None);
}

#[test]
fn test_timer_formatting() {
    let state = PresentationState::new(5, vec![]);
    let elapsed = state.elapsed_formatted();
    assert_eq!(elapsed.len(), 8); // "00:00:00" format
    let parts: Vec<&str> = elapsed.split(':').collect();
    assert_eq!(parts.len(), 3);

    let wall_clock = state.wall_clock_formatted();
    assert!(wall_clock.ends_with("UTC"));
}
