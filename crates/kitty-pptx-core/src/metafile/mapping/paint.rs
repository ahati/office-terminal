//! Brush / paint → `VectorPaint`.

use crate::contracts::{VectorGradient, VectorPaint, VectorPattern};

use crate::metafile::MetafilePaint;

/// Maps an engine brush into a display-list paint.
pub fn map_paint(paint: &MetafilePaint) -> Option<VectorPaint> {
    match paint {
        MetafilePaint::None => None,
        MetafilePaint::Solid(color) => Some(VectorPaint::Solid(*color)),
        MetafilePaint::LinearGradient { from, to, stops } => {
            let angle_deg = if to.0 == from.0 && to.1 == from.1 {
                0.0
            } else {
                (to.1 - from.1).atan2(to.0 - from.0).to_degrees()
            };
            Some(VectorPaint::LinearGradient(VectorGradient::new(
                angle_deg,
                stops.clone(),
            )))
        }
        MetafilePaint::Pattern { preset, fg, bg } => Some(VectorPaint::Pattern(VectorPattern {
            preset: (*preset).to_string(),
            fg: *fg,
            bg: *bg,
        })),
    }
}
