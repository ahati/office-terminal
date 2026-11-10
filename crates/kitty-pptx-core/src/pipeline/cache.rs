use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::contracts::{RenderTargetSpec, RgbaSlide, VectorSlide};

/// Two-tier pipeline cache storing resolution-independent VectorSlides
/// (Tier 1) and rasterized RgbaSlides (Tier 2).
#[derive(Clone, Default)]
pub struct PipelineCache {
    vector_cache: Arc<Mutex<HashMap<usize, VectorSlide>>>,
    raster_cache: Arc<Mutex<HashMap<(usize, RenderTargetSpec), RgbaSlide>>>,
}

impl PipelineCache {
    /// Create a new empty pipeline cache.
    pub fn new() -> Self {
        Self {
            vector_cache: Arc::new(Mutex::new(HashMap::new())),
            raster_cache: Arc::new(Mutex::new(HashMap::new())),
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

    /// Insert a rasterized RGBA slide into the Tier 2 cache.
    pub fn put_raster(&self, index: usize, target: RenderTargetSpec, slide: RgbaSlide) {
        if let Ok(mut lock) = self.raster_cache.lock() {
            lock.insert((index, target), slide);
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
    }

    /// Invalidate both vector and raster caches (e.g. on deck reload).
    pub fn clear_all(&self) {
        if let Ok(mut vlock) = self.vector_cache.lock() {
            vlock.clear();
        }
        if let Ok(mut rlock) = self.raster_cache.lock() {
            rlock.clear();
        }
    }

    /// Number of cached vector slides.
    pub fn vector_len(&self) -> usize {
        self.vector_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Number of cached raster slides.
    pub fn raster_len(&self) -> usize {
        self.raster_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Access the raw Tier 1 vector cache mutex.
    pub fn raw_vector_cache(&self) -> Arc<Mutex<HashMap<usize, VectorSlide>>> {
        Arc::clone(&self.vector_cache)
    }

    /// Access the raw Tier 2 raster cache mutex.
    pub fn raw_raster_cache(&self) -> Arc<Mutex<HashMap<(usize, RenderTargetSpec), RgbaSlide>>> {
        Arc::clone(&self.raster_cache)
    }
}
