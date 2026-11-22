//! The reader's zoom-out floor hands the screen to the page sorter
//! (PRD 14a §6.3): once a further zoom step would leave the reader's
//! two-row window unable to fill the viewport — or shrink panes to
//! sorter-card size — `-` opens the sorter instead of zooming, so the
//! reader never renders thumbnail-sized pages or the voids its two-row
//! window would leave behind.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use office_terminal_tui::app::events::handle_key_event;
use office_terminal_tui::app::mode::AppMode;
use office_terminal_tui::app::sorter_state::reader_zoom_hits_sorter_floor;
use office_terminal_tui::app::state::PresentationState;

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
fn the_floor_fires_on_overview_scale() {
    // The overview condition: a pane within twice sorter-card size is
    // heading for thumbnail territory (12-row cards -> 480 px at cell 20).
    let dom = (612.0_f32, 792.0_f32);
    // Two rows still fill (2*440+20 >= 880) yet the 440 px pane is within
    // twice the 240 px card: overview scale decides.
    assert!(reader_zoom_hits_sorter_floor(0.5, dom, 880.0, 20.0, 12));
    // Still readable size: 792 px panes, no void, above twice card scale.
    assert!(!reader_zoom_hits_sorter_floor(0.9, dom, 880.0, 20.0, 12));
}

#[test]
fn zoom_out_hands_off_to_the_sorter_at_the_floor() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    // 0.9^5 ~ 0.59: panes ~26 rows, still reading size — five presses zoom.
    for _ in 0..5 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Reader, "five presses stay in the reader");
    let zoom = state.reader_state().unwrap().default_zoom;
    assert!((zoom - 0.590).abs() < 0.01, "zoom reached {zoom}");

    // The sixth press would take the pane within twice sorter-card scale
    // (0.9^6 ~ 0.53): the sorter opens instead, selected on the page being
    // read, zoom untouched.
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
    for _ in 0..6 {
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
    assert_eq!(
        reader.default_zoom, 1.0,
        "the applied selection resets the zoomed-out view to fit"
    );
}

#[test]
fn sorter_esc_returns_without_moving_the_reader() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    for _ in 0..6 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Sorter);
    handle_key_event(key(KeyCode::Char('l')), &mut state);
    assert_eq!(state.selected_sorter_index(), 9);

    handle_key_event(key(KeyCode::Esc), &mut state);
    assert_eq!(state.mode(), AppMode::Reader, "Esc returns to the reader");
    assert_eq!(state.reader_state().unwrap().anchor, 8, "the reader is unmoved");
    assert_eq!(state.current_slide(), 8, "the unapplied selection is dropped");
    assert!(
        (state.reader_state().unwrap().default_zoom - 0.590).abs() < 0.01,
        "Esc preserves the zoomed-out view"
    );
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

#[test]
fn sorter_enter_on_the_already_selected_page_still_returns() {
    let mut state = reader_state(30);
    state.set_current_slide(8);

    // Walk down to the floor and into the sorter.
    for _ in 0..6 {
        handle_key_event(key(KeyCode::Char('-')), &mut state);
    }
    assert_eq!(state.mode(), AppMode::Sorter);
    assert_eq!(state.selected_sorter_index(), 8, "the floor selects the read page");

    // Enter without moving the selection: the reader must come back on the
    // same page, and the applied selection resets the zoom to fit.
    handle_key_event(key(KeyCode::Enter), &mut state);
    assert_eq!(
        state.mode(),
        AppMode::Reader,
        "Enter on the already-selected page returns to the reader"
    );
    assert_eq!(state.reader_state().unwrap().anchor, 8);
    assert_eq!(
        state.reader_state().unwrap().default_zoom, 1.0,
        "the same-page selection also lands at fit"
    );
}
