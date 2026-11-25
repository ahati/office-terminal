//! Content-addressed compiled-command cache (PRD 13 §9.1).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::contracts::{NormalizedRect, RectF32, VectorCommand};
use crate::loader::AssetStore;

use super::diagnostics::dedup;
use super::mapping::source_to_emu;
use super::recorder::MetafileRecorder;
use super::{MetafileDiagnostic, MetafileEngine, MetafileError, MetafileInfo, MetafileLimits};

/// One metafile's playback output, ready to be composed into a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledMetafile {
    pub commands: Vec<VectorCommand>,
    pub diagnostics: Vec<MetafileDiagnostic>,
    pub info: MetafileInfo,
}

/// Process-wide resolver used by [`super::expand_metafiles`].
#[derive(Default)]
pub struct MetafileResolver {
    entries: Mutex<HashMap<(String, u64), Arc<CompiledMetafile>>>,
}

impl MetafileResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops every compiled entry (deck reload, tests).
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    /// Plays `rel_id` once per `(content, crop, destination)` and caches the
    /// resulting commands.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &self,
        rel_id: &str,
        assets: &dyn AssetStore,
        engine: &dyn MetafileEngine,
        limits: &MetafileLimits,
        src_rect: &NormalizedRect,
        dest_rect: &RectF32,
    ) -> Result<Arc<CompiledMetafile>, MetafileError> {
        let raw = assets
            .get_raw_bytes(rel_id)
            .map_err(|error| MetafileError::Asset(format!("{error:?}")))?;
        if raw.len() > limits.max_input_bytes {
            return Err(MetafileError::Limit(format!(
                "{} bytes exceeds {}",
                raw.len(),
                limits.max_input_bytes
            )));
        }
        // `.emz`/`.wmz` are inflated with a hard output cap before playback
        // (PRD 13 §8.2). The cache key uses the inflated bytes, so repeated
        // pictures of one part always hit the same compiled commands.
        let bytes = super::unpack::unpack(&raw, rel_id, limits)?;

        // The key is derived only from the inputs (content + crop +
        // destination), so a repeat picture can be served before the bounds
        // probe — a cache hit must not replay the metafile (PRD 13 §9.2).
        let key = (rel_id.to_string(), content_hash(&bytes, src_rect, dest_rect));
        if let Some(hit) = self.entries.lock().ok().and_then(|entries| entries.get(&key).cloned()) {
            return Ok(hit);
        }

        let bounds = engine
            .output_bounds(&bytes, limits)?
            .filter(|bounds| bounds.w > 0.0 && bounds.h > 0.0)
            .ok_or_else(|| MetafileError::Invalid("metafile has no drawable bounds".into()))?;
        let unit_to_emu = source_to_emu(&bounds, src_rect, dest_rect)
            .ok_or_else(|| MetafileError::Invalid("degenerate crop or destination".into()))?;
        let emu_per_unit = ((unit_to_emu.sx.abs() + unit_to_emu.sy.abs()) * 0.5).max(1.0e-6);

        let mut recorder = MetafileRecorder::new(unit_to_emu, emu_per_unit);
        let outcome = engine.play(&bytes, limits, &mut recorder)?;
        let (commands, recorder_diagnostics) = recorder.finish();
        let mut diagnostics = recorder_diagnostics;
        diagnostics.extend(outcome.diagnostics);
        diagnostics = dedup(diagnostics);
        diagnostics.truncate(limits.max_diagnostics);

        let compiled = Arc::new(CompiledMetafile {
            commands,
            diagnostics,
            info: outcome.info,
        });
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key, Arc::clone(&compiled));
        }
        Ok(compiled)
    }
}

/// FNV-1a over the part bytes plus the crop/destination inputs.
///
/// The resolver runs on worker threads and the in-process cache only needs
/// collision resistance against the deck in hand; `blake3` from PRD 13 §9.1
/// remains a follow-up when a persisted cache lands.
fn content_hash(bytes: &[u8], src: &NormalizedRect, dest: &RectF32) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for value in [src.x, src.y, src.w, src.h, dest.x, dest.y, dest.w, dest.h] {
        for byte in value.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

static GLOBAL: OnceLock<MetafileResolver> = OnceLock::new();

/// Shared resolver used by expansion.
pub fn global_resolver() -> &'static MetafileResolver {
    GLOBAL.get_or_init(MetafileResolver::new)
}

/// Clears the shared resolver (deck reload/tests).
pub fn clear_global_cache() {
    global_resolver().clear();
}
