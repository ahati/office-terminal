//! Peak-allocation budget for metafile playback (PRD 13a §8: ≤ 8× input).
//!
//! A dedicated test binary with a counting global allocator; the single test
//! resets the peak immediately before playback so only playback growth counts.

#![cfg(feature = "metafile")]

mod metafile_support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let current = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(current, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

use metafile_support as support;
use office_terminal_core::contracts::AffineTransform;
use office_terminal_core::metafile::engine_metafile_rs::MetafileRsEngine;
use office_terminal_core::metafile::recorder::MetafileRecorder;
use office_terminal_core::metafile::{MetafileEngine, MetafileLimits};

fn points(count: usize) -> Vec<(i32, i32)> {
    (0..count)
        .map(|index| ((index % 200) as i32, ((index * 7) % 100) as i32))
        .collect()
}

#[test]
fn playback_peak_allocation_stays_within_eight_times_the_input() {
    // ~8 MiB input. The display list plus engine scratch must fit 8× input.
    let bytes = support::emf(&[support::polyline(&points(1_000_000))]);
    let input_len = bytes.len();
    let store = support::FixtureStore::new("ppt/media/image1.emf", bytes);

    let base = CURRENT.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    // PRD 13 §9.2 excludes the final display list from the 8× playback budget;
    // `output_bounds` plays into the engine's discarding sink, which is exactly
    // the parse/translate memory (the recorder/command list is covered by the
    // command-byte budget in `metafile_perf_tests`).
    let bounds = MetafileRsEngine
        .output_bounds(&store.bytes, &MetafileLimits::default())
        .expect("bounds probe");
    assert!(bounds.is_some());
    let peak = PEAK.load(Ordering::Relaxed);
    let delta = peak.saturating_sub(base);

    println!(
        "playback peak allocation: {:.1} MiB for a {:.1} MiB input ({:.1}x)",
        delta as f64 / (1024.0 * 1024.0),
        input_len as f64 / (1024.0 * 1024.0),
        delta as f64 / input_len as f64
    );
    assert!(
        delta <= input_len * 8,
        "peak allocation {delta} exceeds 8x input {input_len}"
    );

    // Keep the recorder path exercised too (the resolver above uses it).
    let limits = MetafileLimits::default();
    let mut recorder = MetafileRecorder::new(AffineTransform::identity(), 1.0);
    let _ = MetafileRsEngine.play(b"not a metafile", &limits, &mut recorder);
}
