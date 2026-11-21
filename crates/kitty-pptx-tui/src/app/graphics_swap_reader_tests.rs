//! Reader-pane side of the terminal-graphics swap discipline.
//!
//! The reader pool is small (12 ids) and stable ids repeat every 12 pages
//! (`page % READER_IMAGE_ID_SLOTS`), so a pane can resolve a page whose id is
//! already displayed. Re-transmitting that id would delete the image and its
//! placements (kitty's re-transmit rule) and blank the pane for the whole
//! transmit + decode window — the same defect class the slide/sorter views
//! fixed in [`super::graphics_swap_tests`]. These tests pin the reader
//! syncer's half: swap into a spare pool id, present, then release; reuse a
//! swapped bitmap without re-sending it; and keep the displayed image while
//! the next one renders (the keep-until-Ready rule the slide views follow),
//! swapping atomically once the raster lands.

use kitty_pptx_core::RenderTargetSpec;
use kitty_pptx_display::{CellRect, PixelRect, READER_IMAGE_ID_BASE};

use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::reader_state::ReaderSlotPlacement;
use crate::app::reader_types::page_image_id;
use crate::app::state::PresentationState;

use super::graphics_swap_tests::{LoggingDriver, ScriptedPipeline, CANVAS, CARD_RASTER};

/// Canonical reader page target (2560 px wide, letter aspect).
const PAGE_TARGET: RenderTargetSpec = RenderTargetSpec {
    width_px: 2560,
    height_px: 3312,
};

fn pane(page: usize, row: u16) -> ReaderSlotPlacement {
    ReaderSlotPlacement {
        page,
        id: page_image_id(page),
        cell_rect: CellRect { row, col: 0, rows: 40, cols: 60 },
        pixel_src: PixelRect { x: 0, y: 0, width: 2560, height: 800 },
        page_target: PAGE_TARGET,
        max_scroll_x: 0,
        is_primary: true,
    }
}

/// Bindings with one pane on `page` at the same rect (a page turn).
fn single_pane(page: usize) -> FrameBindings {
    let mut bindings = FrameBindings::default();
    bindings.reader.push(pane(page, 0));
    bindings
}

fn reader_state(pages: usize) -> PresentationState {
    let mut state = PresentationState::new(pages, vec![None; pages]);
    state.enable_reader_mode(pages, vec![(612.0, 792.0)]);
    state
}

#[test]
fn a_page_id_collision_swaps_instead_of_retransmitting_the_displayed_pane() {
    // Pages 5 and 17 share the stable id (5 % 12 == 17 % 12): showing 17
    // after 5 must not re-transmit the id the pane is displaying.
    let pipeline = ScriptedPipeline::ready(&[(5, CARD_RASTER), (17, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = reader_state(30);

    let first = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &first)
        .unwrap();
    let displayed = page_image_id(5);
    {
        let log = log.lock().unwrap();
        assert_eq!(
            log.calls[0],
            format!("stage id={} 64x36", displayed.0),
            "page 5 stages under its stable id: {:?}",
            log.calls
        );
        assert!(log.position(&format!("present id={}", displayed.0)).is_some());
    }

    // Jump 12 pages down: same pane rect, same stable id, different page.
    let second = single_pane(17);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &second)
        .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(
        log.count_prefix(&format!("stage id={} ", displayed.0)),
        1,
        "the displayed pane id is never re-transmitted: {:?}",
        log.calls
    );
    let stage = log.position("stage id=").expect("the new page is staged");
    let present = log
        .position("present id=")
        .filter(|&pos| pos > stage)
        .expect("the spare id is presented");
    let release = log
        .position(&format!("release id={}", displayed.0))
        .expect("the outgoing page is released");
    assert!(
        stage < present && present < release,
        "stage → present → release keeps the pane filled: {:?}",
        log.calls
    );
    // The spare id comes from the reader pool, not from another view's range.
    let staged_line = &log.calls[stage];
    let spare_id: u32 = staged_line
        .split("id=")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|id| id.parse().ok())
        .expect("the staged line names an id");
    assert!(
        (READER_IMAGE_ID_BASE..READER_IMAGE_ID_BASE + 12).contains(&spare_id),
        "the swap id stays inside the reader pool: {staged_line}"
    );
}

#[test]
fn a_swapped_page_is_reused_without_retransmission() {
    // After the swap the pane's pixels live under the spare id: a following
    // frame with the same placement must not talk to the driver at all.
    let pipeline = ScriptedPipeline::ready(&[(5, CARD_RASTER), (17, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = reader_state(30);

    let first = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &first)
        .unwrap();
    let second = single_pane(17);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &second)
        .unwrap();

    let before = log.lock().unwrap().calls.len();
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &second)
        .unwrap();
    assert_eq!(
        log.lock().unwrap().calls.len(),
        before,
        "an unchanged swapped pane re-places nothing"
    );
}

