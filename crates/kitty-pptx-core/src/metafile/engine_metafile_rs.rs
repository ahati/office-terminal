//! Playback backend skeleton.
//!
//! M2 wires the vendored `metafile-core` / `-emf` / `-emfplus` / `-wmf` / `-dib`
//! crates behind this file (PRD 13 §2, §12). Until then the engine reports
//! [`MetafileError::Unavailable`], so every metafile picture degrades to the
//! standard placeholder with a warning instead of disappearing.

use super::{
    PlaybackOutcome, MetafileEngine, MetafileError, MetafileFormat, MetafileInfo, MetafileLimits,
    MetafileSink,
};

/// `metafile-rs`-backed playback engine (skeleton).
#[derive(Default)]
pub struct MetafileRsEngine;

impl MetafileEngine for MetafileRsEngine {
    fn format(&self, bytes: &[u8]) -> Option<MetafileFormat> {
        super::classify::classify_bytes(bytes)
    }

    fn inspect(&self, _bytes: &[u8]) -> Result<MetafileInfo, MetafileError> {
        // M2: parse the header (`metafile_emf::inspect` / `metafile_wmf::inspect`)
        // and return bounds + physical size.
        Err(MetafileError::Unavailable)
    }

    fn play(
        &self,
        _bytes: &[u8],
        _limits: &MetafileLimits,
        _sink: &mut dyn MetafileSink,
    ) -> Result<PlaybackOutcome, MetafileError> {
        // M2: `metafile_emf::playback(bytes, &options, &mut renderer)` with a
        // `metafile_core::Renderer` adapter forwarding to `sink`.
        Err(MetafileError::Unavailable)
    }
}
