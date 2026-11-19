use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use kitty_pptx_tui::app::events::{handle_key_event, AppAction};
use kitty_pptx_tui::app::mode::AppMode;
use kitty_pptx_tui::app::reader_events::handle_reader_mouse;
use kitty_pptx_tui::app::reader_state::{ReaderFitMode, ReaderLayout, ReaderState, Slot};
use kitty_pptx_tui::app::state::PresentationState;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

fn make_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Reader-mode state over `pages` letter-sized pages.
fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0); pages]);
    state
}

/// Draws one reader frame at `w`×`h`.
fn draw_reader(state: &PresentationState, terminal: &mut Terminal<TestBackend>, w: u16, h: u16) {
    terminal
        .draw(|f| kitty_pptx_tui::app::ui::render_reader(state, f, Rect::new(0, 0, w, h)))
        .unwrap();
}

#[test]
fn test_reader_state_initialization() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let reader = ReaderState::new(10, page_sizes);

    assert_eq!(reader.total_pages(), 10);
    assert_eq!(reader.layout, ReaderLayout::Spread);
    assert_eq!(reader.focus, Slot::Left);
    assert_eq!(reader.default_fit, ReaderFitMode::FitPage);
    assert_eq!(reader.default_zoom, 1.0);
    assert!(reader.sync);
    assert!(!reader.cover_alone);

    // Initial pages: side-by-side spread [0, 1]
    assert_eq!(reader.visible_pages(), vec![0, 1]);
    assert_eq!(reader.focused_page(), 0);
}

#[test]
fn test_reader_spread_navigation() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);

    // Anchor 0 -> spread [0, 1]
    assert_eq!(reader.visible_pages(), vec![0, 1]);

    // Next row -> [2, 3]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![2, 3]);

    // Next row -> [4, 5]
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![4, 5]);

    // Focus right -> focused page is 5
    reader.toggle_focus();
    assert_eq!(reader.focus, Slot::Right);
    assert_eq!(reader.focused_page(), 5);

    // Prev row -> [2, 3]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![2, 3]);

    // Prev row -> [0, 1]
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![0, 1]);
}

#[test]
fn test_reader_single_navigation() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    reader.toggle_layout();
    assert_eq!(reader.layout, ReaderLayout::Single);

    assert_eq!(reader.visible_pages(), vec![0]);
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![1]);
    reader.next_row();
    assert_eq!(reader.visible_pages(), vec![2]);
    reader.prev_row();
    assert_eq!(reader.visible_pages(), vec![1]);
}

#[test]
fn test_reader_jump_to_page() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);

    // Jump to page 4 -> spread [4, 5]
    reader.jump_to_page(4);
    assert_eq!(reader.visible_pages(), vec![4, 5]);
    assert_eq!(reader.focused_page(), 4);

    // Jump to page 5 -> spread [4, 5], focused on right (page 5)
    reader.jump_to_page(5);
    assert_eq!(reader.visible_pages(), vec![4, 5]);
    assert_eq!(reader.focused_page(), 5);

    // Jump to 0 -> spread [0, 1]
    reader.jump_to_page(0);
    assert_eq!(reader.visible_pages(), vec![0, 1]);

    // Jump beyond total
    reader.jump_to_page(99);
    assert_eq!(reader.anchor, 8);
}

#[test]
fn test_reader_scroll_and_zoom() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    reader.next_row(); // spread [2, 3]
    reader.cycle_fit(); // Switch visible pages [2, 3] to FitWidth to test vertical scrolling

    // Sync scroll: scrolls both visible pages
    reader.scroll_vertical(50);
    assert_eq!(reader.page_view(2).scroll_y, 50);
    assert_eq!(reader.page_view(3).scroll_y, 50);

    reader.scroll_horizontal(30);
    assert_eq!(reader.page_view(2).scroll_x, 30);
    assert_eq!(reader.page_view(3).scroll_x, 0);

    // Zoom
    reader.adjust_zoom(1.2);
    assert!((reader.page_view(2).zoom - 1.2).abs() < 0.01);

    // Reset view
    reader.reset_view();
    assert_eq!(reader.page_view(2).scroll_y, 0);
    assert!((reader.page_view(2).zoom - 1.0).abs() < 0.01);
}

