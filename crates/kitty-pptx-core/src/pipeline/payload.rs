//! Bounded cache of pre-encoded terminal transmission payloads.
//!
//! Rasterization workers insert a page's driver payload (e.g. Kitty base64)
//! right after rasterizing; the interactive syncer later looks it up instead
//! of encoding on the UI thread. Mirrors the byte-budget discipline of
//! `cache.rs`: entries are keyed `(slide index, render target)` and evicted
//! oldest-first once the running byte total exceeds the budget.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::contracts::{RenderTargetSpec, RgbaSlide, TransmitEncoder, TransmitPayload};

/// Default payload retention budget (128 MB of base64 bytes).
const PAYLOAD_BUDGET_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
struct PayloadMeta {
    /// Total base64 bytes currently retained.
    bytes: usize,
    /// Insertion order used as the eviction queue (stale keys are skipped).
    order: VecDeque<(usize, RenderTargetSpec)>,
}

/// Bounded cache of worker-pre-encoded transmission payloads.
///
/// Insertion-order eviction over a byte budget, mirroring the Tier 2 raster
/// cache in [`super::cache::PipelineCache`]. Cheap to clone: the map and its
/// bookkeeping live behind shared arcs, so the pipeline, its worker pool and
/// every worker thread observe one logical cache.
#[derive(Clone)]
pub struct PayloadCache {
    entries: Arc<Mutex<HashMap<(usize, RenderTargetSpec), TransmitPayload>>>,
    meta: Arc<Mutex<PayloadMeta>>,
    budget_bytes: usize,
}

impl Default for PayloadCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PayloadCache {
    /// Create an empty cache with the default payload byte budget.
    pub fn new() -> Self {
        Self::with_budget_bytes(PAYLOAD_BUDGET_BYTES)
    }

    /// Create an empty cache retaining at most `budget_bytes` of payloads.
    pub fn with_budget_bytes(budget_bytes: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            meta: Arc::new(Mutex::new(PayloadMeta::default())),
            budget_bytes,
        }
    }

    /// Retrieve a pre-encoded payload for the exact target, if cached.
    pub fn get(&self, index: usize, target: &RenderTargetSpec) -> Option<TransmitPayload> {
        self.entries.lock().ok()?.get(&(index, *target)).cloned()
    }

    /// Insert a pre-encoded payload, evicting oldest entries over budget.
    pub fn put(&self, index: usize, target: RenderTargetSpec, payload: TransmitPayload) {
        let new_bytes = payload.base64_len;
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let previous = entries.insert((index, target), payload);
        let old_bytes = previous.as_ref().map(|p| p.base64_len).unwrap_or(0);

        let Ok(mut meta) = self.meta.lock() else {
            return;
        };
        meta.bytes = meta.bytes.saturating_sub(old_bytes) + new_bytes;
        if previous.is_none() {
            meta.order.push_back((index, target));
        }

        // Oldest-first eviction keeps the budget honest without an LRU list.
        while meta.bytes > self.budget_bytes {
            let Some(key) = meta.order.pop_front() else {
                break;
            };
            let Some(evicted) = entries.remove(&key) else {
                continue;
            };
            meta.bytes = meta.bytes.saturating_sub(evicted.base64_len);
        }
    }

    /// Encode `bitmap` with `encoder` and cache the payload for
    /// `(index, target)`. Worker-thread side of the staging policy.
    ///
    /// Encode errors are skipped silently: the interactive syncer falls back
    /// to on-thread encoding through the driver, so a failed pre-encode must
    /// never panic or poison the worker.
    pub fn stage(
        &self,
        encoder: &dyn TransmitEncoder,
        index: usize,
        target: RenderTargetSpec,
        bitmap: &RgbaSlide,
    ) {
        if let Ok(payload) = encoder.encode(bitmap) {
            self.put(index, target, payload);
        }
    }

    /// Invalidate every cached payload (target change / deck reload).
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut meta) = self.meta.lock() {
            *meta = PayloadMeta::default();
        }
    }

    /// Number of cached payloads.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Whether the cache holds no payloads.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
