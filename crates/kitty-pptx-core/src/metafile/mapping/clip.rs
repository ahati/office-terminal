//! Clip region → `VectorPath` in element-local EMU.

use crate::contracts::{AffineTransform, VectorPath};

use crate::metafile::MetafileClip;

/// Maps a clip region; an empty path means "clip everything" and is rejected
/// by the recorder (no command emitted) rather than turning into a no-op clip.
pub fn map_clip(clip: &MetafileClip, unit_to_emu: &AffineTransform) -> Option<VectorPath> {
    if clip.path.segments.is_empty() {
        return None;
    }
    Some(super::transform_path(&clip.path, unit_to_emu))
}