#[test]
fn test_reader_fit_cycle() {
    let page_sizes = vec![(612.0, 792.0); 5];
    let mut reader = ReaderState::new(5, page_sizes);
    assert_eq!(reader.default_fit, ReaderFitMode::FitPage);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitWidth);

    reader.cycle_fit();
    assert_eq!(reader.page_view(0).fit, ReaderFitMode::FitPage);
}

#[test]
fn test_reader_status_line_format() {
    let page_sizes = vec![(612.0, 792.0); 10];
    let mut reader = ReaderState::new(10, page_sizes);
    reader.next_row(); // spread [2, 3] -> 1-based "spread 3-4"

    let status = reader.status_line("report.pdf");
    assert!(status.contains("report.pdf"));
    assert!(status.contains("spread 3-4/10"));
    assert!(status.contains("fit:page 100%"));
    assert!(status.contains("focus:L"));
    assert!(status.contains("sync:on"));
    assert!(status.contains("q:help"));
}

#[test]
fn test_reader_key_events() {
    let mut state = reader_state(10);

    assert_eq!(state.mode(), AppMode::Reader);
    assert_eq!(state.base_mode(), AppMode::Reader);

    // Next row
    let action = handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().visible_pages(), vec![2, 3]);

    // Scroll vertical down in FitWidth mode moves column offset
    state.reader_state_mut().unwrap().cycle_fit();
    let action = handle_key_event(make_key(KeyCode::Char('j')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(state.reader_state().unwrap().scroll_px > 0);

    // Focus toggle
    let action = handle_key_event(make_key(KeyCode::BackTab), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().focus, Slot::Right);

    // Fit cycle
    let action = handle_key_event(make_key(KeyCode::Char('f')), &mut state);
    assert_eq!(action, AppAction::Redraw);

    // Layout toggle
    let action = handle_key_event(make_key(KeyCode::Char('2')), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().layout, ReaderLayout::Single);

    // Quit
    let action = handle_key_event(make_key(KeyCode::Char('q')), &mut state);
    assert_eq!(action, AppAction::Quit);
}

#[test]
fn test_reader_arrow_keys_navigation() {
    let mut state = reader_state(10);

    // Initial: anchor 0 (spread [0, 1])
    assert_eq!(state.reader_state().unwrap().anchor, 0);

    // Down Arrow advances fractionally (>= 8 steps per row)
    for _ in 0..7 {
        let action = handle_key_event(make_key(KeyCode::Down), &mut state);
        assert_eq!(action, AppAction::Redraw);
        assert_eq!(state.reader_state().unwrap().anchor, 0);
        assert!(state.reader_state().unwrap().scroll_px > 0);
    }

    // 8th step completes the row transition to spread [2, 3]
    let action = handle_key_event(make_key(KeyCode::Down), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.current_slide(), 2);

    // PageDown immediately advances to next row [4, 5] (snaps to row top)
    let action = handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 4);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    // PageUp retreats to previous spread [2, 3] (snaps to row top)
    let action = handle_key_event(make_key(KeyCode::PageUp), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.reader_state().unwrap().scroll_px, 0);

    // Right Arrow does horizontal scroll when zoomed or row overflows
    state.reader_state_mut().unwrap().set_max_row_scroll_x(100);
    let action = handle_key_event(make_key(KeyCode::Right), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(state.reader_state().unwrap().row_scroll_x() > 0);

    // Left Arrow scrolls back
    let action = handle_key_event(make_key(KeyCode::Left), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert_eq!(state.reader_state().unwrap().row_scroll_x(), 0);

    // At fit (no overflow), Right advances row and Left retreats row
    state.reader_state_mut().unwrap().set_max_row_scroll_x(0);
    let action = handle_key_event(make_key(KeyCode::Right), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 4);

    let action = handle_key_event(make_key(KeyCode::Left), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
}

#[test]
fn test_reader_sorter_integration() {
    let mut state = reader_state(10);

    // Tab opens sorter
    handle_key_event(make_key(KeyCode::Tab), &mut state);
    assert_eq!(state.mode(), AppMode::Sorter);

    // In sorter, move right twice to slide index 2
    handle_key_event(make_key(KeyCode::Right), &mut state);
    handle_key_event(make_key(KeyCode::Right), &mut state);
    assert_eq!(state.selected_sorter_index(), 2);

    // Press Enter to select slide 2 and exit sorter
    let action = handle_key_event(make_key(KeyCode::Enter), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.mode(), AppMode::Reader);
    assert_eq!(state.current_slide(), 2);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(state.reader_state().unwrap().visible_pages(), vec![2, 3]);
}

#[test]
fn test_h_scroll_preserved_across_next_and_prev_row() {
    let mut state = reader_state(10);

    // Set max_row_scroll_x and scroll horizontally
    state.reader_state_mut().unwrap().set_max_row_scroll_x(200);
    state.reader_state_mut().unwrap().set_row_scroll_x(80);
    assert_eq!(state.reader_state().unwrap().row_scroll_x(), 80);

    // next_row() maintains horizontal scroll
    state.reader_state_mut().unwrap().next_row();
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(
        state.reader_state().unwrap().row_scroll_x(),
        80,
        "row_scroll_x must be preserved on next_row"
    );

    // prev_row() maintains horizontal scroll
    state.reader_state_mut().unwrap().prev_row();
    assert_eq!(state.reader_state().unwrap().anchor, 0);
    assert_eq!(
        state.reader_state().unwrap().row_scroll_x(),
        80,
        "row_scroll_x must be preserved on prev_row"
    );

    // KeyCode::PageDown maintains horizontal scroll
    let action = handle_key_event(make_key(KeyCode::PageDown), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 2);
    assert_eq!(
        state.reader_state().unwrap().row_scroll_x(),
        80,
        "row_scroll_x must be preserved on PageDown"
    );

    // KeyCode::PageUp maintains horizontal scroll
    let action = handle_key_event(make_key(KeyCode::PageUp), &mut state);
    assert_eq!(action, AppAction::SlideChanged);
    assert_eq!(state.reader_state().unwrap().anchor, 0);
    assert_eq!(
        state.reader_state().unwrap().row_scroll_x(),
        80,
        "row_scroll_x must be preserved on PageUp"
    );
}

#[test]
fn test_status_line_uses_primary_placements_at_zoom_0_5() {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut state = reader_state(10);

    // Zoom out to 0.5 so multiple rows are placed in the viewport
    state.reader_state_mut().unwrap().adjust_zoom(0.5);

    draw_reader(&state, &mut terminal, 160, 45);

    let placements = state.reader_placements();
    assert!(
        placements.len() > 2,
        "At zoom 0.5, multiple rows (primary + next) are placed in the viewport"
    );

    let status = state.reader_state().unwrap().status_line("test.pdf");
    assert!(
        status.contains("spread 1-2"),
        "Status line must report primary spread 1-2, got: {}",
        status
    );
    assert!(
        !status.contains("spread 1-4"),
        "Status line must NOT include preview rows in the current spread label"
    );
}

#[test]
fn test_horizontal_mode_h_and_l_slide_and_publish_columns() {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut state = reader_state(10);
    state.reader_state_mut().unwrap().toggle_horizontal();

    draw_reader(&state, &mut terminal, 160, 45);

    let placed_count = state.reader_placements().len();
    assert!(placed_count >= 2);
    assert_eq!(
        state.reader_state().unwrap().num_columns(),
        placed_count,
        "Horizontal mode must publish num_columns matching placed panes"
    );

    // 'l' slides horizontally right
    handle_key_event(make_key(KeyCode::Char('l')), &mut state);
    assert!(
        state.reader_state().unwrap().scroll_px > 0,
        "'l' slides horizontal view forward"
    );

    // 'h' slides horizontally back left
    handle_key_event(make_key(KeyCode::Char('h')), &mut state);
    assert_eq!(
        state.reader_state().unwrap().scroll_px,
        0,
        "'h' slides horizontal view back to origin"
    );
}

#[test]
fn test_continuous_v_scroll_row_1_matches_row_0_horizontal_alignment() {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut state = reader_state(10);

    // Zoom in 1.5x so rows overflow viewport width
    state.reader_state_mut().unwrap().adjust_zoom(1.5);

    // First draw to establish max_row_scroll_x
    draw_reader(&state, &mut terminal, 160, 45);

    // Slide row horizontally
    let max_x = state.reader_state().unwrap().max_row_scroll_x();
    assert!(max_x > 0);
    state.reader_state_mut().unwrap().set_row_scroll_x(max_x / 2);

    // Simulate vertical column scroll (scroll_px scrolled 3/4 through row 0 so row 1 is visible)
    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let viewport_w_px = 160.0 * cell_w_px;
    let viewport_h_px = 44.0 * cell_h_px;
    let row_stride = state
        .reader_state()
        .unwrap()
        .current_row_stride_px(viewport_w_px, viewport_h_px, cell_w_px, cell_h_px);
    state.reader_state_mut().unwrap().scroll_px = (row_stride * 0.75).round() as i64;
    // Intentional smoothing change: converge the displayed value toward the
    // target before asserting geometry at the requested offset.
    while state.reader_state().unwrap().advance_display() {}

    draw_reader(&state, &mut terminal, 160, 45);

    let placements = state.reader_placements();
    let row_0: Vec<_> = placements.iter().filter(|p| p.is_primary).collect();
    let row_1: Vec<_> = placements.iter().filter(|p| !p.is_primary).collect();

    assert_eq!(row_0.len(), 2, "Row 0 has 2 spread pages");
    assert_eq!(row_1.len(), 2, "Row 1 has 2 transition spread pages");

    // Left panes (page 0 and page 2) must match horizontally
    assert_eq!(
        row_0[0].cell_rect.col, row_1[0].cell_rect.col,
        "Left pane col must align exactly between row 0 and row 1"
    );
    assert_eq!(
        row_0[0].pixel_src.x, row_1[0].pixel_src.x,
        "Left pane pixel_src.x must match between row 0 and row 1"
    );

    // Right panes (page 1 and page 3) must match horizontally
    assert_eq!(
        row_0[1].cell_rect.col, row_1[1].cell_rect.col,
        "Right pane col must align exactly between row 0 and row 1"
    );
    assert_eq!(
        row_0[1].pixel_src.x, row_1[1].pixel_src.x,
        "Right pane pixel_src.x must match between row 0 and row 1"
    );
}

#[test]
fn test_reproduce_zoomed_v_scroll_blank_screen_in_gutter() {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut state = PresentationState::new(10, vec![]);
    // OpenCV-like aspect ratio (~0.7727)
    state.enable_reader_mode(10, vec![(460.0, 595.0); 10]);

    // Zoom in ~133%
    state.reader_state_mut().unwrap().adjust_zoom(1.33);

    let metrics = kitty_pptx_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let viewport_w_px = 160.0 * cell_w_px;
    let viewport_h_px = 44.0 * cell_h_px;

    let base_h = state
        .reader_state()
        .unwrap()
        .current_row_height_px(viewport_w_px, viewport_h_px, cell_w_px);
    let row_stride = state
        .reader_state()
        .unwrap()
        .current_row_stride_px(viewport_w_px, viewport_h_px, cell_w_px, cell_h_px);

    // Set scroll_px to just past Row 0 height, but still within row_stride (inside vertical gutter)
    let gutter_scroll = (base_h as i64 + 1).min(row_stride as i64 - 1);
    state.reader_state_mut().unwrap().scroll_px = gutter_scroll;
    // Intentional smoothing change: converge the displayed value so the
    // deep-gutter offset is actually exercised.
    while state.reader_state().unwrap().advance_display() {}

    draw_reader(&state, &mut terminal, 160, 45);

    let placements = state.reader_placements();
    // DEFECT REPRODUCTION:
    // Placements should NEVER be empty while navigating valid pages!
    // Row 0 would be 0 rows and Row 1 would be skipped because
    // row_occupied_cells_h == 0, resulting in a dark blank screen.
    assert!(
        !placements.is_empty(),
        "Placements must not be empty when scrolled into gutter! (DARK SCREEN DEFECT)"
    );
}

#[test]
fn test_reproduce_horizontal_arrow_right_page_shift() {
    // Test a common terminal size, e.g. 160x45 or 120x40
    for &(w, h) in &[(160, 45), (140, 40), (120, 35), (180, 50)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut state = PresentationState::new(10, vec![]);
        state.update_layout_dimensions(w, h);
        state.enable_reader_mode(10, vec![(612.0, 792.0); 10]);

        draw_reader(&state, &mut terminal, w, h);

        let p0 = state.reader_placements();
        let max_row_x = state.reader_state().unwrap().max_row_scroll_x();

        // Verify unzoomed pages at fit have zero horizontal scroll noise
        assert_eq!(max_row_x, 0, "max_row_scroll_x must be 0 at fit");
        for p in &p0 {
            assert_eq!(p.max_scroll_x, 0, "max_scroll_x must be 0 at fit");
            assert_eq!(p.pixel_src.x, 0, "pixel_src.x must be 0 at fit");
        }

        // Press Right arrow: must advance to next spread cleanly without twitching
        let action = handle_key_event(make_key(KeyCode::Right), &mut state);
        assert_eq!(action, AppAction::SlideChanged);
        assert_eq!(state.reader_state().unwrap().anchor, 2);

        draw_reader(&state, &mut terminal, w, h);
        let p1 = state.reader_placements();
        assert_eq!(p1[0].page, 2);
        assert_eq!(p1[0].pixel_src.x, 0);

        // Press Left arrow: must retreat to previous spread cleanly without twitching
        let action_l = handle_key_event(make_key(KeyCode::Left), &mut state);
        assert_eq!(action_l, AppAction::SlideChanged);
        assert_eq!(state.reader_state().unwrap().anchor, 0);

        draw_reader(&state, &mut terminal, w, h);
        let p2 = state.reader_placements();
        assert_eq!(p2[0].page, 0);
        assert_eq!(p2[0].pixel_src.x, 0);
    }
}

#[test]
fn test_reader_mouse_wheel_scroll_direction() {
    let wheel = |kind| MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE };
    let mut state = reader_state(10);

    // Wheel-down scrolls forward through the same path as j.
    let action = handle_reader_mouse(wheel(MouseEventKind::ScrollDown), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(state.reader_state().unwrap().scroll_px > 0, "wheel-down grows the scroll target");

    // Wheel-up scrolls back to the origin.
    let action = handle_reader_mouse(wheel(MouseEventKind::ScrollUp), &mut state);
    assert_eq!(action, AppAction::Redraw);
    assert!(state.reader_state().unwrap().scroll_px == 0, "wheel-up returns the target to the origin");

    // Non-wheel kinds are ignored.
    assert_eq!(
        handle_reader_mouse(wheel(MouseEventKind::Down(MouseButton::Left)), &mut state),
        AppAction::None
    );
}

#[test]
fn test_advance_display_converges_to_target() {
    let mut reader = ReaderState::new(4, vec![(612.0, 792.0); 4]);

    // Exponential approach snaps within a bounded number of frames; the
    // loop needs no sleeping because advance_display is deterministic.
    reader.scroll_px = 900;
    assert!(reader.is_animating(), "target set, displayed value lags");
    let mut frames = 0;
    while reader.advance_display() && frames < 1_000 {
        frames += 1;
    }
    assert!(frames < 100, "converges in a small number of frames, got {frames}");
    assert!(!reader.is_animating());
    assert_eq!(reader.displayed_scroll_px(), 900.0);

    // Reverse direction (back to the origin) converges as well.
    reader.scroll_px = 0;
    let mut back_frames = 0;
    while reader.advance_display() && back_frames < 1_000 {
        back_frames += 1;
    }
    assert_eq!(reader.displayed_scroll_px(), 0.0);
    assert!(!reader.is_animating());
}
