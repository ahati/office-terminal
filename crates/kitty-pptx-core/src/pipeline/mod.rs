pub mod cache;
pub mod cancellation;
pub mod payload;
pub mod worker;

mod render;

pub use cache::PipelineCache;
pub use cancellation::CancellationToken;
pub use payload::PayloadCache;
pub use worker::{PrefetchTask, WorkerPool, WorkerSharedContext};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::compiler::SlideCompiler;
use crate::contracts::{
    InvalidateCause, PageSource, PipelineEntry, PptxPageSource, PrefetchPlan, RenderPipeline,
    RenderTargetSpec, RgbaSlide, TransmitEncoder, TransmitPayload, WarningEvent,
};
use crate::error::RenderError;
use crate::loader::AssetStore;
use crate::rasterizer::SlideRasterizer;

/// Default implementation of the `RenderPipeline` trait providing background
/// priority prefetching, cooperative cancellation, two-tier caching, and synchronous fallback.
pub struct DefaultRenderPipeline {
    source: Arc<dyn PageSource>,
    rasterizer: Arc<dyn SlideRasterizer>,
    asset_store: Option<Arc<dyn AssetStore>>,
    generation: AtomicU64,
    cache: PipelineCache,
    /// Pre-encoded transmission payloads, shared with the worker pool.
    payloads: PayloadCache,
    workers: Arc<WorkerPool>,
    last_target: Mutex<RenderTargetSpec>,
    sync_fallback: AtomicBool,
    /// Compile/expansion warnings tagged with their slide index.
    warnings: Arc<Mutex<Vec<(usize, WarningEvent)>>>,
}

impl DefaultRenderPipeline {
    /// Create a new render pipeline wrapping a [`PageSource`].
    pub fn from_page_source(
        source: Arc<dyn PageSource>,
        rasterizer: Arc<dyn SlideRasterizer>,
    ) -> Self {
        Self::from_page_source_with_workers(source, rasterizer, 2)
    }

    /// Create a new render pipeline wrapping a [`PageSource`] with a custom worker count.
    pub fn from_page_source_with_workers(
        source: Arc<dyn PageSource>,
        rasterizer: Arc<dyn SlideRasterizer>,
        worker_threads: usize,
    ) -> Self {
        let cache = PipelineCache::new();
        let payloads = PayloadCache::new();
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let asset_store = source.asset_store();
        let workers = Arc::new(WorkerPool::with_source_and_warnings(
            worker_threads,
            Arc::clone(&source),
            asset_store.clone(),
            Arc::clone(&rasterizer),
            cache.clone(),
            payloads.clone(),
            None,
            Arc::clone(&warnings),
        ));

        Self {
            source,
            rasterizer,
            asset_store,
            generation: AtomicU64::new(0),
            cache,
            payloads,
            workers,
            last_target: Mutex::new(RenderTargetSpec {
                width_px: 1920,
                height_px: 1080,
            }),
            sync_fallback: AtomicBool::new(false),
            warnings,
        }
    }

    /// Create a new render pipeline with default 2 background worker threads.
    pub fn new(
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
        rasterizer: Arc<dyn SlideRasterizer>,
    ) -> Self {
        Self::with_worker_count(presentation, asset_store, compiler, rasterizer, 2)
    }

    /// Create a new render pipeline with a custom number of background worker threads.
    pub fn with_worker_count(
        presentation: Arc<pptx_model::Presentation>,
        asset_store: Arc<dyn AssetStore>,
        compiler: Arc<dyn SlideCompiler>,
        rasterizer: Arc<dyn SlideRasterizer>,
        worker_threads: usize,
    ) -> Self {
        let source = Arc::new(PptxPageSource::new(
            presentation,
            Arc::clone(&asset_store),
            compiler,
        ));
        Self::from_page_source_with_workers(source, rasterizer, worker_threads)
    }

    /// Builder option to enable/disable synchronous fallback on `entry()` misses.
    pub fn with_sync_fallback(self, enabled: bool) -> Self {
        self.sync_fallback.store(enabled, Ordering::Release);
        self
    }

    /// Builder option: attach an off-thread transmission encoder (e.g. Kitty
    /// PNG + base64).
    ///
    /// Prefetch workers then pre-encode each raster's terminal payload right
    /// after rasterizing (for tasks whose plan requests staging), so the
    /// interactive syncer writes prebuilt bytes instead of paying encode
    /// cost on the UI thread.
    pub fn with_transmit_encoder(self, encoder: Arc<dyn TransmitEncoder>) -> Self {
        self.workers.set_transmit_encoder(encoder);
        self
    }

    /// Configure whether `entry()` should synchronously rasterize if a frame is not yet ready.
    pub fn set_sync_fallback(&self, enabled: bool) {
        self.sync_fallback.store(enabled, Ordering::Release);
    }

