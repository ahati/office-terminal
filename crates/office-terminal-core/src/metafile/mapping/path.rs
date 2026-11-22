//! Engine path → `VectorPath` in element-local EMU.

use crate::contracts::{AffineTransform, VectorPath};

use crate::metafile::MetafilePath;

/// Applies the output-units → EMU mapping to an engine path.
pub fn map_path(path: &MetafilePath, unit_to_emu: &AffineTransform) -> VectorPath {
    let raw = VectorPath::new(path.segments.clone(), path.fill_rule);
    super::transform_path(&raw, unit_to_emu)
}
