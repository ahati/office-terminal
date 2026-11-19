//! Viewer-worker parity batch — the reference's worker/scheduler contracts
//! mapped onto this pipeline's equivalent seam: `WorkerPool` (priority queue,
//! cooperative cancellation, failure channel) and `DefaultRenderPipeline`
//! (`prefetch` window, target supersede).
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `presentation-progressive.test.ts` (7 cases: prefix publication, opening
//! slide first, background failure surfacing, malformed prefix),
//! `worker-task-scheduler.test.ts` (1), `slide-pull-worker.test.ts` (24),
//! `slide-pull-client.test.ts` (4), `render-worker-canvas-transfer.test.ts` (3),
//! `presentation-worker-bitmap-ownership.test.ts` (6),
//! `worker-renderer-modules.test.ts` (1), `worker-init-hang.test.ts` (7),
//! `render-worker-init-hang.test.ts` (7), `renderer-destroy-invalidation.test.ts`
//! (1), `presentation-bounded-pipeline.test.ts` (1),
//! `presentation-canvas-realm.test.ts` (1).
//!
//! N/A: `worker-task-scheduler.ts` crosses a host *task* boundary
//! (`MessageChannel`) — Rust has no microtask/task split in this pipeline, so
//! there is nothing to port; the canvas-transfer / bitmap-ownership /
//! module-registry specs describe `OffscreenCanvas` ownership, which the terminal
//! pipeline does not have (frames are `Vec<u8>` RGBA).
//!
//! BLOCKED (no production API, see `tests/parity/coverage/viewer.md`):
//! `BoundedPullSession` / `pullWithCreditRetry` (no byte-credit pull protocol),
//! worker init timeout (no hang bound) and the worker selection/find realm.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_viewer_worker_tests.rs.txt`; the
//! ledger keeps their rows.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use kitty_pptx_core::contracts::{
    InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec, RgbaSlide,
    VectorSlide, WarningEvent,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::pipeline::{PrefetchTask, WorkerPool};
use kitty_pptx_core::rasterizer::SlideRasterizer;
use kitty_pptx_core::{
    AssetError, CancellationToken, DefaultRenderPipeline, DefaultSlideCompiler, RenderError,
    SlideCompiler, TinySkiaRasterizer,
};

const TARGET: RenderTargetSpec = RenderTargetSpec {
    width_px: 160,
    height_px: 90,
};

// ── fixtures ────────────────────────────────────────────────────────────────

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("none".into()))
    }
    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

fn deck_of(count: usize) -> Arc<pptx_model::Presentation> {
    let slides = (0..count)
        .map(|index| {
            serde_json::from_value::<pptx_model::Slide>(serde_json::json!({
                "index": index,
                "slideNumber": index + 1,
                "partName": format!("ppt/slides/slide{}.xml", index + 1),
                "elements": [],
                "elementSources": [],
                "comments": []
            }))
            .expect("slide")
        })
        .collect();
    Arc::new(pptx_model::Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides,
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    })
}

/// A compiler that holds every task at its gate and records the order in which
/// tasks are actually popped off the priority queue. Lets the opening-slide
/// priority be asserted without racing the workers.
struct GatedCompiler {
    released: Arc<AtomicBool>,
    started: Arc<Mutex<Vec<usize>>>,
    failing: Option<usize>,
}

impl GatedCompiler {
    fn new(failing: Option<usize>) -> Self {
        Self {
            released: Arc::new(AtomicBool::new(false)),
            started: Arc::new(Mutex::new(Vec::new())),
            failing,
        }
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
    }

    fn started(&self) -> Vec<usize> {
        self.started
            .lock()
            .map(|taken| taken.clone())
            .unwrap_or_default()
    }
}

impl SlideCompiler for GatedCompiler {
    fn compile(
        &self,
        slide: &pptx_model::Slide,
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        if let Ok(mut taken) = self.started.lock() {
            taken.push(slide.index);
        }
        if self.failing == Some(slide.index) {
            return Err(RenderError::CompilationFailed(format!(
                "slide {} is poisoned",
                slide.index
            )));
        }
        while !self.released.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(1));
        }
        DefaultSlideCompiler::new().compile(slide, slide_width, slide_height, warning_sink)
    }
}

