use crate::error::RenderError;
use std::sync::Arc;

use super::transmit::TransmitPayload;

/// RgbaSlide is the pixel seam between composition and terminal graphics display.
/// Pixels are straight (non-premultiplied) RGBA8, tightly packed in row-major order.
#[derive(Clone)]
pub struct RgbaSlide {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // len == (width * height * 4) as usize
}

impl RgbaSlide {
    pub fn downscale_to(&self, max_width: u32, max_height: u32) -> RgbaSlide {
        if self.width <= max_width && self.height <= max_height {
            return self.clone();
        }
        let scale_w = max_width as f64 / self.width.max(1) as f64;
        let scale_h = max_height as f64 / self.height.max(1) as f64;
        let scale = scale_w.min(scale_h);
        let target_w = ((self.width as f64 * scale).round() as u32).max(1);
        let target_h = ((self.height as f64 * scale).round() as u32).max(1);

        // Resize from a borrowed view of the Arc-backed pixels: no full-raster
        // copy (an ~18 MB canvas raster used to be cloned per downscale).
        let src_img = match image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
            self.width,
            self.height,
            &self.pixels[..],
        ) {
            Some(img) => img,
            None => return self.clone(),
        };
        let resized = image::imageops::resize(
            &src_img,
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );
        RgbaSlide {
            width: target_w,
            height: target_h,
            pixels: Arc::from(resized.into_raw().into_boxed_slice()),
        }
    }
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
    Ready { bitmap: RgbaSlide },
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

    /// Drains warnings produced by compilation/expansion since the last call.
    ///
    /// Pipelines that do not surface diagnostics may keep the default no-op.
    fn drain_warnings(&self) -> Vec<(usize, WarningEvent)> {
        Vec::new()
    }

    /// Pre-encoded transmission payload for a cached raster, if the pipeline
    /// pre-encodes on its worker threads. `None` = the caller must encode.
    fn staged_payload(&self, index: usize, target: &RenderTargetSpec) -> Option<TransmitPayload> {
        let _ = (index, target);
        None
    }

    /// Prefetch a window of pages, each at its own render target.
    ///
    /// The default falls back to single-target `prefetch` using the first
    /// entry's target, preserving the previous semantics for simple pipelines.
    fn prefetch_window(&self, plan: &PrefetchPlan, targets: &[(usize, RenderTargetSpec)]) {
        if let Some((_, target)) = targets.first() {
            self.prefetch(plan, target);
        } else {
            let _ = plan;
        }
    }

    /// Number of slides currently queued or rendering (0 for simple pipelines).
    fn in_flight(&self) -> usize {
        0
    }

    /// Receiver that yields one event per worker-completed raster, letting the
    /// UI loop sleep until fresh results actually exist. `None` = no workers.
    fn completion_receiver(&self) -> Option<crossbeam_channel::Receiver<()>> {
        None
    }
}
