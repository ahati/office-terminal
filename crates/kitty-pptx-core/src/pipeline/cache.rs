use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::contracts::{RenderTargetSpec, RgbaSlide, VectorSlide};

/// Default raster retention budget (PRD 02a §3: 64 MB).
const DEFAULT_RASTER_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Bookkeeping that keeps the Tier 2 cache bounded and its resolution
/// fallback O(1).
#[derive(Default)]
struct RasterMeta {
    /// Total pixel bytes currently retained.
    bytes: usize,
    /// Insertion order, used as the eviction queue (stale keys are skipped).
    order: VecDeque<(usize, RenderTargetSpec)>,
    /// Widest cached target per slide index, for cheap fallback lookups.
    best: HashMap<usize, RenderTargetSpec>,
}

/// Two-tier pipeline cache storing resolution-independent VectorSlides
/// (Tier 1) and rasterized RgbaSlides (Tier 2).
///
/// Rasters are retained under a byte budget and evicted oldest-first, so a
/// long session that keeps resizes and zooms cannot grow without bound.
#[derive(Clone)]
pub struct PipelineCache {
    vector_cache: Arc<Mutex<HashMap<usize, VectorSlide>>>,
    raster_cache: Arc<Mutex<HashMap<(usize, RenderTargetSpec), RgbaSlide>>>,
    meta: Arc<Mutex<RasterMeta>>,
    budget_bytes: usize,
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineCache {
    /// Create a new empty pipeline cache with the default raster budget.
    pub fn new() -> Self {
        Self::with_budget_bytes(DEFAULT_RASTER_BUDGET_BYTES)
    }

    /// Create an empty cache retaining at most `budget_bytes` of rasters.
    pub fn with_budget_bytes(budget_bytes: usize) -> Self {
        Self {
            vector_cache: Arc::new(Mutex::new(HashMap::new())),
            raster_cache: Arc::new(Mutex::new(HashMap::new())),
            meta: Arc::new(Mutex::new(RasterMeta::default())),
            budget_bytes,
        }
    }

    /// Retrieve a cached vector slide if present.
    pub fn get_vector(&self, index: usize) -> Option<VectorSlide> {
        self.vector_cache.lock().ok()?.get(&index).cloned()
    }

    /// Insert a compiled vector slide into the Tier 1 cache.
    pub fn put_vector(&self, index: usize, slide: VectorSlide) {
        if let Ok(mut lock) = self.vector_cache.lock() {
            lock.insert(index, slide);
        }
    }

    /// Check if a vector slide is present in the Tier 1 cache.
    pub fn has_vector(&self, index: usize) -> bool {
        self.vector_cache
            .lock()
            .map(|c| c.contains_key(&index))
            .unwrap_or(false)
    }

    /// Retrieve a cached raster slide for the given target specification if present.
    pub fn get_raster(&self, index: usize, target: &RenderTargetSpec) -> Option<RgbaSlide> {
        self.raster_cache.lock().ok()?.get(&(index, *target)).cloned()
    }

    /// Widest cached raster for `index`, in O(1).
    ///
    /// The canvas falls back to this when its exact target is not cached yet
    /// (e.g. right after a resize), instead of scanning every cached raster.
    pub fn best_raster(&self, index: usize) -> Option<RgbaSlide> {
        let target = self.meta.lock().ok()?.best.get(&index).copied()?;
        if let Some(bitmap) = self.get_raster(index, &target) {
            return Some(bitmap);
        }
        // Defensive: metadata should never lag the map, but never return `None`
        // just because it did. Rescan only this slide's entries.
        let entries = self.raster_cache.lock().ok()?;
        let (_, bitmap) = entries
            .iter()
            .filter(|((slide, _), _)| *slide == index)
            .max_by_key(|((_, spec), _)| spec.width_px)?;
        Some(bitmap.clone())
    }

    /// Insert a rasterized RGBA slide into the Tier 2 cache.
    pub fn put_raster(&self, index: usize, target: RenderTargetSpec, slide: RgbaSlide) {
        let new_bytes = slide.pixels.len();
        let Ok(mut entries) = self.raster_cache.lock() else {
            return;
        };
        let previous = entries.insert((index, target), slide);
        let old_bytes = previous.as_ref().map(|b| b.pixels.len()).unwrap_or(0);

        let Ok(mut meta) = self.meta.lock() else {
            return;
        };
        meta.bytes = meta.bytes.saturating_sub(old_bytes) + new_bytes;
        if previous.is_none() {
            meta.order.push_back((index, target));
        }
        if meta
            .best
            .get(&index)
            .is_none_or(|current| current.width_px < target.width_px)
        {
            meta.best.insert(index, target);
        }

        // Oldest-first eviction keeps the budget honest without an LRU list.
        while meta.bytes > self.budget_bytes {
            let Some(key) = meta.order.pop_front() else {
                break;
            };
            let Some(evicted) = entries.remove(&key) else {
                continue;
            };
            meta.bytes = meta.bytes.saturating_sub(evicted.pixels.len());
            if meta.best.get(&key.0) == Some(&key.1) {
                let replacement = entries
                    .keys()
                    .filter(|(slide, _)| *slide == key.0)
                    .map(|(_, spec)| *spec)
                    .max_by_key(|spec| spec.width_px);
                match replacement {
                    Some(spec) => {
                        meta.best.insert(key.0, spec);
                    }
                    None => {
                        meta.best.remove(&key.0);
                    }
                }
            }
        }
    }

    /// Check if a raster slide is present in the Tier 2 cache.
    pub fn contains_raster(&self, index: usize, target: &RenderTargetSpec) -> bool {
        self.raster_cache
            .lock()
            .map(|c| c.contains_key(&(index, *target)))
            .unwrap_or(false)
    }

    /// Invalidate all rasterized frames (e.g. on target resolution change).
    pub fn clear_raster(&self) {
        if let Ok(mut lock) = self.raster_cache.lock() {
            lock.clear();
        }
        if let Ok(mut meta) = self.meta.lock() {
            *meta = RasterMeta::default();
        }
    }

    /// Invalidate both vector and raster caches (e.g. on deck reload).
    pub fn clear_all(&self) {
        if let Ok(mut vlock) = self.vector_cache.lock() {
            vlock.clear();
        }
        self.clear_raster();
    }

    /// Number of cached vector slides.
    pub fn vector_len(&self) -> usize {
        self.vector_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Number of cached raster slides.
    pub fn raster_len(&self) -> usize {
        self.raster_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Pixel bytes currently retained by the Tier 2 cache.
    pub fn raster_bytes(&self) -> usize {
        self.meta.lock().map(|m| m.bytes).unwrap_or(0)
    }

    /// Access the raw Tier 1 vector cache mutex.
    pub fn raw_vector_cache(&self) -> Arc<Mutex<HashMap<usize, VectorSlide>>> {
        Arc::clone(&self.vector_cache)
    }

    /// Access the raw Tier 2 raster cache mutex (read-only diagnostics).
    ///
    /// Mutating this map bypasses the byte budget and the best-target index.
    pub fn raw_raster_cache(&self) -> Arc<Mutex<HashMap<(usize, RenderTargetSpec), RgbaSlide>>> {
        Arc::clone(&self.raster_cache)
    }
}
