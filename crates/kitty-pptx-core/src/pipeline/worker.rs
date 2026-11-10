use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crossbeam_channel::{Receiver, Sender};

use crate::compiler::SlideCompiler;
use crate::contracts::{RenderTargetSpec, WarningEvent};
use crate::error::RenderError;
use crate::loader::AssetStore;
use crate::rasterizer::SlideRasterizer;
use super::cache::PipelineCache;
use super::cancellation::CancellationToken;

/// Work item representing a pending slide pre-rasterization task.
#[derive(Clone)]
pub struct PrefetchTask {
    /// Priority level (higher values pop first from the priority queue).
    pub priority: u32,
    /// Target slide index (0-based).
    pub slide_index: usize,
    /// Destination render resolution.
    pub target: RenderTargetSpec,
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
    pub presentation: Arc<pptx_model::Presentation>,
    pub asset_store: Arc<dyn AssetStore>,
    pub compiler: Arc<dyn SlideCompiler>,
    pub rasterizer: Arc<dyn SlideRasterizer>,
    pub cache: PipelineCache,
    pub in_flight: Arc<Mutex<HashSet<usize>>>,
    pub failed: Arc<Mutex<HashMap<usize, RenderError>>>,
    pub warning_sink: Sender<WarningEvent>,
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
        let queue = Arc::new(Mutex::new(BinaryHeap::new()));
        let (notify_tx, notify_rx) = crossbeam_channel::unbounded();
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let failed = Arc::new(Mutex::new(HashMap::new()));
        let active_tokens = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let (warning_sink, _warning_rx) = crossbeam_channel::unbounded();

        let ctx = WorkerSharedContext {
            presentation,
            asset_store,
            compiler,
            rasterizer,
            cache,
            in_flight: Arc::clone(&in_flight),
            failed: Arc::clone(&failed),
            warning_sink,
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
        }
    }

    /// Schedule a slide prefetch task with given priority and target spec.
    /// Returns the associated cooperative cancellation token.
    pub fn schedule(
        &self,
        slide_index: usize,
        priority: u32,
        target: RenderTargetSpec,
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

        // Clean out canceled tasks waiting in queue
        if let Ok(mut q) = self.queue.lock() {
            let retained: Vec<PrefetchTask> = q
                .drain()
                .filter(|t| !t.cancellation.is_canceled())
                .collect();
            for task in retained {
                q.push(task);
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

    if index >= ctx.presentation.slides.len() {
        if let Ok(mut inf) = ctx.in_flight.lock() {
            inf.remove(&index);
        }
        return;
    }

    // Step 1: Obtain or compile VectorSlide (Tier 1)
    let vector_slide = match ctx.cache.get_vector(index) {
        Some(v) => v,
        None => {
            if task.cancellation.is_canceled() || shutdown.load(AtomicOrdering::Acquire) {
                if let Ok(mut inf) = ctx.in_flight.lock() {
                    inf.remove(&index);
                }
                return;
            }

            let slide = &ctx.presentation.slides[index];
            match ctx.compiler.compile(
                slide,
                ctx.presentation.slide_width,
                ctx.presentation.slide_height,
                &ctx.warning_sink,
            ) {
                Ok(v) => {
                    ctx.cache.put_vector(index, v.clone());
                    v
                }
                Err(err) => {
                    if let Ok(mut fl) = ctx.failed.lock() {
                        fl.insert(index, err);
                    }
                    if let Ok(mut inf) = ctx.in_flight.lock() {
                        inf.remove(&index);
                    }
                    return;
                }
            }
        }
    };

    // Cooperative cancellation check before expensive rasterization
    if task.cancellation.is_canceled() || shutdown.load(AtomicOrdering::Acquire) {
        if let Ok(mut inf) = ctx.in_flight.lock() {
            inf.remove(&index);
        }
        return;
    }

    // Step 2: Rasterize to RgbaSlide (Tier 2)
    match ctx.rasterizer.rasterize(&vector_slide, &target, ctx.asset_store.as_ref()) {
        Ok(bmp) => {
            if !task.cancellation.is_canceled() && !shutdown.load(AtomicOrdering::Acquire) {
                ctx.cache.put_raster(index, target, bmp);
                if let Ok(mut fl) = ctx.failed.lock() {
                    fl.remove(&index);
                }
            }
        }
        Err(err) => {
            if !task.cancellation.is_canceled() {
                if let Ok(mut fl) = ctx.failed.lock() {
                    fl.insert(index, err);
                }
            }
        }
    }

    if let Ok(mut inf) = ctx.in_flight.lock() {
        inf.remove(&index);
    }
}