fn pipeline_with(
    count: usize,
    compiler: Arc<GatedCompiler>,
    workers: usize,
) -> DefaultRenderPipeline {
    DefaultRenderPipeline::with_worker_count(
        deck_of(count),
        Arc::new(NoAssets),
        compiler,
        Arc::new(TinySkiaRasterizer::new()),
        workers,
    )
}

fn plan(center: usize, behind: usize, ahead: usize) -> PrefetchPlan {
    PrefetchPlan {
        center,
        behind,
        ahead,
        stage: true,
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    predicate()
}

// ── prefetch window / priority (ref presentation-progressive.test.ts:199-348) ─

/// "publishes the opening slide while keeping the final slide count stable": the
/// active slide is scheduled at maximum priority, so it is the first task the
/// pool ever pops. ref presentation-progressive.test.ts:199-260
#[test]
fn prefetch_schedules_the_opening_slide_first() {
    let compiler = Arc::new(GatedCompiler::new(None));
    let pipeline = pipeline_with(8, Arc::clone(&compiler), 1);

    pipeline.prefetch(&plan(3, 1, 2), &TARGET);
    assert!(
        wait_until(|| !compiler.started().is_empty()),
        "a worker picks up a task"
    );
    let started = compiler.started();
    assert_eq!(started[0], 3, "the center slide is compiled first");
    compiler.release();
}

/// The window is exactly `center ± (behind/ahead)`; nothing outside it is
/// scheduled, and everything inside it lands in the cache once released.
/// ref presentation-progressive.test.ts:199-260 (prefix ordering)
#[test]
fn prefetch_window_holds_only_the_planned_slides() {
    let compiler = Arc::new(GatedCompiler::new(None));
    let pipeline = pipeline_with(8, Arc::clone(&compiler), 2);

    pipeline.prefetch(&plan(4, 1, 1), &TARGET);
    assert!(wait_until(|| pipeline.workers().in_flight_count() >= 2));
    for index in [3usize, 4, 5] {
        assert!(
            pipeline.workers().is_in_flight(index),
            "slide {index} in flight"
        );
    }
    for index in [0usize, 2, 6, 7] {
        assert!(
            !pipeline.workers().is_in_flight(index),
            "slide {index} untouched"
        );
    }

    compiler.release();
    assert!(wait_until(|| {
        [3usize, 4, 5]
            .iter()
            .all(|index| pipeline.cache().contains_raster(*index, &TARGET))
    }));
    assert_eq!(pipeline.workers().in_flight_count(), 0);
}

/// A background failure surfaces on `entry()` without disturbing the slides that
/// did render — the reference rethrows the failure after publishing the opening
/// slide. ref presentation-progressive.test.ts:350-444
#[test]
fn a_background_failure_is_reported_alongside_a_ready_opening_slide() {
    let compiler = Arc::new(GatedCompiler::new(Some(2)));
    let pipeline = pipeline_with(4, Arc::clone(&compiler), 2);
    compiler.release(); // slide 2 fails regardless of the gate

    pipeline.prefetch(&plan(0, 0, 2), &TARGET);
    assert!(
        wait_until(|| matches!(pipeline.entry(0), PipelineEntry::Ready { .. })),
        "the opening slide renders"
    );
    assert!(
        wait_until(|| matches!(pipeline.entry(2), PipelineEntry::Failed { .. })),
        "the failing slide is reported"
    );
    match pipeline.entry(2) {
        PipelineEntry::Failed { error } => assert_eq!(
            error,
            RenderError::CompilationFailed("slide 2 is poisoned".to_string())
        ),
        other => panic!("expected Failed for slide 2, got {}", entry_name(&other)),
    }
    assert!(pipeline.cache().contains_raster(0, &TARGET));
    assert!(!pipeline.cache().contains_raster(2, &TARGET));
}

/// A changed pixel target supersedes the prefetched rasters: the old-resolution
/// frames are dropped (never served as fresh) and the vector tier survives.
/// ref renderer-destroy-invalidation.test.ts:19-66, `InvalidateCause::TargetChanged`
#[test]
fn a_new_target_supersedes_the_prefetched_rasters() {
    let compiler = Arc::new(GatedCompiler::new(None));
    let pipeline = pipeline_with(3, Arc::clone(&compiler), 2);
    compiler.release();
    pipeline.prefetch(&plan(0, 0, 1), &TARGET);
    assert!(wait_until(|| pipeline.cache().contains_raster(0, &TARGET)));
    assert!(pipeline.cache().has_vector(0));

    let bigger = RenderTargetSpec {
        width_px: 320,
        height_px: 180,
    };
    pipeline.invalidate(InvalidateCause::TargetChanged);
    assert!(
        !pipeline.cache().contains_raster(0, &TARGET),
        "stale frame dropped"
    );
    assert!(pipeline.cache().has_vector(0), "vector tier survives");
    assert_eq!(pipeline.workers().in_flight_count(), 0);

    pipeline.prefetch(&plan(0, 0, 0), &bigger);
    assert!(wait_until(|| pipeline.cache().contains_raster(0, &bigger)));
    match pipeline.entry(0) {
        PipelineEntry::Ready { bitmap, .. } => {
            assert_eq!((bitmap.width, bitmap.height), (320, 180));
        }
        other => panic!(
            "expected Ready at the new target, got {}",
            entry_name(&other)
        ),
    }
}

// ── pool-level cancellation / supersede contract ───────────────────────────

/// A queued pool (zero workers) so the queue state is deterministic.
fn quiet_pool(count: usize) -> WorkerPool {
    WorkerPool::new(
        0,
        deck_of(count),
        Arc::new(NoAssets),
        Arc::new(DefaultSlideCompiler::new()),
        Arc::new(TinySkiaRasterizer::new()),
        kitty_pptx_core::pipeline::PipelineCache::new(),
    )
}

/// A slide jump cancels every token outside the new window and purges their
/// queued tasks — the supersede contract of `cancel_except`.
/// ref presentation-progressive.test.ts:199-260 (window replacement)
#[test]
fn cancel_except_cancels_tokens_and_purges_the_queue() {
    let pool = quiet_pool(6);
    let tokens: Vec<CancellationToken> = (0..6)
        .map(|index| pool.schedule(index, 10, TARGET, false))
        .collect();
    assert_eq!(pool.queue_len(), 6);
    assert_eq!(pool.in_flight_count(), 6);

    let keep: HashSet<usize> = [2usize, 3].into_iter().collect();
    pool.cancel_except(&keep);

    assert_eq!(pool.queue_len(), 2, "only the new window is queued");
    for (index, token) in tokens.iter().enumerate() {
        assert_eq!(
            token.is_canceled(),
            !keep.contains(&index),
            "token {index} cancellation follows the window"
        );
    }
    assert!(pool.is_in_flight(2) && pool.is_in_flight(3));
}

/// `cancel_all` (used by target change / deck reload) drains queue and in-flight
/// bookkeeping, and a cancelled token never resumes. ref presentation-destroy.test.ts:115-127
#[test]
fn cancel_all_drains_queue_and_in_flight() {
    let pool = quiet_pool(4);
    let tokens: Vec<CancellationToken> = (0..4)
        .map(|index| pool.schedule(index, 10, TARGET, false))
        .collect();
    pool.cancel_all();

    assert_eq!(pool.queue_len(), 0);
    assert_eq!(pool.in_flight_count(), 0);
    assert!(tokens.iter().all(|token| token.is_canceled()));
    assert!(!pool.is_in_flight(0));
}

/// Prefetching the same window twice does not double-schedule: a slide already
/// in flight is skipped, so one slide has one render session at a time.
/// ref slide-pull-client.ts:41-47 (one pull session per slide)
#[test]
fn prefetch_does_not_reschedule_a_slide_already_in_flight() {
    let compiler = Arc::new(GatedCompiler::new(None));
    let pipeline = pipeline_with(4, Arc::clone(&compiler), 1);

    pipeline.prefetch(&plan(1, 0, 0), &TARGET);
    assert!(wait_until(|| !compiler.started().is_empty()));
    assert!(pipeline.workers().is_in_flight(1));

    // Same window again while the first render is still blocked at the gate.
    pipeline.prefetch(&plan(1, 0, 0), &TARGET);
    assert_eq!(pipeline.workers().in_flight_count(), 1);
    assert_eq!(
        pipeline.workers().queue_len(),
        0,
        "no duplicate task queued"
    );
    assert_eq!(compiler.started(), vec![1], "exactly one compile started");

    compiler.release();
    assert!(wait_until(|| pipeline.cache().contains_raster(1, &TARGET)));
}

/// The priority queue is a max-heap on `priority`; equal priorities compare
/// equal (so a re-schedule cannot reorder the queue). ref worker-task-scheduler.ts:1-19
#[test]
fn prefetch_task_priority_orders_the_queue() {
    let task = |priority: u32, slide_index: usize| PrefetchTask {
        priority,
        slide_index,
        target: TARGET,
        stage: false,
        cancellation: CancellationToken::new(),
    };
    assert!(task(20, 0) > task(10, 5));
    assert_eq!(task(20, 0).cmp(&task(20, 9)), std::cmp::Ordering::Equal);
    assert!(
        task(20, 0) != task(20, 1),
        "equality includes the slide index"
    );
}

/// The pool shutdown is idempotent and leaves no work behind — the analogue of
/// the reference's `destroy()` being safe to call twice.
/// ref presentation-destroy.test.ts:123-127
#[test]
fn pool_shutdown_is_idempotent() {
    let pool = quiet_pool(3);
    pool.schedule(0, 10, TARGET, false);
    pool.schedule(1, 10, TARGET, false);
    pool.shutdown();
    assert_eq!((pool.queue_len(), pool.in_flight_count()), (0, 0));
    pool.shutdown();
    assert_eq!((pool.queue_len(), pool.in_flight_count()), (0, 0));
}

fn entry_name(entry: &PipelineEntry) -> &'static str {
    match entry {
        PipelineEntry::Ready { .. } => "Ready",
        PipelineEntry::Rendering => "Rendering",
        PipelineEntry::NotLoaded => "NotLoaded",
        PipelineEntry::Failed { .. } => "Failed",
    }
}

