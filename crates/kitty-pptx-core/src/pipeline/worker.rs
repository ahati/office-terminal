use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crossbeam_channel::{Receiver, Sender};

use crate::compiler::SlideCompiler;
use crate::contracts::{PageSource, PptxPageSource, RenderTargetSpec, TransmitEncoder, WarningEvent};
use crate::error::RenderError;
use crate::loader::AssetStore;
use crate::rasterizer::SlideRasterizer;
use super::cache::PipelineCache;
use super::cancellation::CancellationToken;
use super::payload::PayloadCache;
use super::render;

/// Shared, late-bindable slot for the off-thread transmission encoder.
///
/// The pool is created before the pipeline builder attaches an encoder, so
/// the slot is shared into every worker context and mutated through
/// [`WorkerPool::set_transmit_encoder`].
type TransmitSlot = Arc<Mutex<Option<Arc<dyn TransmitEncoder>>>>;

/// Work item representing a pending slide pre-rasterization task.
#[derive(Clone)]
pub struct PrefetchTask {
    /// Priority level (higher values pop first from the priority queue).
    pub priority: u32,
    /// Target slide index (0-based).
    pub slide_index: usize,
    /// Destination render resolution.
    pub target: RenderTargetSpec,
    /// Pre-encode the terminal transmission payload after rasterizing.
    pub stage: bool,
    /// Cooperative cancellation token.
    pub cancellation: CancellationToken,
}

impl PartialEq for PrefetchTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.slide_index == other.slide_index
    }
}

impl Eq for PrefetchTask {}

impl PartialOrd for PrefetchTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrefetchTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Max-heap ordering: higher priority comes first
        self.priority.cmp(&other.priority)
    }
}

#[derive(Clone)]
pub struct WorkerSharedContext {
    pub source: Arc<dyn PageSource>,
    pub asset_store: Option<Arc<dyn AssetStore>>,
    pub rasterizer: Arc<dyn SlideRasterizer>,
    pub cache: PipelineCache,
    pub in_flight: Arc<Mutex<HashSet<usize>>>,
    pub failed: Arc<Mutex<HashMap<usize, RenderError>>>,
    /// Compile/expansion warnings, tagged with the slide index.
    pub warnings: Arc<Mutex<Vec<(usize, WarningEvent)>>>,
    /// Late-bindable transmission encoder (PNG + base64 for the Kitty
    /// driver); workers pre-encode staged rasters through this slot.
    pub transmit: TransmitSlot,
    /// Pre-encoded payload cache shared with the pipeline.
    pub payloads: PayloadCache,
    /// Signals one event per worker-completed raster to the UI loop.
    pub done_tx: Option<Sender<()>>,
}

/// Background worker thread pool with priority queue and cooperative cancellation.
pub struct WorkerPool {
    queue: Arc<Mutex<BinaryHeap<PrefetchTask>>>,
    notify_tx: Sender<()>,
    in_flight: Arc<Mutex<HashSet<usize>>>,
    failed: Arc<Mutex<HashMap<usize, RenderError>>>,
    active_tokens: Arc<Mutex<HashMap<usize, CancellationToken>>>,
    shutdown: Arc<AtomicBool>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    /// Late-bindable transmission encoder shared with every worker context.
    transmit: TransmitSlot,
    /// Yields one event per worker-completed raster for the UI loop.
    done_rx: Receiver<()>,
}

