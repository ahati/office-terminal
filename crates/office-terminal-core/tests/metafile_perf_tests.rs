//! NFR budget smoke for metafile playback (PRD 13 §9.2 / PRD 13a §8).
//!
//! Budgets only make sense in an optimized build; debug runs return early.
//! Run with `cargo test --release -p office-terminal-core --test metafile_perf_tests`.

#![cfg(feature = "metafile")]

mod metafile_support;

use std::time::Instant;

use metafile_support as support;
use office_terminal_core::contracts::{
    AffineTransform, PathSegment, PositionedGlyph, VectorCommand, VectorPaint, VectorSlide, VectorStroke,
};
use office_terminal_core::metafile::engine_metafile_rs::MetafileRsEngine;
use office_terminal_core::metafile::recorder::MetafileRecorder;
use office_terminal_core::metafile::{self, MetafileEngine, MetafileError, MetafileLimits};

const EMF: &str = "image/x-emf";

fn compile_scene() -> VectorSlide {
    let (compile_tx, _compile_rx) = crossbeam_channel::unbounded();
    support::compile_slide(&support::picture_slide("ppt/media/image1.emf", EMF), &compile_tx)
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
    assert!(warm <= 5.0, "a cache hit must not replay playback, took {warm:.2} ms");
    assert!(warm < cold, "cache hit {warm:.2} ms must beat cold {cold:.2} ms");
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

fn command_bytes(commands: &[VectorCommand]) -> usize {
    use std::mem::size_of;
    fn paint_bytes(paint: &VectorPaint) -> usize {
        match paint {
            VectorPaint::Solid(_) => 4,
            VectorPaint::LinearGradient(g) | VectorPaint::RadialGradient(g) => 8 + g.stops.len() * 8,
            VectorPaint::Image(fill) => 64 + fill.rel_id.len(),
            VectorPaint::Pattern(_) => 64,
            VectorPaint::Texture(tex) => 64 + tex.png.len(),
        }
    }
    fn stroke_bytes(stroke: &VectorStroke) -> usize {
        32 + stroke.dash_segments.as_ref().map_or(0, |dash| dash.len() * 4)
    }
    commands
        .iter()
        .map(|command| match command {
            VectorCommand::DrawPath { path, fill, stroke, .. } => {
                path.segments.len() * size_of::<PathSegment>()
                    + fill.as_ref().map_or(0, paint_bytes)
                    + stroke.as_ref().map_or(0, stroke_bytes)
            }
            VectorCommand::DrawLineEnd { path, fill, stroke, .. } => {
                path.segments.len() * size_of::<PathSegment>()
                    + fill.as_ref().map_or(0, paint_bytes)
                    + stroke.as_ref().map_or(0, stroke_bytes)
            }
            VectorCommand::DrawText { runs, .. } => runs
                .iter()
                .map(|run| run.text.len() + run.glyphs.len() * size_of::<PositionedGlyph>() + 64)
                .sum(),
            VectorCommand::DrawBitmap { png, .. } => png.len() + 64,
            VectorCommand::DrawImage { rel_id, .. } | VectorCommand::DrawMetafile { rel_id, .. } => rel_id.len(),
            VectorCommand::PushClip(path) => path.segments.len() * size_of::<PathSegment>(),
            VectorCommand::PopClip => 0,
        })
        .sum::<usize>()
        + std::mem::size_of_val(commands)
}

#[test]
fn large_streams_and_repeated_pictures_stay_bounded() {
    if cfg!(debug_assertions) {
        eprintln!("skipped: release-only metafile budget");
        return;
    }
    // >10 MiB total: 1M points (~8 MiB) plus an ignored GDI comment.
    let padding = vec![0u8; 3_000_000];
    let bytes = support::emf(&[support::polyline(&points(1_000_000)), support::emf_comment(&padding)]);
    assert!(bytes.len() > 10 * 1024 * 1024, "expected a >10 MiB stream");
    let store = support::FixtureStore::new("ppt/media/image1.emf", bytes.clone());

    metafile::clear_global_cache();
    let mut scene = compile_scene();
    let cold = expand_ms(&mut scene, &store);
    println!("1M-point polyline: {cold:.2} ms");
    assert!(
        cold <= 250.0,
        "large playback should stay under 250 ms, took {cold:.2} ms"
    );
    let expanded_bytes = command_bytes(&scene.commands);
    println!(
        "command bytes {expanded_bytes} vs input {} ({:.1}x)",
        bytes.len(),
        expanded_bytes as f64 / bytes.len() as f64
    );
    assert!(expanded_bytes <= bytes.len() * 8, "display list exceeds 8x input");

    // A repeated picture on many slides must be cache hits, not replays.
    let small = support::emf(&[support::polyline(&points(20_000))]);
    let small_store = support::FixtureStore::new("ppt/media/image1.emf", small);
    metafile::clear_global_cache();
    let mut first = compile_scene();
    let _ = expand_ms(&mut first, &small_store);
    let started = Instant::now();
    for _ in 0..50 {
        let mut slide = compile_scene();
        let _ = expand_ms(&mut slide, &small_store);
    }
    let repeats = started.elapsed().as_secs_f64() * 1000.0;
    println!("50 repeated pictures: {repeats:.2} ms");
    assert!(repeats <= 250.0, "50 cached pictures took {repeats:.2} ms");
}

#[test]
fn dc_depth_limit_is_bounded() {
    let mut within = Vec::new();
    for id in 0..64u32 {
        within.push(support::emf_plus_save(id));
    }
    for id in (0..64u32).rev() {
        within.push(support::emf_plus_restore(id));
    }
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let result = MetafileRsEngine.play(
        &support::emf_plus_only_emf(&within),
        &MetafileLimits::default(),
        &mut recorder,
    );
    assert!(result.is_ok(), "64-deep save/restore must fit: {result:?}");

    let mut over = Vec::new();
    for id in 0..65u32 {
        over.push(support::emf_plus_save(id));
    }
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let result = MetafileRsEngine.play(
        &support::emf_plus_only_emf(&over),
        &MetafileLimits::default(),
        &mut recorder,
    );
    assert!(matches!(result, Err(MetafileError::Limit(_))), "{result:?}");
}

#[test]
fn container_depth_limit_is_bounded() {
    let mut within = Vec::new();
    for id in 0..256u32 {
        within.push(support::emf_plus_begin_container(id));
    }
    for id in (0..256u32).rev() {
        within.push(support::emf_plus_end_container(id));
    }
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let result = MetafileRsEngine.play(
        &support::emf_plus_only_emf(&within),
        &MetafileLimits::default(),
        &mut recorder,
    );
    assert!(result.is_ok(), "256 nested containers must fit: {result:?}");

    let mut over = Vec::new();
    for id in 0..257u32 {
        over.push(support::emf_plus_begin_container(id));
    }
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let result = MetafileRsEngine.play(
        &support::emf_plus_only_emf(&over),
        &MetafileLimits::default(),
        &mut recorder,
    );
    assert!(matches!(result, Err(MetafileError::Limit(_))), "{result:?}");
}
