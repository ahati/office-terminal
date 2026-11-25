//! Integration test running performance benchmarks and asserting baseline throughput invariants.
//!
//! Run with:
//! `cargo test -p office-terminal-tui --test perf_bench_tests -- --nocapture`

use office_terminal_tui::perf_bench::{format_benchmark_results, run_all_benchmarks};

#[test]
fn test_tui_performance_benchmarks_and_invariants() {
    let results = run_all_benchmarks();
    println!("\n{}", format_benchmark_results(&results));

    // Assert baseline performance invariants
    for res in &results {
        assert!(
            res.iterations > 0,
            "benchmark {} should have executed iterations",
            res.name
        );
        assert!(res.ops_per_sec > 0.0, "benchmark {} ops/sec must be positive", res.name);

        // Sanity checks: Ensure performance regressions do not exceed generous safety bounds
        if res.name.starts_with("clear_letterbox_") {
            // Direct letterbox fills should complete in under 500 µs (typically < 10 µs)
            assert!(
                res.p50_duration.as_micros() < 500,
                "letterbox clearing took too long: {:?}",
                res.p50_duration
            );
        } else if res.name.starts_with("compute_zoomed_fit_") {
            // Zoom geometric calculations should complete in under 100 µs (typically < 1 µs)
            assert!(
                res.p50_duration.as_micros() < 100,
                "zoom fit calculation took too long: {:?}",
                res.p50_duration
            );
        } else if res.name == "graphics_syncer_steady_state" {
            // Steady-state graphics syncer should achieve high throughput (> 5,000 ops/sec)
            assert!(
                res.ops_per_sec > 5000.0,
                "graphics syncer steady state throughput too low: {:.1} ops/sec",
                res.ops_per_sec
            );
        } else if res.name.starts_with("page_transition_") {
            // Page transitions with full render + graphics sync should complete in under 5 ms
            assert!(
                res.p50_duration.as_millis() < 5,
                "page transition took too long: {:?}",
                res.p50_duration
            );
        } else if res.name.starts_with("scroll_") {
            // Interactive scrolling should easily support 60 FPS (< 16.6 ms budget, typically < 2 ms)
            assert!(
                res.p50_duration.as_millis() < 10,
                "scrolling pass took too long: {:?}",
                res.p50_duration
            );
        } else if res.name.starts_with("zoom_") {
            // Dynamic zooming and panning reposition should complete well within frame budget
            assert!(
                res.p50_duration.as_millis() < 10,
                "zooming/panning pass took too long: {:?}",
                res.p50_duration
            );
        }
    }
}
