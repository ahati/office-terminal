//! High-resolution performance benchmarking suite for TUI rendering, transitions, scrolling, and zoom.

pub mod interactive;
pub mod render;
pub mod transitions;

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use office_terminal_core::{
    DisplayError, InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec,
    RgbaSlide,
};
use office_terminal_display::{
    DriverCaps, ImageId, Placement, PlacementMode, TerminalGraphicsDriver,
};

pub use interactive::{
    bench_scroll_reader_vertical, bench_scroll_sorter_grid, bench_zoom_ladder_step,
    bench_zoom_pan_reposition,
};
pub use render::{
    bench_clear_letterbox, bench_compute_zoomed_slide_fit, bench_render_normal,
    bench_render_presenter,
};
pub use transitions::{
    bench_graphics_syncer_reposition, bench_graphics_syncer_steady_state,
    bench_page_transition_jump, bench_page_transition_next, bench_page_transition_presenter,
};

/// Metrics collected across benchmark iterations.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: usize,
    pub min_duration: Duration,
    pub p50_duration: Duration,
    pub mean_duration: Duration,
    pub p95_duration: Duration,
    pub max_duration: Duration,
    pub ops_per_sec: f64,
}

/// Executes a function with warmup and measured iterations, producing statistical metrics.
pub fn run_benchmark<F: FnMut()>(name: &str, warmup: usize, iterations: usize, mut f: F) -> BenchmarkResult {
    for _ in 0..warmup {
        f();
    }

    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        durations.push(start.elapsed());
    }

    durations.sort_unstable();

    let total: Duration = durations.iter().sum();
    let min_duration = *durations.first().unwrap_or(&Duration::ZERO);
    let p50_duration = durations[iterations / 2];
    let p95_duration = durations[(iterations * 95) / 100];
    let max_duration = *durations.last().unwrap_or(&Duration::ZERO);
    let mean_duration = if iterations > 0 {
        total / iterations as u32
    } else {
        Duration::ZERO
    };

    let total_secs = total.as_secs_f64();
    let ops_per_sec = if total_secs > 0.0 {
        iterations as f64 / total_secs
    } else {
        0.0
    };

    BenchmarkResult {
        name: name.to_string(),
        iterations,
        min_duration,
        p50_duration,
        mean_duration,
        p95_duration,
        max_duration,
        ops_per_sec,
    }
}

/// Mock pipeline for deterministic benchmark execution.
#[derive(Clone, Debug, Default)]
pub struct BenchPipeline;

impl RenderPipeline for BenchPipeline {
    fn prefetch(&self, _plan: &PrefetchPlan, _target: &RenderTargetSpec) {}
    fn entry(&self, _index: usize) -> PipelineEntry {
        PipelineEntry::Ready {
            bitmap: RgbaSlide {
                width: 640,
                height: 360,
                pixels: vec![0u8; 640 * 360 * 4].into(),
            },
        }
    }
    fn invalidate(&self, _cause: InvalidateCause) {}
}

/// Mock driver recording graphics command invocations without terminal I/O.
#[derive(Clone, Debug, Default)]
pub struct BenchDriver {
    pub staged: usize,
    pub presented: usize,
    pub repositioned: usize,
    pub released: usize,
}

impl TerminalGraphicsDriver for BenchDriver {
    fn name(&self) -> &'static str {
        "bench-driver"
    }
    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: false,
            resident_ids: 8,
        }
    }
    fn placement_mode(&self) -> PlacementMode {
        PlacementMode::Interactive
    }
    fn stage(&mut self, _bmp: &RgbaSlide, _id: ImageId) -> Result<(), DisplayError> {
        self.staged += 1;
        Ok(())
    }
    fn present(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        self.presented += 1;
        Ok(())
    }
    fn reposition(&mut self, _id: ImageId, _p: Placement) -> Result<(), DisplayError> {
        self.repositioned += 1;
        Ok(())
    }
    fn release(&mut self, _id: ImageId) -> Result<(), DisplayError> {
        self.released += 1;
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), DisplayError> {
        Ok(())
    }
}

/// Runs the complete benchmark suite covering rendering, transitions, scrolling, and zooming.
pub fn run_all_benchmarks() -> Vec<BenchmarkResult> {
    let mut results = Vec::new();

    // 1. Core rendering & microbenchmarks
    results.extend(bench_render_normal());
    results.extend(bench_render_presenter());
    results.extend(bench_clear_letterbox());
    results.extend(bench_compute_zoomed_slide_fit());

    // 2. Page transitions
    results.push(bench_page_transition_next());
    results.push(bench_page_transition_jump());
    results.push(bench_page_transition_presenter());

    // 3. Scrolling
    results.push(bench_scroll_reader_vertical());
    results.push(bench_scroll_sorter_grid());

    // 4. Zooming & graphics syncer
    results.push(bench_zoom_ladder_step());
    results.push(bench_zoom_pan_reposition());
    results.push(bench_graphics_syncer_steady_state());
    results.push(bench_graphics_syncer_reposition());

    results
}

fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{} ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2} µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.3} s", d.as_secs_f64())
    }
}

fn format_ops(ops: f64) -> String {
    if ops >= 1_000_000.0 {
        format!("{:.2} M", ops / 1_000_000.0)
    } else if ops >= 1_000.0 {
        format!("{:.1} k", ops / 1_000.0)
    } else {
        format!("{:.1}", ops)
    }
}

/// Formats benchmark results into a clean GitHub Flavored Markdown table.
pub fn format_benchmark_results(results: &[BenchmarkResult]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "| Benchmark | Iterations | Min | Median (p50) | Mean | p95 | Max | Throughput |"
    );
    let _ = writeln!(
        out,
        "| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |"
    );
    for r in results {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} ops/s |",
            r.name,
            r.iterations,
            format_duration(r.min_duration),
            format_duration(r.p50_duration),
            format_duration(r.mean_duration),
            format_duration(r.p95_duration),
            format_duration(r.max_duration),
            format_ops(r.ops_per_sec),
        );
    }
    out
}
