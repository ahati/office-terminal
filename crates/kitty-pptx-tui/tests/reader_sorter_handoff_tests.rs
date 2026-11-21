//! The reader's zoom-out floor hands the screen to the page sorter
//! (PRD 14a §6.3): once a further zoom step would leave the reader's
//! two-row window unable to fill the viewport — or shrink panes to
//! sorter-card size — `-` opens the sorter instead of zooming, so the
//! reader never renders thumbnail-sized pages or the voids its two-row
//! window would leave behind.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use kitty_pptx_tui::app::events::handle_key_event;
use kitty_pptx_tui::app::mode::AppMode;
use kitty_pptx_tui::app::sorter_state::reader_zoom_hits_sorter_floor;
use kitty_pptx_tui::app::state::PresentationState;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![None; pages]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0); pages]);
    state.set_layout_dimensions(160, 45);
    state
}

#[test]
fn the_floor_fires_when_two_rows_stop_filling_the_viewport() {
    // 160x45 terminal, letter pages, cell 10x20: viewport height 880 px,
    // gutter 20 px, sorter card 12 rows (240 px).
    let dom = (612.0_f32, 792.0_f32);

    // Two full-height panes plus the gutter still cover the viewport.
    assert!(!reader_zoom_hits_sorter_floor(0.55, dom, 880.0, 20.0, 12));
    // Below ~0.49 the two-row window cannot fill: 2*880*0.47 + 20 < 880.
    assert!(reader_zoom_hits_sorter_floor(0.47, dom, 880.0, 20.0, 12));
}

#[test]
fn the_floor_also_fires_on_sorter_card_scale() {
    // Short terminals where the two-row window still fits but the pane is
    // already card-sized: 7 viewport rows (140 px), 5-row cards.
    let dom = (612.0_f32, 792.0_f32);
    // Two rows still fill (2*70+20 >= 140) yet the 70 px pane fits a 100 px
    // card: card scale decides.
    assert!(reader_zoom_hits_sorter_floor(0.5, dom, 140.0, 20.0, 5));
    // A zoom where neither condition holds.
    assert!(!reader_zoom_hits_sorter_floor(0.9, dom, 140.0, 20.0, 5));
}

#[test]
fn zoom_out_hands_off_to_the_sorter_at_the_floor() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    // 0.9^6 ~ 0.53: still comfortably above the floor — six presses zoom.
    for _ in 0..6 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Reader, "six presses stay in the reader");
    let zoom = state.reader_state().unwrap().default_zoom;
    assert!((zoom - 0.531).abs() < 0.01, "zoom reached {zoom}");

    // The seventh press would cross the floor (0.9^7 ~ 0.48): the sorter
    // opens instead, selected on the page being read, zoom untouched.
    handle_key_event(key(KeyCode::Char('-')), &mut state);
    assert_eq!(state.mode(), AppMode::Sorter, "the floor hands off to the sorter");
    assert_eq!(state.selected_sorter_index(), 8, "the sorter selects the read page");
    let zoom_after = state.reader_state().unwrap().default_zoom;
    assert_eq!(
        zoom, zoom_after,
        "the handoff does not zoom past the floor"
    );
}

#[test]
fn sorter_enter_returns_to_the_reader_on_the_selected_page() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    // Walk down to the floor and into the sorter.
    for _ in 0..7 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Sorter);

    // Move the selection one card right and apply it.
    handle_key_event(key(KeyCode::Char('l')), &mut state);
    assert_eq!(state.selected_sorter_index(), 9);
    handle_key_event(key(KeyCode::Enter), &mut state);

    assert_eq!(state.mode(), AppMode::Reader, "Enter returns to the reader");
    let reader = state.reader_state().unwrap();
    assert_eq!(
        reader.anchor, 8,
        "the reader re-anchors onto the selected row (page 9 pairs with 8)"
    );
    assert_eq!(
        state.current_slide(), 9,
        "the applied selection is the page the sorter chose"
    );
}

#[test]
fn sorter_esc_returns_without_moving_the_reader() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    for _ in 0..7 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Sorter);
    handle_key_event(key(KeyCode::Char('l')), &mut state);
    assert_eq!(state.selected_sorter_index(), 9);

    handle_key_event(key(KeyCode::Esc), &mut state);
    assert_eq!(state.mode(), AppMode::Reader, "Esc returns to the reader");
    assert_eq!(state.reader_state().unwrap().anchor, 8, "the reader is unmoved");
    assert_eq!(state.current_slide(), 8, "the unapplied selection is dropped");
}

#[test]
fn tab_remains_a_manual_override_from_any_zoom() {
    let mut state = reader_state(30);
    state.set_current_slide(4);

    // At fit zoom, far above the floor, Tab still opens the sorter and
    // Esc hands the screen straight back.
    handle_key_event(key(KeyCode::Tab), &mut state);
    assert_eq!(state.mode(), AppMode::Sorter);
    assert_eq!(state.selected_sorter_index(), 4);
    handle_key_event(key(KeyCode::Esc), &mut state);
    assert_eq!(state.mode(), AppMode::Reader);
    assert_eq!(state.reader_state().unwrap().anchor, 4);
}