impl WorkerPool {
    /// Spawns a background worker pool with `num_workers` threads.
    pub fn new(
        num_workers: usize,
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
        rasterizer: Arc<dyn SlideRasterizer>,
        cache: PipelineCache,
    ) -> Self {
        Self::with_warnings(
            num_workers,
            presentation,
            asset_store,
            compiler,
            rasterizer,
            cache,
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    /// Like [`WorkerPool::new`] but shares a warning buffer with the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn with_warnings(
        num_workers: usize,
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
        rasterizer: Arc<dyn SlideRasterizer>,
        cache: PipelineCache,
        warnings: Arc<Mutex<Vec<(usize, WarningEvent)>>>,
    ) -> Self {
        let source = Arc::new(PptxPageSource::new(
            presentation,
            Arc::clone(&asset_store),
            compiler,
        ));
        Self::with_source_and_warnings(
            num_workers,
            source,
            Some(asset_store),
            rasterizer,
            cache,
            PayloadCache::new(),
            None,
            warnings,
        )
    }

    /// Spawns a background worker pool from a generalized [`PageSource`].
    ///
    /// `payloads` is the pre-encoded transmission cache shared with the
    /// pipeline; `transmit` optionally pre-binds an encoder (the pipeline
    /// builder can also attach one later via
    /// [`WorkerPool::set_transmit_encoder`]).
    #[allow(clippy::too_many_arguments)]
    pub fn with_source_and_warnings(
        num_workers: usize,
        source: Arc<dyn PageSource>,
        asset_store: Option<Arc<dyn AssetStore>>,
        rasterizer: Arc<dyn SlideRasterizer>,
        cache: PipelineCache,
        payloads: PayloadCache,
        transmit: Option<Arc<dyn TransmitEncoder>>,
        warnings: Arc<Mutex<Vec<(usize, WarningEvent)>>>,
    ) -> Self {
        let queue = Arc::new(Mutex::new(BinaryHeap::new()));
        let (notify_tx, notify_rx) = crossbeam_channel::unbounded();
        let (done_tx, done_rx) = crossbeam_channel::unbounded::<()>();
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let failed = Arc::new(Mutex::new(HashMap::new()));
        let active_tokens = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let transmit = Arc::new(Mutex::new(transmit));

        let ctx = WorkerSharedContext {
            source,
            asset_store,
            rasterizer,
            cache,
            in_flight: Arc::clone(&in_flight),
            failed: Arc::clone(&failed),
            warnings: Arc::clone(&warnings),
            transmit: Arc::clone(&transmit),
            payloads,
            done_tx: Some(done_tx),
        };

        let mut threads = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            let thread_rx = notify_rx.clone();
            let thread_queue = Arc::clone(&queue);
            let thread_shutdown = Arc::clone(&shutdown);
            let thread_ctx = ctx.clone();

            let handle = thread::Builder::new()
                .name("kitty-pptx-prefetch".to_string())
                .spawn(move || {
                    worker_loop(thread_rx, thread_queue, thread_shutdown, thread_ctx);
                })
                .expect("failed to spawn prefetch worker thread");

            threads.push(handle);
        }

        Self {
            queue,
            notify_tx,
            in_flight,
            failed,
            active_tokens,
            shutdown,
            threads: Mutex::new(threads),
            transmit,
            done_rx,
        }
    }

    /// Schedule a slide prefetch task with given priority, target spec and
    /// staging flag. Returns the associated cooperative cancellation token.
    pub fn schedule(
        &self,
        slide_index: usize,
        priority: u32,
        target: RenderTargetSpec,
        stage: bool,
    ) -> CancellationToken {
        let token = CancellationToken::new();

        if let Ok(mut tokens) = self.active_tokens.lock() {
            tokens.insert(slide_index, token.clone());
        }

        if let Ok(mut inf) = self.in_flight.lock() {
            inf.insert(slide_index);
        }

        let task = PrefetchTask {
            priority,
            slide_index,
            target,
            stage,
            cancellation: token.clone(),
        };

        if let Ok(mut q) = self.queue.lock() {
            q.push(task);
        }

        self.notify_tx.send(()).ok();
        token
    }