#[test]
fn a_rendering_page_keeps_the_displayed_image_until_the_raster_lands() {
    // Page turns keep the previous image on screen while the next raster
    // renders — including when that image lives under a swapped id — and
    // swap atomically once it lands: present first, release after. The
    // release-into-a-blank discipline flickered at every transition when a
    // held key outran the rasterizer.
    let pipeline = ScriptedPipeline::ready(&[
        (5, CARD_RASTER),
        (17, CARD_RASTER),
        (29, CARD_RASTER),
    ]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = reader_state(30);

    let first = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &first)
        .unwrap();
    let second = single_pane(17);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &second)
        .unwrap();

    // Page 29 (third collision on the same stable id) is still rendering:
    // the pane keeps showing page 17's bitmap — nothing is released.
    pipeline.set_rendering(29);
    let third = single_pane(29);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &third)
        .unwrap();
    {
        let log = log.lock().unwrap();
        let first_swap_release = log
            .position(&format!("release id={}", page_image_id(5).0))
            .expect("the first pane swap released the stable id");
        let releases_after_swap = log
            .calls
            .iter()
            .enumerate()
            .filter(|(pos, call)| *pos > first_swap_release && call.starts_with("release id="))
            .count();
        assert_eq!(
            releases_after_swap, 0,
            "the rendering pane keeps the displayed image: {:?}",
            log.calls
        );
    }

    // The raster lands: the new bitmap is presented BEFORE the displayed
    // one is released — an atomic swap, never a blank frame.
    pipeline.set_size(29, CARD_RASTER);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &third)
        .unwrap();
    let log = log.lock().unwrap();
    let present = log
        .calls
        .iter()
        .rposition(|call| call.starts_with("present id="))
        .expect("the new bitmap is presented");
    let releases_after_present = log
        .calls
        .iter()
        .enumerate()
        .filter(|(pos, call)| *pos > present && call.starts_with("release id="))
        .count();
    assert!(
        releases_after_present >= 1,
        "the displayed image is released after the swap: {:?}",
        log.calls
    );
}

#[test]
fn a_page_kept_while_rendering_returns_instantly() {
    // With the pane keeping its displayed image through the raster window,
    // scrolling back to that page must re-place the resident bitmap without
    // re-transmitting anything — the image never left the terminal.
    let pipeline = ScriptedPipeline::ready(&[(5, CARD_RASTER), (17, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = reader_state(30);

    // The pane shows page 5 (staged + placed under its stable id).
    let first = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &first)
        .unwrap();
    let id = page_image_id(5).0;
    assert_eq!(
        log.lock().unwrap().count_prefix(&format!("stage id={id} ")),
        1,
        "page 5 transmits once"
    );

    // The pane moves to a colliding page that is still rendering: the
    // displayed bitmap is kept, not released.
    pipeline.set_rendering(17);
    let second = single_pane(17);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &second)
        .unwrap();
    assert!(
        log.lock().unwrap().position(&format!("release id={id}")).is_none(),
        "the displayed bitmap is kept while page 17 renders"
    );

    // Scrolling back to page 5 re-places the resident bitmap — no second
    // transmission, no blank window.
    let third = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &third)
        .unwrap();
    let log = log.lock().unwrap();
    assert_eq!(
        log.count_prefix(&format!("stage id={id} ")),
        1,
        "page 5 never re-transmits: {:?}",
        log.calls
    );
    assert!(
        log.calls.iter().any(|call| call.starts_with(&format!("present id={id}"))),
        "the pane is placed from the resident bitmap"
    );
}

#[test]
fn the_sorter_teardown_waits_for_the_reader_to_present() {
    // Leaving the sorter into the reader releases the grid's cards only
    // after the reader's panes present (present-then-release, the same
    // discipline as the slide path). When the returning page's raster was
    // evicted during the sorter visit, the cards stay on screen instead of
    // blanking it — the reader never "fails to come back".
    let pipeline = ScriptedPipeline::ready(&[(5, CARD_RASTER), (6, CARD_RASTER)]);
    let (mut driver, log) = LoggingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = reader_state(30);

    // The reader owns the screen with page 5 staged and placed.
    let reader_bindings = single_pane(5);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &reader_bindings)
        .unwrap();
    assert!(
        log.lock().unwrap().position(&format!("present id={}", page_image_id(5).0)).is_some(),
        "the reader pane is on screen"
    );

    // The sorter takes over: two cards staged; the reader's pane releases.
    state.open_sorter();
    let sorter_bindings = FrameBindings {
        thumbnails: vec![
            (5, CellRect { row: 0, col: 0, rows: 10, cols: 40 }),
            (6, CellRect { row: 0, col: 40, rows: 10, cols: 40 }),
        ],
        ..FrameBindings::default()
    };
    for _ in 0..8 {
        syncer
            .sync(&mut state, &pipeline, &mut driver, &CANVAS, &sorter_bindings)
            .unwrap();
    }
    assert_eq!(
        log.lock().unwrap().count_prefix("present id="),
        3,
        "two cards presented beside the reader pane"
    );

    // Enter jumps back to page 5 — but its raster was evicted while the
    // sorter had the screen. The cards must stay: releasing them now would
    // leave nothing on screen for the whole raster window.
    pipeline.set_rendering(5);
    state.set_selected_sorter_index(5);
    state.exit_sorter(true);
    let releases_before = log.lock().unwrap().count_prefix("release id=");
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &reader_bindings)
        .unwrap();
    {
        let log = log.lock().unwrap();
        assert_eq!(
            log.count_prefix("release id="),
            releases_before,
            "the grid cards stay while the reader cannot present"
        );
    }

    // The raster lands: the reader presents first, then the cards release.
    pipeline.set_size(5, CARD_RASTER);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &reader_bindings)
        .unwrap();
    let log = log.lock().unwrap();
    let present_pos = log
        .calls
        .iter()
        .rposition(|call| call.starts_with(&format!("present id={}", page_image_id(5).0)))
        .expect("the reader pane is presented again");
    let releases_after = log
        .calls
        .iter()
        .enumerate()
        .filter(|(pos, call)| *pos > present_pos && call.starts_with("release id="))
        .count();
    assert!(
        releases_after >= 2,
        "both grid cards release after the reader presents: {:?}",
        log.calls
    );
}
