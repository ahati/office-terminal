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
    InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec, RgbaSlide,
    VectorSlide,
};
use crate::error::RenderError;
use crate::loader::AssetStore;
use crate::rasterizer::SlideRasterizer;

/// Default implementation of the `RenderPipeline` trait providing background
/// priority prefetching, cooperative cancellation, two-tier caching, and synchronous fallback.
pub struct DefaultRenderPipeline {
    compiler: Arc<dyn SlideCompiler>,
    rasterizer: Arc<dyn SlideRasterizer>,
    presentation: Arc<pptx_model::Presentation>,
    asset_store: Arc<dyn AssetStore>,
    generation: AtomicU64,
    cache: PipelineCache,
    workers: Arc<WorkerPool>,
    last_target: Mutex<RenderTargetSpec>,
    sync_fallback: AtomicBool,
}

impl DefaultRenderPipeline {
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
        let cache = PipelineCache::new();
        let workers = Arc::new(WorkerPool::new(
            worker_threads,
            Arc::clone(&presentation),
            Arc::clone(&asset_store),
            Arc::clone(&compiler),
            Arc::clone(&rasterizer),
            cache.clone(),
        ));

        Self {
            compiler,
            rasterizer,
            presentation,
            asset_store,
            generation: AtomicU64::new(0),
            cache,
            workers,
            last_target: Mutex::new(RenderTargetSpec {
                width_px: 1920,
                height_px: 1080,
            }),
            sync_fallback: AtomicBool::new(false),
        }
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

        if index >= self.presentation.slides.len() {
            return Err(RenderError::CompilationFailed(format!(
                "slide index {} out of bounds (total slides: {})",
                index,
                self.presentation.slides.len()
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
            let (tx, _rx) = crossbeam_channel::unbounded();
            let slide = &self.presentation.slides[index];
            let v = self.compiler.compile_in_deck(
                slide,
                &self.presentation.slides,
                self.presentation.slide_width,
                self.presentation.slide_height,
                &tx,
            )?;
            self.cache.put_vector(index, v.clone());
            v
        };

        // Tier 2: Target rasterization
        let bmp = self
            .rasterizer
            .rasterize(&vector_slide, target, self.asset_store.as_ref())?;
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
}

impl RenderPipeline for DefaultRenderPipeline {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec) {
        if let Ok(mut t) = self.last_target.lock() {
            *t = *target;
        }

        let total_slides = self.presentation.slides.len();
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

        // Also check if any resolution is cached for this slide index
        if let Ok(rcache) = self.cache.raw_raster_cache().lock() {
            if let Some((_, bmp)) = rcache.iter().find(|((idx, _), _)| *idx == index) {
                return PipelineEntry::Ready {
                    bitmap: bmp.clone(),
                    staged: false,
                };
            }
        }

        // 2. Synchronous fallback if enabled
        if self.sync_fallback.load(Ordering::Acquire) {
            match self.render_sync(index, &target) {
                Ok(bmp) => {
                    return PipelineEntry::Ready {
                        bitmap: bmp,
                        staged: false,
                    }
                }
                Err(error) => return PipelineEntry::Failed { error },
            }
        }

        // 3. Check for previous background failure
        if let Some(error) = self.workers.get_failed(index) {
            return PipelineEntry::Failed { error };
        }

        // 4. Check if currently in-flight
        if self.workers.is_in_flight(index) {
            return PipelineEntry::Rendering;
        }

        // 5. Not yet loaded
        PipelineEntry::NotLoaded
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
