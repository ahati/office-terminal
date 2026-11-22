//! Scrolling and dynamic zooming performance benchmarks.

use office_terminal_core::RenderTargetSpec;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use super::{run_benchmark, BenchDriver, BenchPipeline, BenchmarkResult};
use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::app::ui::{render_normal, render_reader, render_sorter};

/// Benchmarks continuous vertical scrolling and smooth display advance in Reader mode.
pub fn bench_scroll_reader_vertical() -> BenchmarkResult {
    let mut state = PresentationState::new(20, vec![]);
    state.enable_reader_mode(20, vec![(612.0, 792.0); 20]);

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");
    let area = Rect::new(0, 0, 160, 50);

    run_benchmark("scroll_reader_vertical_continuous", 50, 500, || {
        if let Some(reader) = state.reader_state_mut() {
            reader.scroll_vertical(25);
            let _ = reader.advance_display();
        }
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_reader(&state, f, area, &mut bindings));
    })
}

/// Benchmarks 2D thumbnail grid navigation and card scrolling in Slide Sorter mode.
pub fn bench_scroll_sorter_grid() -> BenchmarkResult {
    let mut state = PresentationState::new(48, vec![None; 48]);
    state.set_mode(AppMode::Sorter);

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");
    let area = Rect::new(0, 0, 160, 50);

    run_benchmark("scroll_sorter_grid_navigation", 50, 500, || {
        if !state.sorter_down() {
            state.sorter_home();
        }
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_sorter(&state, f, area, &mut bindings));
    })
}

/// Benchmarks discrete zoom ladder stepping (zoom in / zoom out) with dynamic layout.
pub fn bench_zoom_ladder_step() -> BenchmarkResult {
    let mut state = PresentationState::new(5, vec![None; 5]);
    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");
    let area = Rect::new(0, 0, 160, 50);

    let mut zoom_in_dir = true;
    run_benchmark("zoom_ladder_stepping", 50, 500, || {
        if zoom_in_dir {
            if !state.nav_mut().zoom_in() {
                zoom_in_dir = false;
            }
        } else if !state.nav_mut().zoom_out() {
            zoom_in_dir = true;
        }
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_normal(&state, f, area, &mut bindings));
    })
}

/// Benchmarks pan movements inside a zoomed slide and graphics driver subrect repositioning.
pub fn bench_zoom_pan_reposition() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let mut state = PresentationState::new(5, vec![None; 5]);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");
    let area = Rect::new(0, 0, 160, 50);

    // Zoom into 2.0x so bounds overflow and panning is active
    state.set_scale(2.0);

    // Prime initial frame and sync
    let mut bindings = FrameBindings::default();
    let _ = terminal.draw(|f| render_normal(&state, f, area, &mut bindings));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings)
        .expect("initial sync succeeds");

    let mut pan_dir = 2.0f32;
    run_benchmark("zoom_pan_subrect_reposition", 50, 1000, || {
        if !state.pan_zoom(pan_dir, 0.0) {
            pan_dir = -pan_dir;
            state.pan_zoom(pan_dir, 0.0);
        }
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_normal(&state, f, area, &mut bindings));
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, &bindings);
        let _ = std::hint::black_box(res);
    })
}
