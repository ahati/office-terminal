use super::*;

#[test]
fn test_row_transition_zero_restage_stable_ids() {
    let (mut terminal, mut state) = reader_fixture(160, 10, vec![(612.0, 792.0); 10]);

    let (mut driver, log) = MockRecordingDriver::new();
    let pipeline = MockPdfPipeline;
    let mut syncer = GraphicsSyncer::new();
    let canvas_target = RenderTargetSpec {
        width_px: 2560,
        height_px: 3312,
    };

    // Initial draw and sync: spread [0, 1] stages under stable per-page ids.
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings)
        .unwrap();

    let metrics = office_terminal_display::probe_cell_pixel_size();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let (row_stride, base_h) = {
        let reader = state.reader_state().unwrap();
        (
            reader.current_row_stride_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px, cell_h_px),
            reader.current_row_height_px(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px),
        )
    };

    // Scroll near the row boundary so pages 2,3 peek from below and stage.
    state.reader_state_mut().unwrap().scroll_px = (row_stride - 8.0).round() as i64;
    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings)
        .unwrap();

    let peek_placements: Vec<_> = bindings.reader.iter().filter(|p| p.page >= 2).collect();
    assert!(peek_placements.len() >= 2, "Next-row pages peek from below");
    for p in &peek_placements {
        assert_eq!(p.id, page_image_id(p.page), "pane ids are page-stable, not row-parity");
    }
    let staged_before = log.lock().unwrap().staged.len();
    let presented_before = log.lock().unwrap().presented.len();
    assert_eq!(staged_before, 4, "All four visible panes staged exactly once");

    // Cross the row boundary exactly like the j handler does: pages 2,3
    // become the primary row with the SAME stable ids.
    state.reader_state_mut().unwrap().scroll_px = (row_stride + 4.0).round() as i64;
    let geo = {
        let metrics = office_terminal_display::probe_cell_pixel_size();
        let cell_w_px = metrics.width_px.max(4.0) as f32;
        let cell_h_px = metrics.height_px.max(8.0) as f32;
        let reader = state.reader_state().unwrap();
        crate::app::reader_scroll::RowGeometry {
            viewport_h_px: 44.0 * cell_h_px,
            slot_w_px: reader.slot_width(160.0 * cell_w_px, 44.0 * cell_h_px, cell_w_px),
            cell_w_px,
            cell_h_px,
        }
    };
    let _ = state
        .reader_state_mut()
        .unwrap()
        .scroll_column_detailed(0, row_stride, base_h, geo);
    assert_eq!(
        state.reader_state().unwrap().anchor,
        2,
        "Anchor advanced to the next row"
    );

    let bindings = draw_reader(&mut state, &mut terminal, 160);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas_target, &bindings)
        .unwrap();

    assert_eq!(
        log.lock().unwrap().staged.len(),
        staged_before,
        "row transition re-places panes without re-transmitting them"
    );
    assert!(
        log.lock().unwrap().presented.len() > presented_before,
        "the moved panes were re-placed at their new cells"
    );
}
