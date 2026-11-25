//! The single compile → rasterize path.
//!
//! Both the synchronous renderer (`DefaultRenderPipeline::rasterize_now`) and
//! the background workers ([`super::worker`]) resolve pages through
//! [`render_page`], so the two paths cannot drift apart again: identical
//! bounds handling, cache tiers, native-first raster policy, warning
//! collection, and cooperative cancellation checkpoints.

use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;

use crate::contracts::{PageSource, RenderTargetSpec, RgbaSlide, VectorSlide, WarningEvent};
use crate::error::RenderError;
use crate::loader::asset_store::EmptyAssetStore;
use crate::loader::AssetStore;
use crate::rasterizer::SlideRasterizer;

use super::cache::PipelineCache;
use super::cancellation::CancellationToken;

/// Shared compile/expansion warning buffer: slide index + event pairs.
pub(crate) type WarningBuffer = Arc<Mutex<Vec<(usize, WarningEvent)>>>;

/// Moves warnings produced during one compile into the shared buffer.
///
/// The single home of warning collection: the sync renderer and every worker
/// task funnel their compile/expansion diagnostics through this.
pub(crate) fn flush_warnings(buffer: &WarningBuffer, index: usize, receiver: &Receiver<WarningEvent>) {
    let events: Vec<WarningEvent> = receiver.try_iter().collect();
    if events.is_empty() {
        return;
    }
    if let Ok(mut sink) = buffer.lock() {
        sink.extend(events.into_iter().map(|event| (index, event)));
    }
}

/// Reports whether a cooperative cancellation checkpoint tripped.
fn is_canceled(cancel: Option<&CancellationToken>) -> bool {
    cancel.is_some_and(|token| token.is_canceled())
}

/// Error returned when cancellation aborts a render before publication.
///
/// This value never surfaces to a caller: it is produced only after a
/// checkpoint observed `cancel` as already canceled, and the only caller that
/// passes a live token (the worker pool) drops errors while its token reports
/// canceled — cancellation is a supersede, not a failure. The synchronous
/// caller passes `None`, so its checkpoints can never trip.
fn canceled_before_publication() -> RenderError {
    RenderError::RasterizationError("render canceled before publication".to_string())
}

/// Renders page `index` at `target`, populating both cache tiers.
///
/// Resolution order:
/// 1. bounds check against `source.len()`;
/// 2. Tier 2 raster cache;
/// 3. native-first raster miss — raster-native sources (PDF) render directly
///    and never touch the vector tier, so the interactive pipeline never pays
///    a codec round-trip;
/// 4. the vector path: Tier 1 cache → `compile_page` → rasterize.
///
/// `cancel` receives the cooperative cancellation checkpoints the workers
/// historically enforced: before compiling, before publishing a compiled
/// scene, before rasterizing, and before publishing the fresh raster. A
/// canceled render publishes nothing. `assets` falls back to
/// [`EmptyAssetStore`] when `None`, matching the rasterizer contract.
///
/// The parameter set is the unified render contract itself (source,
/// rasterizer, assets, caches, warnings, page, target, cancellation); see the
/// `too_many_arguments` allow below.
#[allow(clippy::too_many_arguments)] // one parameter per pipeline seam; a wrapper struct would only obscure ownership
pub(crate) fn render_page(
    source: &dyn PageSource,
    rasterizer: &dyn SlideRasterizer,
    assets: Option<&Arc<dyn AssetStore>>,
    cache: &PipelineCache,
    warnings: &WarningBuffer,
    index: usize,
    target: &RenderTargetSpec,
    cancel: Option<&CancellationToken>,
) -> Result<RgbaSlide, RenderError> {
    if index >= source.len() {
        return Err(RenderError::CompilationFailed(format!(
            "slide index {} out of bounds (total slides: {})",
            index,
            source.len()
        )));
    }

    // Fast path: cached raster.
    if let Some(bitmap) = cache.get_raster(index, target) {
        return Ok(bitmap);
    }

    // Native-first raster miss. Errors propagate and latch like any other
    // render failure; `None` (the default) keeps the vector path.
    if let Some(result) = source.render_raster(index, target) {
        let bitmap = result?;
        cache.put_raster(index, *target, bitmap.clone());
        return Ok(bitmap);
    }

    // Vector path: Tier 1 cache → compile → rasterize. The cached scene is a
    // shared `Arc`, so publishing to the cache never deep-copies the display
    // list.
    let vector_slide: Arc<VectorSlide> = if let Some(cached) = cache.get_vector(index) {
        cached
    } else {
        if is_canceled(cancel) {
            return Err(canceled_before_publication());
        }

        let (warning_tx, warning_rx) = crossbeam_channel::unbounded::<WarningEvent>();
        let scene = match source.compile_page(index, &warning_tx) {
            Ok(scene) => scene,
            Err(error) => {
                flush_warnings(warnings, index, &warning_rx);
                return Err(error);
            }
        };

        // A cancellation that arrived during compilation must not publish a
        // stale scene into the cache (the deck may have been reloaded while
        // the compiler was running).
        if is_canceled(cancel) {
            return Err(canceled_before_publication());
        }

        // Publish warnings before the cache entry so a consumer that observes
        // the published result can also drain them.
        flush_warnings(warnings, index, &warning_rx);
        cache.put_vector(index, scene)
    };

    // Cooperative cancellation before expensive rasterization.
    if is_canceled(cancel) {
        return Err(canceled_before_publication());
    }

    let store: &dyn AssetStore = match assets {
        Some(store) => store.as_ref(),
        None => &EmptyAssetStore,
    };
    let bitmap = rasterizer.rasterize(&vector_slide, target, store)?;

    // Cooperative cancellation before publishing the fresh raster.
    if is_canceled(cancel) {
        return Err(canceled_before_publication());
    }

    cache.put_raster(index, *target, bitmap.clone());
    Ok(bitmap)
}
