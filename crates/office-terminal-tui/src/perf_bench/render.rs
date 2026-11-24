//! Rendering and layout microbenchmarks.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;

use super::{run_benchmark, BenchmarkResult};
use crate::app::bindings::FrameBindings;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::app::ui::render_app;
use crate::widget::canvas::clear_letterbox;
use crate::widget::layout::{
    compute_slide_rect, ASPECT_16_9, ASPECT_4_3, DEFAULT_CELL_ASPECT_RATIO,
};
use crate::widget::zoom_fit::compute_zoomed_slide_fit;

/// Benchmarks fullscreen presentation rendering at multiple terminal resolutions.
pub fn bench_render_normal() -> Vec<BenchmarkResult> {
    let sizes = [("80x24", 80u16, 24u16), ("160x50", 160, 50), ("320x100", 320, 100)];
    let mut results = Vec::with_capacity(sizes.len());

    for (label, w, h) in sizes {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("valid terminal backend");
        let state = PresentationState::new(10, vec![None; 10]);

        let name = format!("render_normal_{}", label);
        results.push(run_benchmark(&name, 30, 300, || {
            let mut bindings = FrameBindings::default();
            let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
        }));
    }

    results
}

/// Benchmarks presenter console rendering at multiple terminal resolutions.
pub fn bench_render_presenter() -> Vec<BenchmarkResult> {
    let sizes = [("80x24", 80u16, 24u16), ("160x50", 160, 50), ("320x100", 320, 100)];
    let mut results = Vec::with_capacity(sizes.len());

    for (label, w, h) in sizes {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("valid terminal backend");
        let notes = vec![
            Some("Detailed speaker notes to benchmark presenter text formatting and wrapping.".to_string());
            10
        ];
        let mut state = PresentationState::new(10, notes);
        state.set_mode(AppMode::PresenterConsole);

        let name = format!("render_presenter_{}", label);
        results.push(run_benchmark(&name, 30, 300, || {
            let mut bindings = FrameBindings::default();
            let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
        }));
    }

    results
}

/// Benchmarks direct letterbox and pillarbox clearing.
pub fn bench_clear_letterbox() -> Vec<BenchmarkResult> {
    let mut results = Vec::with_capacity(3);

    // Case 1: 16:9 slide in 4:3 area (top/bottom letterbox bars)
    {
        let total = Rect::new(0, 0, 100, 50);
        let fitted = compute_slide_rect(total, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        let mut buf = Buffer::empty(total);
        results.push(run_benchmark("clear_letterbox_16_9_in_100x50", 50, 1000, || {
            clear_letterbox(&mut buf, total, fitted, Color::Black);
        }));
    }

    // Case 2: 4:3 slide in 16:9 area (left/right pillarbox bars)
    {
        let total = Rect::new(0, 0, 160, 50);
        let fitted = compute_slide_rect(total, ASPECT_4_3, DEFAULT_CELL_ASPECT_RATIO);
        let mut buf = Buffer::empty(total);
        results.push(run_benchmark("clear_letterbox_4_3_in_160x50", 50, 1000, || {
            clear_letterbox(&mut buf, total, fitted, Color::Black);
        }));
    }

    // Case 3: 320x100 high-resolution area
    {
        let total = Rect::new(0, 0, 320, 100);
        let fitted = compute_slide_rect(total, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        let mut buf = Buffer::empty(total);
        results.push(run_benchmark("clear_letterbox_16_9_in_320x100", 50, 1000, || {
            clear_letterbox(&mut buf, total, fitted, Color::Black);
        }));
    }

    results
}

/// Microbenchmarks geometric zoom and placement solver.
pub fn bench_compute_zoomed_slide_fit() -> Vec<BenchmarkResult> {
    let area = Rect::new(0, 0, 160, 50);
    let mut results = Vec::with_capacity(3);

    results.push(run_benchmark("compute_zoomed_fit_1.0x", 100, 2000, || {
        let fit = compute_zoomed_slide_fit(
            area,
            ASPECT_16_9,
            DEFAULT_CELL_ASPECT_RATIO,
            1.0,
            0.0,
            0.0,
        );
        std::hint::black_box(fit);
    }));

    results.push(run_benchmark("compute_zoomed_fit_2.0x_panned", 100, 2000, || {
        let fit = compute_zoomed_slide_fit(
            area,
            ASPECT_16_9,
            DEFAULT_CELL_ASPECT_RATIO,
            2.0,
            10.5,
            -5.2,
        );
        std::hint::black_box(fit);
    }));

    results.push(run_benchmark("compute_zoomed_fit_4.0x_panned", 100, 2000, || {
        let fit = compute_zoomed_slide_fit(
            area,
            ASPECT_16_9,
            DEFAULT_CELL_ASPECT_RATIO,
            4.0,
            -25.0,
            15.0,
        );
        std::hint::black_box(fit);
    }));

    results
}
