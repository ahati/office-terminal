pub mod cache;
pub mod cancellation;
pub mod worker;

pub use cache::PipelineCache;
pub use cancellation::CancellationToken;
pub use worker::{PrefetchTask, WorkerPool, WorkerSharedContext};

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::compiler::SlideCompiler;
use crate::contracts::{
    InvalidateCause, PageSource, PipelineEntry, PptxPageSource, PrefetchPlan, RenderPipeline,
    RenderTargetSpec, RgbaSlide, VectorSlide, WarningEvent,
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
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let asset_store = source.asset_store();
        let workers = Arc::new(WorkerPool::with_source_and_warnings(
            worker_threads,
            Arc::clone(&source),
            asset_store.clone(),
            Arc::clone(&rasterizer),
            cache.clone(),
            Arc::clone(&warnings),
        ));

        Self {
            source,
            rasterizer,
            asset_store,
            generation: AtomicU64::new(0),
            cache,
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
    fn rasterize_now(
        &self,
        index: usize,
        target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, RenderError> {
        if index >= self.source.len() {
            return Err(RenderError::CompilationFailed(format!(
                "slide index {} out of bounds (total slides: {})",
                index,
                self.source.len()
            )));
        }

        // Fast path: cached raster
        if let Some(bmp) = self.cache.get_raster(index, target) {
            return Ok(bmp);
        }

        // Tier 1: Vector slide
        let vector_slide: VectorSlide = if let Some(v) = self.cache.get_vector(index) {
            v
        } else {
            let (tx, rx) = crossbeam_channel::unbounded();
            let v = match self.source.compile_page(index, &tx) {
                Ok(scene) => scene,
                Err(error) => {
                    self.flush_warnings(index, &rx);
                    return Err(error);
                }
            };
            self.flush_warnings(index, &rx);
            self.cache.put_vector(index, v.clone());
            v
        };

        // Tier 2: Target rasterization
        let empty_store = crate::loader::asset_store::EmptyAssetStore;
        let store: &dyn AssetStore = match &self.asset_store {
            Some(s) => s.as_ref(),
            None => &empty_store,
        };
        let bmp = self.rasterizer.rasterize(&vector_slide, target, store)?;
        self.cache.put_raster(index, *target, bmp.clone());
        Ok(bmp)
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

    /// Moves per-task warnings into the shared buffer.
    fn flush_warnings(&self, index: usize, receiver: &crossbeam_channel::Receiver<WarningEvent>) {
        let events: Vec<WarningEvent> = receiver.try_iter().collect();
        if events.is_empty() {
            return;
        }
        if let Ok(mut buffer) = self.warnings.lock() {
            buffer.extend(events.into_iter().map(|event| (index, event)));
        }
    }

    /// Shared tail of `entry`/`entry_at` once the target raster cache missed.
    fn entry_after_cache_miss(&self, index: usize, target: &RenderTargetSpec) -> PipelineEntry {
        // 1. Synchronous fallback if enabled
        if self.sync_fallback.load(Ordering::Acquire) {
            return match self.rasterize_now(index, target) {
                Ok(bitmap) => PipelineEntry::Ready {
                    bitmap,
                    staged: false,
                },
                Err(error) => PipelineEntry::Failed { error },
            };
        }

        // 2. Check for a latched background failure
        if let Some(error) = self.workers.get_failed(index) {
            return PipelineEntry::Failed { error };
        }

        // 3. Check if currently in-flight
        if self.workers.is_in_flight(index) {
            return PipelineEntry::Rendering;
        }

        // 4. Not yet loaded
        PipelineEntry::NotLoaded
    }
}

impl RenderPipeline for DefaultRenderPipeline {
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
            self.workers.schedule(idx, prio, *target);
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
            return PipelineEntry::Ready {
                bitmap: bmp,
                staged: false,
            };
        }

        // Fall back to the widest cached raster for this slide (e.g. the frame
        // from before a resize). O(1) via the cache's per-slide index instead of
        // scanning every cached raster.
        if let Some(bitmap) = self.cache.best_raster(index) {
            return PipelineEntry::Ready {
                bitmap,
                staged: false,
            };
        }

        self.entry_after_cache_miss(index, &target)
    }

    fn entry_at(&self, index: usize, target: &RenderTargetSpec) -> PipelineEntry {
        // Exact target only: the caller (the sorter grid) stages this bitmap as
        // the canvas raster and lets the terminal scale it, so another
        // resolution would silently change what the card shows.
        if let Some(bmp) = self.cache.get_raster(index, target) {
            return PipelineEntry::Ready {
                bitmap: bmp,
                staged: false,
            };
        }

        self.entry_after_cache_miss(index, target)
    }

    fn drain_warnings(&self) -> Vec<(usize, WarningEvent)> {
        DefaultRenderPipeline::drain_warnings(self)
    }

    fn invalidate(&self, cause: InvalidateCause) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        match cause {
            InvalidateCause::Resize => {
                // Resolution unchanged, only cell grid adjusted; keep caches intact
            }
            InvalidateCause::TargetChanged => {
                // New pixel target resolution; cancel workers and clear raster cache
                self.workers.cancel_all();
                self.cache.clear_raster();
            }
            InvalidateCause::DeckReload => {
                // Entire presentation reloaded; cancel workers, clear all caches
                // and drop the per-slide failure latch so a stale compile error
                // cannot poison the freshly loaded deck.
                self.workers.cancel_all();
                self.workers.clear_failed();
                self.cache.clear_all();
            }
        }
    }
}