    /// Returns the current pipeline generation counter.
    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Access the two-tier cache.
    pub fn cache(&self) -> &PipelineCache {
        &self.cache
    }

    /// Access the underlying background worker pool.
    pub fn workers(&self) -> &Arc<WorkerPool> {
        &self.workers
    }

    /// Synchronously compiles and rasterizes a slide for the given target spec,
    /// populating the two-tier cache, bypassing the background worker queue.
    pub fn render_sync(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, RenderError> {
        if let Ok(mut t) = self.last_target.lock() {
            *t = *target;
        }
        self.rasterize_now(index, target)
    }

    /// Compiles and rasterizes `index` for `target` without changing the
    /// pipeline's notion of the main canvas target.
    ///
    /// Thin caller over the shared render path ([`render::render_page`]) with
    /// the workers' failure semantics: a fresh success clears any stale
    /// per-slide failure latch, an error latches it, so sync and background
    /// renders behave identically.
    fn rasterize_now(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, RenderError> {
        match render::render_page(
            self.source.as_ref(),
            self.rasterizer.as_ref(),
            self.asset_store.as_ref(),
            &self.cache,
            &self.warnings,
            index,
            target,
            None,
        ) {
            Ok(bitmap) => {
                self.workers.clear_failure(index);
                Ok(bitmap)
            }
            Err(error) => {
                self.workers.latch_failure(index, error.clone());
                Err(error)
            }
        }
    }

    /// Get a slide from the cache if present, otherwise render it synchronously.
    pub fn get_or_render(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, RenderError> {
        self.render_sync(index, target)
    }

    /// Drains warnings produced since the last call (slide index + event).
    pub fn drain_warnings(&self) -> Vec<(usize, WarningEvent)> {
        self.warnings
            .lock()
            .map(|mut buffer| std::mem::take(&mut *buffer))
            .unwrap_or_default()
    }

    /// Shared tail of `entry`/`entry_at` once the requested raster cache
    /// missed.
    ///
    /// `exact_target` marks explicit-target lookups (the sorter grid, reader
    /// pages): they must not kick off synchronous rendering on the calling
    /// thread — the background workers own rasterization for those views.
    fn entry_after_cache_miss(
        &self,
        index: usize,
        target: &RenderTargetSpec,
        exact_target: bool,
    ) -> PipelineEntry {
        // 1. Check for a latched failure (background worker or last attempt)
        if let Some(error) = self.workers.get_failed(index) {
            return PipelineEntry::Failed { error };
        }

        // 2. Check if currently in-flight
        if self.workers.is_in_flight(index) {
            return PipelineEntry::Rendering;
        }

        // 3. Synchronous fallback if enabled and not already running in background
        if !exact_target && self.sync_fallback.load(Ordering::Acquire) {
            return match self.rasterize_now(index, target) {
                Ok(bitmap) => PipelineEntry::Ready { bitmap },
                Err(error) => PipelineEntry::Failed { error },
            };
        }

        // 4. Not yet loaded
        PipelineEntry::NotLoaded
    }
}

impl RenderPipeline for DefaultRenderPipeline {
    fn render(&self, index: usize, target: &RenderTargetSpec) -> Result<RgbaSlide, RenderError> {
        self.render_sync(index, target)
    }

    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec) {
        if let Ok(mut t) = self.last_target.lock() {
            *t = *target;
        }

        let total_slides = self.source.len();
        if total_slides == 0 {
            return;
        }

        let center = plan.center;
        let mut ordered_targets: Vec<(usize, u32)> = Vec::new();
        let mut cur_prio = 10_000u32;

        // 1. Current active slide gets maximum priority
        if center < total_slides {
            ordered_targets.push((center, cur_prio));
            cur_prio = cur_prio.saturating_sub(10);
        }

        // 2. Schedule ahead & behind with priority order: N+1, N-1, N+2, N-2, ...
        let max_dist = std::cmp::max(plan.ahead, plan.behind);
        for dist in 1..=max_dist {
            if dist <= plan.ahead && center + dist < total_slides {
                ordered_targets.push((center + dist, cur_prio));
                cur_prio = cur_prio.saturating_sub(10);
            }
            if dist <= plan.behind && center >= dist {
                ordered_targets.push((center - dist, cur_prio));
                cur_prio = cur_prio.saturating_sub(10);
            }
        }

        // Cooperative cancellation: cancel pending tasks outside the new window
        let active_set: HashSet<usize> = ordered_targets.iter().map(|(idx, _)| *idx).collect();
        self.workers.cancel_except(&active_set);

        // Enqueue remaining tasks in order of priority
        for (idx, prio) in ordered_targets {
            if self.cache.contains_raster(idx, target) {
                continue;
            }
            if self.workers.is_in_flight(idx) {
                continue;
            }
            self.workers.schedule(idx, prio, *target, plan.stage);
        }
    }

    fn entry(&self, index: usize) -> PipelineEntry {
        let target = self
            .last_target
            .lock()
            .map(|t| *t)
            .unwrap_or(RenderTargetSpec {
                width_px: 1920,
                height_px: 1080,
            });

        // 1. Check Tier 2 raster cache for target
        if let Some(bmp) = self.cache.get_raster(index, &target) {
            return PipelineEntry::Ready { bitmap: bmp };
        }

        // Fall back to the widest cached raster for this slide (e.g. the frame
        // from before a resize). O(1) via the cache's per-slide index instead of
        // scanning every cached raster.
        if let Some(bitmap) = self.cache.best_raster(index) {
            return PipelineEntry::Ready { bitmap };
        }

        self.entry_after_cache_miss(index, &target, false)
    }

    fn entry_at(&self, index: usize, target: &RenderTargetSpec) -> PipelineEntry {
        // Exact target only: the caller (the sorter grid) stages this bitmap as
        // the canvas raster and lets the terminal scale it, so another
        // resolution would silently change what the card shows. No
        // widest-raster fallback, no synchronous fallback.
        if let Some(bmp) = self.cache.get_raster(index, target) {
            return PipelineEntry::Ready { bitmap: bmp };
        }

        self.entry_after_cache_miss(index, target, true)
    }

    fn drain_warnings(&self) -> Vec<(usize, WarningEvent)> {
        DefaultRenderPipeline::drain_warnings(self)
    }

    fn staged_payload(&self, index: usize, target: &RenderTargetSpec) -> Option<TransmitPayload> {
        self.payloads.get(index, target)
    }

    fn prefetch_window(&self, plan: &PrefetchPlan, targets: &[(usize, RenderTargetSpec)]) {
        // Per-page-target scheduling: unlike `prefetch`, there is no single
        // canvas target to latch, and `entry()` fallbacks are not used by
        // window callers (they resolve each page with `entry_at`).
        let total_slides = self.source.len();
        if total_slides == 0 || targets.is_empty() {
            return;
        }

        // Priority per plan window: center first, then alternating +1/-1, ...,
        // mirroring `prefetch`'s ordering.
        let mut priority_of: HashMap<usize, u32> = HashMap::new();
        let mut cur_prio = 10_000u32;
        if plan.center < total_slides {
            priority_of.insert(plan.center, cur_prio);
            cur_prio = cur_prio.saturating_sub(10);
        }
        let max_dist = std::cmp::max(plan.ahead, plan.behind);
        for dist in 1..=max_dist {
            if dist <= plan.ahead && plan.center + dist < total_slides {
                priority_of.entry(plan.center + dist).or_insert(cur_prio);
                cur_prio = cur_prio.saturating_sub(10);
            }
            if dist <= plan.behind && plan.center >= dist {
                priority_of.entry(plan.center - dist).or_insert(cur_prio);
                cur_prio = cur_prio.saturating_sub(10);
            }
        }

        // Cooperative cancellation: cancel pending tasks outside the window.
        let active_set: HashSet<usize> = targets.iter().map(|(idx, _)| *idx).collect();
        self.workers.cancel_except(&active_set);

        // Out-of-window targets still render, after the prioritized window.
        let tail_prio = cur_prio;
        for (idx, target) in targets {
            if *idx >= total_slides {
                continue;
            }
            let prio = priority_of.get(idx).copied().unwrap_or(tail_prio);
            if self.cache.contains_raster(*idx, target) {
                continue;
            }
            if self.workers.is_in_flight(*idx) {
                continue;
            }
            self.workers.schedule(*idx, prio, *target, plan.stage);
        }
    }

    fn in_flight(&self) -> usize {
        self.workers.in_flight_count()
    }

    fn completion_receiver(&self) -> Option<crossbeam_channel::Receiver<()>> {
        Some(self.workers.completion_receiver())
    }

    fn invalidate(&self, cause: InvalidateCause) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        match cause {
            InvalidateCause::Resize => {
                // Resolution unchanged, only cell grid adjusted; keep caches intact
            }
            InvalidateCause::TargetChanged => {
                // New pixel target resolution; cancel workers and clear raster
                // cache plus any pre-encoded payloads keyed to old targets.
                self.workers.cancel_all();
                self.cache.clear_raster();
                self.payloads.clear();
            }
            InvalidateCause::DeckReload => {
                // Entire presentation reloaded; cancel workers, clear all caches
                // (stale payloads would show the old deck) and drop the
                // per-slide failure latch so a stale compile error
                // cannot poison the freshly loaded deck.
                self.workers.cancel_all();
                self.workers.clear_failed();
                self.cache.clear_all();
                self.payloads.clear();
            }
        }
    }
}
