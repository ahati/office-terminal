//! Tier 2 raster cache budget and resolution-fallback tests.

use kitty_pptx_core::contracts::{RenderTargetSpec, RgbaSlide};
use kitty_pptx_core::pipeline::PipelineCache;

fn target(width_px: u32) -> RenderTargetSpec {
    RenderTargetSpec {
        width_px,
        height_px: 1,
    }
}

/// A 4x4 raster (64 bytes) whose first pixel identifies which target produced it.
fn bitmap(marker: u8) -> RgbaSlide {
    RgbaSlide {
        width: 4,
        height: 4,
        pixels: vec![marker; 4 * 4 * 4].into(),
    }
}

#[test]
fn evicts_oldest_rasters_over_the_budget() {
    // 64 bytes per raster; 128 retains exactly two.
    let cache = PipelineCache::with_budget_bytes(128);
    cache.put_raster(0, target(320), bitmap(1));
    cache.put_raster(1, target(320), bitmap(2));
    assert_eq!(cache.raster_len(), 2);
    assert_eq!(cache.raster_bytes(), 128);

    cache.put_raster(2, target(320), bitmap(3));

    assert_eq!(cache.raster_len(), 2, "budget holds two rasters");
    assert_eq!(cache.raster_bytes(), 128);
    assert!(
        !cache.contains_raster(0, &target(320)),
        "the oldest raster is evicted first"
    );
    assert!(cache.contains_raster(1, &target(320)));
    assert!(cache.contains_raster(2, &target(320)));
}

#[test]
fn overwriting_a_target_does_not_double_count_bytes() {
    let cache = PipelineCache::with_budget_bytes(1024);
    cache.put_raster(0, target(320), bitmap(1));
    cache.put_raster(0, target(320), bitmap(9));

    assert_eq!(cache.raster_len(), 1);
    assert_eq!(cache.raster_bytes(), 64);
    assert_eq!(
        cache.get_raster(0, &target(320)).unwrap().pixels[0],
        9,
        "the newer frame wins"
    );
}

#[test]
fn best_raster_prefers_the_widest_cached_target() {
    let cache = PipelineCache::new();
    cache.put_raster(0, target(320), bitmap(32));
    cache.put_raster(0, target(960), bitmap(96));
    cache.put_raster(0, target(640), bitmap(64));

    assert_eq!(
        cache.best_raster(0).expect("a raster is cached").pixels[0],
        96,
        "the canvas fallback uses the sharpest frame"
    );
    assert!(cache.best_raster(1).is_none());
}

#[test]
fn best_raster_recovers_when_the_widest_frame_is_evicted() {
    // Two rasters fit; the newest insertion pushes the oldest (widest) out.
    let cache = PipelineCache::with_budget_bytes(128);
    cache.put_raster(0, target(960), bitmap(96));
    cache.put_raster(0, target(320), bitmap(32));
    assert_eq!(cache.best_raster(0).unwrap().pixels[0], 96);

    cache.put_raster(1, target(320), bitmap(1));

    assert!(!cache.contains_raster(0, &target(960)));
    assert_eq!(
        cache.best_raster(0).expect("the smaller frame remains").pixels[0],
        32
    );
}

#[test]
fn clearing_the_raster_cache_resets_bytes_and_index() {
    let cache = PipelineCache::with_budget_bytes(128);
    cache.put_raster(0, target(320), bitmap(1));
    assert_eq!(cache.raster_bytes(), 64);

    cache.clear_raster();

    assert_eq!(cache.raster_len(), 0);
    assert_eq!(cache.raster_bytes(), 0);
    assert!(cache.best_raster(0).is_none());
}
