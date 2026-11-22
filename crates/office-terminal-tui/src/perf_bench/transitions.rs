//! Slide page transition and graphics synchronization benchmarks.

use office_terminal_core::RenderTargetSpec;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::{run_benchmark, BenchDriver, BenchPipeline, BenchmarkResult};
use crate::app::bindings::FrameBindings;
use crate::app::graphics::GraphicsSyncer;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::app::ui::render_app;

/// Benchmarks sequential page turning (`next_slide`) with end-to-end draw and graphics sync.
pub fn bench_page_transition_next() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let mut state = PresentationState::new(20, vec![None; 20]);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");

    // Prime the first frame
    let mut bindings = FrameBindings::default();
    let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings)
        .expect("initial sync succeeds");

    run_benchmark("page_transition_next_slide", 50, 500, || {
        state.next_slide();
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, &bindings);
        let _ = std::hint::black_box(res);
    })
}

/// Benchmarks distant slide jumps with end-to-end draw and graphics sync.
pub fn bench_page_transition_jump() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let mut state = PresentationState::new(30, vec![None; 30]);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");

    // Prime the first frame
    let mut bindings = FrameBindings::default();
    let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings)
        .expect("initial sync succeeds");

    let targets = [0, 15, 2, 28, 5, 22, 10, 1];
    let mut idx = 0;

    run_benchmark("page_transition_jump_to_slide", 50, 500, || {
        let target = targets[idx % targets.len()];
        idx += 1;
        state.jump_to_slide(target);
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, &bindings);
        let _ = std::hint::black_box(res);
    })
}

/// Benchmarks slide transitions in Presenter Console view (updating current + preview slots).
pub fn bench_page_transition_presenter() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let notes = vec![
        Some("Slide notes to verify speaker notes formatting during transition.".to_string());
        20
    ];
    let mut state = PresentationState::new(20, notes);
    state.set_mode(AppMode::PresenterConsole);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");

    // Prime initial frame
    let mut bindings = FrameBindings::default();
    let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings)
        .expect("initial sync succeeds");

    run_benchmark("page_transition_presenter_view", 50, 500, || {
        state.next_slide();
        let mut bindings = FrameBindings::default();
        let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, &bindings);
        let _ = std::hint::black_box(res);
    })
}

/// Benchmarks steady-state graphics synchronization throughput (zero-allocation bypass).
pub fn bench_graphics_syncer_steady_state() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let mut state = PresentationState::new(5, vec![None; 5]);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("valid terminal backend");
    let mut bindings = FrameBindings::default();
    let _ = terminal.draw(|f| render_app(&state, f, &mut bindings));

    // Warm-up initial stage and placement
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings)
        .expect("initial sync succeeds");

    run_benchmark("graphics_syncer_steady_state", 100, 2000, || {
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, &bindings);
        let _ = std::hint::black_box(res);
    })
}

/// Benchmarks non-destructive resize / repositioning latency of graphics syncer.
pub fn bench_graphics_syncer_reposition() -> BenchmarkResult {
    let pipeline = BenchPipeline;
    let mut driver = BenchDriver::default();
    let mut state = PresentationState::new(5, vec![None; 5]);
    let mut syncer = GraphicsSyncer::new();
    let canvas = RenderTargetSpec {
        width_px: 640,
        height_px: 360,
    };

    let backend_a = TestBackend::new(80, 24);
    let mut terminal_a = Terminal::new(backend_a).expect("valid terminal backend");
    let mut bindings_a = FrameBindings::default();
    let _ = terminal_a.draw(|f| render_app(&state, f, &mut bindings_a));

    let backend_b = TestBackend::new(100, 30);
    let mut terminal_b = Terminal::new(backend_b).expect("valid terminal backend");
    let mut bindings_b = FrameBindings::default();
    let _ = terminal_b.draw(|f| render_app(&state, f, &mut bindings_b));

    // Warm-up initial sync
    syncer
        .sync(&mut state, &pipeline, &mut driver, &canvas, &bindings_a)
        .expect("initial sync succeeds");

    let mut toggle = false;
    run_benchmark("graphics_syncer_reposition", 50, 1000, || {
        let b = if toggle { &bindings_a } else { &bindings_b };
        toggle = !toggle;
        let res = syncer.sync(&mut state, &pipeline, &mut driver, &canvas, b);
        let _ = std::hint::black_box(res);
    })
}
