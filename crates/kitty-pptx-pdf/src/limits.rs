use std::time::Duration;

/// Maximum permissible raster dimension in pixels along any axis (PRD 14 §8).
pub const MAX_RASTER_AXIS: u32 = 8192;

/// Nominal canonical raster width for Tier-1 display lists.
pub const CANONICAL_TARGET_WIDTH: f32 = 2560.0;

/// Minimum clamp for canonical width.
pub const CANONICAL_WIDTH_MIN: f32 = 1024.0;

/// Maximum clamp for canonical width.
pub const CANONICAL_WIDTH_MAX: f32 = 4096.0;

/// Default page rendering deadline (PRD 14 §8).
pub const DEFAULT_PAGE_RENDER_DEADLINE: Duration = Duration::from_secs(10);

/// Calculates the deterministic canonical scale factor for a page of size `(w_pt, h_pt)`.
pub fn calculate_canonical_scale(w_pt: f32, h_pt: f32) -> f32 {
    if w_pt <= 0.0 || h_pt <= 0.0 {
        return 1.0;
    }

    let target_w = CANONICAL_TARGET_WIDTH.clamp(CANONICAL_WIDTH_MIN, CANONICAL_WIDTH_MAX);
    let scale = target_w / w_pt;

    // Enforce MAX_RASTER_AXIS clamp on both axes
    let max_w_scale = MAX_RASTER_AXIS as f32 / w_pt;
    let max_h_scale = MAX_RASTER_AXIS as f32 / h_pt;

    scale.min(max_w_scale).min(max_h_scale).max(0.1)
}
