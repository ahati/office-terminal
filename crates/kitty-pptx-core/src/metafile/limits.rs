//! Resource limits for metafile playback (PRD 13 §8.1).

/// Bounds applied before any record is replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetafileLimits {
    /// Maximum compressed part size accepted for playback (32 MiB).
    pub max_input_bytes: usize,
    /// Maximum number of records replayed.
    pub max_records: usize,
    /// Maximum points in one geometry record.
    pub max_points_per_record: usize,
    /// Maximum GDI save/restore depth.
    pub max_dc_depth: usize,
    /// Maximum live pens/brushes/fonts/palettes.
    pub max_objects: usize,
    /// Maximum decoded bitmap pixels.
    pub max_pixels: u64,
    /// Maximum diagnostics collected per metafile.
    pub max_diagnostics: usize,
}

impl Default for MetafileLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_records: 200_000,
            max_points_per_record: 1_000_000,
            max_dc_depth: 64,
            max_objects: 16_384,
            max_pixels: 8192 * 8192,
            max_diagnostics: 64,
        }
    }
}
