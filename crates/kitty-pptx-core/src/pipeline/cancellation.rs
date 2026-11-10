use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cooperative cancellation token used to abort pending or in-progress
/// background prefetch and rasterization tasks early.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    canceled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new uncanceled cancellation token.
    pub fn new() -> Self {
        Self {
            canceled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mark this token as canceled.
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }

    /// Check if cancellation has been requested.
    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::Acquire)
    }

    /// Reset the cancellation token to false.
    pub fn reset(&self) {
        self.canceled.store(false, Ordering::Release);
    }

    /// Obtain a reference to the underlying atomic boolean.
    pub fn raw_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.canceled)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
