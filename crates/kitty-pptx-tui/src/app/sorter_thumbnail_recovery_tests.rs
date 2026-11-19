//! Sorter raster-target resolution and recovery behavior.
//!
//! Cards render at a dedicated aspect-derived sorter target (1280 px wide)
//! instead of the canvas raster, and the syncer recovers from pipeline
//! misses: restaging from the downscaled-card cache when a raster is evicted,
//! and re-issuing the idempotent window prefetch while a card stays
//! unresolvable so no card is left permanently blank.

use kitty_pptx_core::RenderTargetSpec;

use crate::app::graphics::GraphicsSyncer;
use crate::app::graphics_thumbnails::thumbnail_target_for_aspect;

use super::sorter_graphics_tests::{
    draw_sorter, sorter_state, sync_until_settled, GridPipeline, RecordingDriver, CANVAS, SHORT,
    WIDE,
};

/// Sorter target for the default 16:9 test state.
const THUMB: RenderTargetSpec = RenderTargetSpec {
    width_px: 1280,
    height_px: 720,
};

#[test]
fn sorter_resolves_the_dedicated_thumbnail_target() {
    let pipeline = GridPipeline::new(6);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(6);

    let bindings = draw_sorter(&state, WIDE);
    let cards = &bindings.thumbnails;
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();

    // Cards are looked up at the dedicated aspect-derived sorter target, not
    // the canvas target: 1280 px keeps text runs above the rasterizer's 6 px
    // font floor while costing ~5x less than a canvas-resolution raster.
    let expected = thumbnail_target_for_aspect(state.aspect_ratio());
    assert_eq!(
        expected, THUMB,
        "16:9 deck derives a 1280x720 sorter target"
    );
    let targets = pipeline.requested_targets();
    assert!(!targets.is_empty(), "cards resolve an explicit target");
    assert!(
        targets.iter().all(|target| *target == expected),
        "every card resolves the sorter target: {targets:?}"
    );
    assert!(
        targets.iter().all(|target| *target != CANVAS),
        "no card reuses the canvas raster"
    );

    // The visible page is prefetched at that same target, so the grid warms
    // exactly the rasters the cards need.
    let prefetched = pipeline.prefetch_log();
    for (slide, _) in cards {
        assert!(
            prefetched.contains(&(*slide, expected)),
            "visible slide {slide} is prefetched at the sorter target: {prefetched:?}"
        );
    }
    assert!(
        log.lock().unwrap().staged.len() <= 4,
        "staging is throttled"
    );
}

#[test]
fn sorter_target_is_resize_and_aspect_derived() {
    // Landscape fits 1280 px wide; portrait fits 1280 px tall; degenerate
    // aspects fall back to 16:9 instead of dividing by zero.
    assert_eq!(
        thumbnail_target_for_aspect(16.0 / 9.0),
        RenderTargetSpec {
            width_px: 1280,
            height_px: 720
        }
    );
    assert_eq!(
        thumbnail_target_for_aspect(9.0 / 16.0),
        RenderTargetSpec {
            width_px: 720,
            height_px: 1280
        }
    );
    assert_eq!(
        thumbnail_target_for_aspect(0.0),
        thumbnail_target_for_aspect(16.0 / 9.0)
    );
    assert_eq!(
        thumbnail_target_for_aspect(f32::NAN),
        thumbnail_target_for_aspect(16.0 / 9.0)
    );
}

#[test]
fn sorter_restages_a_card_from_the_downscale_cache_when_the_raster_is_gone() {
    let pipeline = GridPipeline::new(30);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(30);

    let first_bindings = draw_sorter(&state, SHORT);
    assert_eq!(first_bindings.thumbnails.len(), 12);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        &first_bindings,
        12,
    );

    // Scroll far away, staging a second page of cards.
    state.set_selected_sorter_index(24);
    let second_bindings = draw_sorter(&state, SHORT);
    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        &second_bindings,
        24,
    );
    let staged_before = log.lock().unwrap().staged.len();

    // The first page's rasters fall out of the pipeline budget on the way
    // back: only the syncer's downscaled-card cache still holds them.
    pipeline.set_ready(false);
    state.set_selected_sorter_index(0);
    let back_bindings = draw_sorter(&state, SHORT);
    assert_eq!(back_bindings.thumbnails.len(), 12);

    sync_until_settled(
        &mut syncer,
        &mut state,
        &pipeline,
        &mut driver,
        &log,
        &back_bindings,
        staged_before + 12,
    );

    let log = log.lock().unwrap();
    assert_eq!(
        log.staged.len(),
        staged_before + 12,
        "the whole first page restages from the card cache without a raster"
    );
    assert!(
        log.presented.len() >= staged_before + 12,
        "every restaged card is placed"
    );
}

#[test]
fn sorter_reissues_prefetch_while_a_card_is_unresolvable() {
    let pipeline = GridPipeline::new(6);
    pipeline.set_ready(false);
    let (mut driver, log) = RecordingDriver::new();
    let mut syncer = GraphicsSyncer::new();
    let mut state = sorter_state(6);

    let bindings = draw_sorter(&state, WIDE);
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    let after_first = pipeline.prefetch_count();
    assert!(after_first > 0, "the window prefetch fires");
    assert!(
        log.lock().unwrap().staged.is_empty(),
        "nothing can be staged while no raster or card exists"
    );

    // Every further frame re-issues the idempotent window prefetch while a
    // visible card stays unresolvable, so an evicted raster can never leave
    // a permanently blank card.
    syncer
        .sync(&mut state, &pipeline, &mut driver, &CANVAS, &bindings)
        .unwrap();
    let after_second = pipeline.prefetch_count();
    assert!(
        after_second > after_first,
        "the self-healing prefetch re-fires: {after_first} -> {after_second}"
    );

    // The re-scheduled render lands: the grid fills without any window change.
    pipeline.set_ready(true);
    sync_until_settled(&mut syncer, &mut state, &pipeline, &mut driver, &log, &bindings, 6);
    assert_eq!(log.lock().unwrap().staged.len(), 6);
}
