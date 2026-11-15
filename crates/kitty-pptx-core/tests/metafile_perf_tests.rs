//! NFR budget smoke for metafile playback (PRD 13 §9.2 / PRD 13a §8).
//!
//! Budgets only make sense in an optimized build; debug runs return early.
//! Run with `cargo test --release -p kitty-pptx-core --test metafile_perf_tests`.

#![cfg(feature = "metafile")]


mod metafile_support;

use std::time::Instant;

use kitty_pptx_core::contracts::{AffineTransform, VectorSlide};
use kitty_pptx_core::metafile::engine_metafile_rs::MetafileRsEngine;
use kitty_pptx_core::metafile::recorder::MetafileRecorder;
use kitty_pptx_core::metafile::{self, MetafileEngine, MetafileError, MetafileLimits};
use metafile_support as support;

const EMF: &str = "image/x-emf";

fn compile_scene() -> VectorSlide {
    let (compile_tx, _compile_rx) = crossbeam_channel::unbounded();
    support::compile_slide(
        &support::picture_slide("ppt/media/image1.emf", EMF),
        &compile_tx,
    )
}

fn expand_ms(scene: &mut VectorSlide, store: &support::FixtureStore) -> f64 {
    let (tx, _rx) = crossbeam_channel::unbounded();
    let started = Instant::now();
    let summary = metafile::expand_metafiles(scene, store, &tx);
    assert_eq!(summary.expanded, 1, "fixture must play");
    started.elapsed().as_secs_f64() * 1000.0
}

fn points(count: usize) -> Vec<(i32, i32)> {
    (0..count)
        .map(|index| ((index % 200) as i32, ((index * 7) % 100) as i32))
        .collect()
}

#[test]
fn playback_and_cache_hit_stay_within_budget() {
    if cfg!(debug_assertions) {
        eprintln!("skipped: release-only metafile budget");
        return;
    }
    // ~160 KB of geometry: inside the "typical EMF <= 1 MiB" budget.
    let bytes = support::emf(&[support::polyline(&points(20_000))]);
    let store = support::FixtureStore::new("ppt/media/image1.emf", bytes);

    metafile::clear_global_cache();
    let mut scene = compile_scene();
    let cold = expand_ms(&mut scene, &store);
    println!("playback+translate (cold): {cold:.2} ms");
    assert!(
        cold <= 50.0,
        "typical playback should stay under 50 ms, took {cold:.2} ms"
    );

    // Same rel_id/content/crop/destination: content-addressed cache hit.
    let mut warm_scene = compile_scene();
    let warm = expand_ms(&mut warm_scene, &store);
    println!("playback+translate (cache hit): {warm:.2} ms");
    assert!(
        warm <= 5.0,
        "a cache hit must not replay playback, took {warm:.2} ms"
    );
    assert!(
        warm < cold,
        "cache hit {warm:.2} ms must beat cold {cold:.2} ms"
    );
}

#[test]
fn a_resource_limit_aborts_promptly() {
    let bytes = support::emf(&[support::polyline(&points(5_000))]);
    let limits = MetafileLimits {
        max_points_per_record: 100,
        ..MetafileLimits::default()
    };
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let started = Instant::now();
    let result = MetafileRsEngine.play(&bytes, &limits, &mut recorder);
    let (commands, _) = recorder.finish();
    assert!(matches!(result, Err(MetafileError::Limit(_))), "{result:?}");
    assert!(commands.len() < 10, "limit must stop before recording ink");
    assert!(started.elapsed().as_secs() < 1, "limit must abort promptly");
}
