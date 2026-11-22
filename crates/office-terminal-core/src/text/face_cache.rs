//! The bounded LRUs behind the face catalogue.
//!
//! Reference: `packages/pptx/src/powerpoint-line-metrics.ts:51-81` (share
//! cache) and `:237-279` (interned face metrics). Both are `Map`s capped at
//! `SHARE_CACHE_LIMIT` entries with the least recently used key evicted first.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use super::faces::{FaceMetricsFull, SHARE_CACHE_LIMIT};

/// Insertion-ordered map with least-recently-used eviction: the reference's
/// `Map` + `keys().next()` idiom.
pub(super) struct Lru<T> {
    map: HashMap<String, Option<T>>,
    order: VecDeque<String>,
}

impl<T> Lru<T> {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(super) fn get(&mut self, key: &str) -> Option<&Option<T>> {
        if !self.map.contains_key(key) {
            return None;
        }
        self.order.retain(|known| known != key);
        self.order.push_back(key.to_string());
        self.map.get(key)
    }

    pub(super) fn insert(&mut self, key: String, value: Option<T>) {
        self.map.insert(key.clone(), value);
        // A re-insert is the most recent use again.
        self.order.retain(|known| *known != key);
        self.order.push_back(key);
        while self.map.len() > SHARE_CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub(super) fn len(&self) -> usize {
        self.map.len()
    }
}

static SHARE_CACHE: OnceLock<Mutex<Lru<f32>>> = OnceLock::new();
static FACE_CACHE: OnceLock<Mutex<Lru<Arc<FaceMetricsFull>>>> = OnceLock::new();

fn lock<T>(cell: &'static OnceLock<Mutex<T>>, init: fn() -> T) -> MutexGuard<'static, T> {
    cell.get_or_init(|| Mutex::new(init()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The share cache.
pub(super) fn shares() -> MutexGuard<'static, Lru<f32>> {
    lock(&SHARE_CACHE, Lru::new)
}

/// The interned face-metrics cache.
pub(super) fn faces() -> MutexGuard<'static, Lru<Arc<FaceMetricsFull>>> {
    lock(&FACE_CACHE, Lru::new)
}
