//! Standalone high-resolution performance benchmark harness for `office-terminal-tui`.
//!
//! Run with:
//! `cargo bench -p office-terminal-tui`

use office_terminal_tui::perf_bench::{format_benchmark_results, run_all_benchmarks};

fn main() {
    println!("\n=== Running Office Terminal TUI Performance Benchmarks ===\n");
    let results = run_all_benchmarks();
    println!("{}", format_benchmark_results(&results));
}
