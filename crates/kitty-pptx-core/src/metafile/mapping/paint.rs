//! Brush / paint → `VectorPaint`.

use std::sync::Arc;

use crate::contracts::{
    AffineTransform, VectorGradient, VectorPaint, VectorPattern, VectorTextureFill,
};

use crate::metafile::MetafilePaint;

/// Maps an engine brush into a display-list paint.
pub fn map_paint(paint: &MetafilePaint, unit_to_emu: &AffineTransform) -> Option<VectorPaint> {
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
        MetafilePaint::Texture {
            png,
            width,
            height,
            transform,
            repeat,
            opacity,
        } => Some(VectorPaint::Texture(Box::new(VectorTextureFill {
            png: Arc::clone(png),
            width: *width,
            height: *height,
            transform: super::compose(unit_to_emu, transform),
            repeat: *repeat,
            opacity: *opacity,
        }))),
    }
}
