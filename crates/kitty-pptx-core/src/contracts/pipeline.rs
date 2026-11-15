use crate::error::RenderError;
use std::sync::Arc;

/// RgbaSlide is the pixel seam between composition and terminal graphics display.
/// Pixels are straight (non-premultiplied) RGBA8, tightly packed in row-major order.
#[derive(Clone)]
pub struct RgbaSlide {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // len == (width * height * 4) as usize
}

/// RenderTargetSpec is an opaque target resolution produced by display negotiation or CLI export.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RenderTargetSpec {
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WarningEvent {
    FontSubstituted {
        requested: String,
        substitute: String,
    },
    UnsupportedPreset {
        name: String,
    },
    MediaFailed {
        rel_id: String,
        reason: String,
    },
    FidelityDegraded {
        element_id: String,
        feature: &'static str,
        class: FidelityClass,
    },
    ResourceLimitHit {
        limit: &'static str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidelityClass {
    Full,
    Degraded,
    Placeholder,
    Omitted,
}

pub struct PrefetchPlan {
    pub center: usize,
    pub behind: usize,
    pub ahead: usize,
    pub stage: bool,
}

pub enum PipelineEntry {
    Ready { bitmap: RgbaSlide, staged: bool },
    Rendering,
    NotLoaded,
    Failed { error: RenderError },
}

pub enum InvalidateCause {
    Resize,
    TargetChanged,
    DeckReload,
}

pub trait RenderPipeline: Send + Sync {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec);

    fn entry(&self, index: usize) -> PipelineEntry;

    /// Resolves `index` against an explicit render target.
    ///
    /// Views that share a raster (the sorter grid reuses the canvas bitmap and
    /// lets the terminal scale it) look up the exact target they draw with
    /// instead of relying on the pipeline's last-requested target.
    /// Implementations without target-aware caches may fall back to
    /// [`RenderPipeline::entry`].
    fn entry_at(&self, index: usize, target: &RenderTargetSpec) -> PipelineEntry {
        let _ = target;
        self.entry(index)
    }

    fn invalidate(&self, cause: InvalidateCause);
}