    /// Cancel all in-flight or pending tasks whose slide index is not in `keep`.
    pub fn cancel_except(&self, keep: &HashSet<usize>) {
        if let Ok(mut tokens) = self.active_tokens.lock() {
            tokens.retain(|idx, token| {
                if !keep.contains(idx) {
                    token.cancel();
                    false
                } else {
                    true
                }
            });
        }

        // Drop canceled tasks that have not started yet.
        let mut canceled = Vec::new();
        if let Ok(mut q) = self.queue.lock() {
            let mut retained = Vec::with_capacity(q.len());
            for task in q.drain() {
                if task.cancellation.is_canceled() {
                    canceled.push(task.slide_index);
                } else {
                    retained.push(task);
                }
            }
            for task in retained {
                q.push(task);
            }
        }

        if canceled.is_empty() {
            return;
        }

        // A canceled queued task will never reach `execute_task`, so nothing
        // else clears its `in_flight` slot. Leaving it behind makes
        // `is_in_flight` true forever, which stops `prefetch` from ever
        // re-scheduling the slide: the canvas and any sorter card for it stay
        // blank until `cancel_all`. Release the slots for indices that no
        // longer have a live token (a running task keeps its token and clears
        // its own slot when it finishes).
        let still_active: HashSet<usize> = self
            .active_tokens
            .lock()
            .map(|tokens| tokens.keys().copied().collect())
            .unwrap_or_default();
        if let Ok(mut in_flight) = self.in_flight.lock() {
            for index in canceled {
                if !still_active.contains(&index) {
                    in_flight.remove(&index);
                }
            }
        }
    }

    /// Cancel all currently pending and active background tasks.
    pub fn cancel_all(&self) {
        if let Ok(mut tokens) = self.active_tokens.lock() {
            for token in tokens.values() {
                token.cancel();
            }
            tokens.clear();
        }

        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }

        if let Ok(mut inf) = self.in_flight.lock() {
            inf.clear();
        }
    }

    /// Check if the given slide index is currently in flight (queued or rendering).
    pub fn is_in_flight(&self, slide_index: usize) -> bool {
        self.in_flight
            .lock()
            .map(|inf| inf.contains(&slide_index))
            .unwrap_or(false)
    }

    /// Get failure error for the given slide index, if any.
    pub fn get_failed(&self, slide_index: usize) -> Option<RenderError> {
        self.failed.lock().ok()?.get(&slide_index).cloned()
    }

    /// Latch a per-slide render failure.
    ///
    /// The unified failure semantics of the sync renderer: a latched error is
    /// replayed by `entry`/`entry_at` instead of re-paying the failing render.
    pub fn latch_failure(&self, slide_index: usize, error: RenderError) {
        if let Ok(mut fl) = self.failed.lock() {
            fl.insert(slide_index, error);
        }
    }

    /// Drop the per-slide failure latch after a successful render.
    pub fn clear_failure(&self, slide_index: usize) {
        if let Ok(mut fl) = self.failed.lock() {
            fl.remove(&slide_index);
        }
    }

    /// Clear all recorded rendering failures.
    pub fn clear_failed(&self) {
        if let Ok(mut fl) = self.failed.lock() {
            fl.clear();
        }
    }

    /// Number of slides currently in flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.lock().map(|inf| inf.len()).unwrap_or(0)
    }

    /// Number of tasks waiting in the priority queue.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// Receiver that yields one event per worker-completed raster, letting
    /// the UI loop know fresh results exist without polling the caches.
    pub fn completion_receiver(&self) -> Receiver<()> {
        self.done_rx.clone()
    }

    /// Attaches the off-thread transmission encoder workers use to pre-encode
    /// staged rasters. Called by the pipeline builder after pool creation;
    /// the slot is shared with every already-spawned worker thread.
    pub fn set_transmit_encoder(&self, encoder: Arc<dyn TransmitEncoder>) {
        if let Ok(mut slot) = self.transmit.lock() {
            *slot = Some(encoder);
        }
    }

    /// Shut down all worker threads cleanly.
    pub fn shutdown(&self) {
        self.shutdown.store(true, AtomicOrdering::Release);
        self.cancel_all();
        if let Ok(threads) = self.threads.lock() {
            for _ in 0..threads.len() {
                self.notify_tx.send(()).ok();
            }
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.shutdown();
        if let Ok(mut threads) = self.threads.lock() {
            for handle in threads.drain(..) {
                handle.join().ok();
            }
        }
    }
}

