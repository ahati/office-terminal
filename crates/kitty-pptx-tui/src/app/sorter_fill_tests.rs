//! Sorter card fill-in across the per-frame staging throttle.
//!
//! Re-entering the grid with every raster already cached (pipeline cache or
//! downscale cache) resolves every card immediately: nothing schedules
//! workers, so no completion event can wake the deadline-driven loop — and
//! the sorter has no clock, toast or animation either. The sync must
//! therefore report how many ready cards the throttle deferred, and the
//! caller must keep drawing frames until the count reaches zero. Without
//! that report the grid stayed four-cards-full until the next key press
//! (sorter → Enter → sorter).

use std::sync::Arc;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::bindings::FrameBindings;
use crate::app::ui::render_app;

use super::sorter_graphics_tests::{
    draw_sorter, sorter_state, GridPipeline, RecordingDriver, CANVAS, WIDE,
};
use crate::app::mode::AppMode;
use crate::app::{GraphicsSyncOutcome, PresentationApp};

#[test]
fn re_entry_with_cached_rasters_drains_the_throttle_over_frames() {
    // GridPipeline answers Ready for every card without any worker, exactly
    // like a warm cache on sorter re-entry.
    let pipeline = GridPipeline::new(9);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = super::graphics::GraphicsSyncer::new();
    let mut state = sorter_state(9);

    let bindings = draw_sorter(&state, WIDE);
    let total = bindings.thumbnails.len();
    assert_eq!(total, 9, "the whole deck fits one grid page");

    let staged_so_far = |log: &std::sync::Mutex<super::sorter_graphics_tests::DriverLog>| {
        log.lock().unwrap().staged.len()
    };

    // Frame 1: the throttle stages four cards and defers the rest.
    let deferred = syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    assert_eq!(staged_so_far(&log), 4, "the throttle caps new stages per frame");
    assert_eq!(deferred, total - 4, "the remainder is reported to the caller");

    // Frames 2..n: no pipeline activity happens in between — the report
    // alone must drive them.
    let deferred = syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    assert_eq!(staged_so_far(&log), 8);
    assert_eq!(deferred, total - 8);

    let deferred = syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    assert_eq!(staged_so_far(&log), total, "every card is resident");
    assert_eq!(deferred, 0, "the drain terminates");

    // A settled grid reports nothing further.
    let deferred = syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    assert_eq!(deferred, 0);
    assert_eq!(staged_so_far(&log), total);
}

#[test]
fn sync_graphics_surfaces_the_deferred_cards_for_the_run_loop() {
    // The PresentationApp plumbing: `sync_graphics` must hand the run loop
    // the deferred count so its dirty check schedules the follow-up frames.
    let pipeline = Arc::new(GridPipeline::new(9));
    let (driver, _log) = RecordingDriver::new();
    let mut app = PresentationApp::new(pipeline, Box::new(driver), 9, vec![None; 9]);
    app.state_mut().set_mode(AppMode::Sorter);

    let mut terminal = Terminal::new(TestBackend::new(WIDE.0, WIDE.1)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_app(app.state(), f, &mut bindings))
        .unwrap();

    let outcome = app.sync_graphics(&bindings).unwrap();
    assert_eq!(
        outcome,
        GraphicsSyncOutcome {
            fresh_warnings: 0,
            deferred_card_stages: bindings.thumbnails.len() - 4,
        },
        "the first sorter frame defers everything past the throttle"
    );

    // The slide views report no deferred cards.
    app.state_mut().set_mode(AppMode::Fullscreen);
    let mut terminal = Terminal::new(TestBackend::new(WIDE.0, WIDE.1)).unwrap();
    let mut bindings = FrameBindings::default();
    terminal
        .draw(|f| render_app(app.state(), f, &mut bindings))
        .unwrap();
    assert_eq!(app.sync_graphics(&bindings).unwrap().deferred_card_stages, 0);
}