// ── worker panic barrier (worker hardening) ──────────────────────────────────

/// A rasterizer that panics on its first call, then succeeds. `VectorSlide`
/// carries no identity, so the once-flag pins the blast radius to the first
/// (highest-priority) task deterministically.
struct PanicsOnce {
    armed: AtomicBool,
}

impl SlideRasterizer for PanicsOnce {
    fn rasterize(
        &self,
        _scene: &VectorSlide,
        _target: &RenderTargetSpec,
        _assets: &dyn AssetStore,
    ) -> Result<RgbaSlide, RenderError> {
        if self.armed.swap(false, Ordering::AcqRel) {
            panic!("rasterizer exploded on the first slide");
        }
        Ok(RgbaSlide {
            width: 2,
            height: 2,
            pixels: Arc::from(vec![0u8; 16].into_boxed_slice()),
        })
    }
}

/// A panicking rasterize must surface as `PipelineEntry::Failed`, clear the
/// slide from `in_flight`, and leave the worker thread able to render the
/// next slide. Without the barrier the entry stays `Rendering` forever and
/// the presenter canvas stays silently blank.
#[test]
fn a_panicking_worker_fails_the_slide_and_keeps_the_pool_alive() {
    let compiler = Arc::new(GatedCompiler::new(None));
    let rasterizer = Arc::new(PanicsOnce {
        armed: AtomicBool::new(true),
    });
    let compiler_dyn: Arc<dyn SlideCompiler> = compiler.clone();
    let rasterizer_dyn: Arc<dyn SlideRasterizer> = rasterizer;
    let pipeline = DefaultRenderPipeline::with_worker_count(
        deck_of(8),
        Arc::new(NoAssets),
        compiler_dyn,
        rasterizer_dyn,
        1,
    );

    pipeline.prefetch(&plan(4, 0, 0), &TARGET);
    assert!(wait_until(|| !compiler.started().is_empty()));
    compiler.release();

    assert!(
        wait_until(|| matches!(pipeline.entry(4), PipelineEntry::Failed { .. })),
        "the panic surfaces as Failed, not an eternal Rendering"
    );
    match pipeline.entry(4) {
        PipelineEntry::Failed { error } => {
            assert!(
                error.to_string().contains("exploded"),
                "the panic payload is preserved: {error}"
            );
        }
        other => panic!("expected Failed for slide 4, got {}", entry_name(&other)),
    }
    assert!(
        !pipeline.workers().is_in_flight(4),
        "the wedged in-flight slot is released"
    );

    // The same worker thread still serves the next task.
    pipeline.prefetch(&plan(2, 0, 0), &TARGET);
    assert!(
        wait_until(|| matches!(pipeline.entry(2), PipelineEntry::Ready { .. })),
        "the pool survives the panic and renders the next slide"
    );
}