fn worker_loop(
    notify_rx: Receiver<()>,
    queue: Arc<Mutex<BinaryHeap<PrefetchTask>>>,
    shutdown: Arc<AtomicBool>,
    ctx: WorkerSharedContext,
) {
    while !shutdown.load(AtomicOrdering::Acquire) {
        match notify_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(()) => {
                let task = {
                    let mut q = match queue.lock() {
                        Ok(l) => l,
                        Err(_) => break,
                    };
                    q.pop()
                };

                let task = match task {
                    Some(t) => t,
                    None => continue,
                };

                // Cooperative cancellation check before starting
                if task.cancellation.is_canceled() || shutdown.load(AtomicOrdering::Acquire) {
                    if let Ok(mut inf) = ctx.in_flight.lock() {
                        inf.remove(&task.slide_index);
                    }
                    continue;
                }

                execute_task(&task, &ctx, &shutdown);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn execute_task(task: &PrefetchTask, ctx: &WorkerSharedContext, shutdown: &AtomicBool) {
    let index = task.slide_index;
    let target = task.target;

    // Fast check: already cached?
    if ctx.cache.contains_raster(index, &target) {
        if let Ok(mut inf) = ctx.in_flight.lock() {
            inf.remove(&index);
        }
        return;
    }

    if index >= ctx.source.len() {
        if let Ok(mut inf) = ctx.in_flight.lock() {
            inf.remove(&index);
        }
        return;
    }

    // Panic barrier: a compile/rasterize panic must not wedge the slide in
    // `in_flight` forever (the syncer would treat it as perpetually
    // `Rendering` and the canvas would stay blank with no diagnostics and no
    // retry). Record the panic as a render failure and keep the thread alive.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_and_rasterize(task, ctx, shutdown);
    }));
    if let Err(payload) = outcome {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        if !shutdown.load(AtomicOrdering::Acquire) {
            if let Ok(mut fl) = ctx.failed.lock() {
                fl.insert(index, RenderError::RasterizationError(detail));
            }
        }
    }

    if let Ok(mut inf) = ctx.in_flight.lock() {
        inf.remove(&index);
    }
}

/// The compile + rasterize half of a task, split out so [`execute_task`] can
/// guard it with a panic barrier. The render work itself lives in the shared
/// path ([`render::render_page`]); this half only adds the worker
/// bookkeeping: failure latching, payload pre-encoding, and the completion
/// event. On completion the entry is in the raster cache, recorded in
/// `failed`, or dropped (cancellation).
fn compile_and_rasterize(task: &PrefetchTask, ctx: &WorkerSharedContext, shutdown: &AtomicBool) {
    let index = task.slide_index;
    let target = task.target;

    match render::render_page(
        ctx.source.as_ref(),
        ctx.rasterizer.as_ref(),
        ctx.asset_store.as_ref(),
        &ctx.cache,
        &ctx.warnings,
        index,
        &target,
        Some(&task.cancellation),
    ) {
        Ok(bitmap) => {
            // The raster is published inside the shared render path; the
            // completion side effects stay guarded exactly as before: a
            // canceled or shutting-down task never clears the failure latch,
            // stages a payload, or signals the UI loop.
            if !task.cancellation.is_canceled() && !shutdown.load(AtomicOrdering::Acquire) {
                if let Ok(mut fl) = ctx.failed.lock() {
                    fl.remove(&index);
                }

                // Staging policy (PRD 03a): pre-encode the terminal payload
                // on the worker so the UI thread writes prebuilt bytes.
                // Encode failures are skipped: the syncer falls back to
                // on-thread encoding through the driver.
                if task.stage {
                    let encoder = ctx
                        .transmit
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone());
                    if let Some(encoder) = encoder {
                        ctx.payloads.stage(encoder.as_ref(), index, target, &bitmap);
                    }
                }

                // One event per freshly published raster wakes the UI loop.
                if let Some(done) = ctx.done_tx.as_ref() {
                    done.send(()).ok();
                }
            }
        }
        Err(error) => {
            // Cancellation is a supersede, not a failure: only real render
            // errors latch for later `entry` reads to replay.
            if !task.cancellation.is_canceled() {
                if let Ok(mut fl) = ctx.failed.lock() {
                    fl.insert(index, error);
                }
            }
        }
    }
}
