//! Slide dim overlay: a translucent scrim painted over the finished slide.
//!
//! Ported from the reference's dim pass (ref renderer.ts:8110-8122): save →
//! `globalAlpha = opacity` → `fillStyle = color` → `fillRect(0, 0, w, h)` →
//! restore, applied after the whole slide (ref renderer.ts:9091) and never
//! looking at `slide.hidden`. In a display list the scrim is simply the last
//! command: the `globalAlpha × fillStyle-alpha` product folds into the RGBA.

use crate::compiler::style::modulate_alpha;
use crate::contracts::{AffineTransform, FillRule, PathSegment, VectorCommand, VectorPaint, VectorPath};

/// `DimOptions` (ref `types.ts:250-255`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimOptions {
    /// Scrim colour, RGBA.
    pub color: [u8; 4],
    /// `globalAlpha` multiplier in `[0, 1]`.
    pub opacity: f32,
}

/// Append one full-slide scrim as the final command.
pub fn apply_dim_overlay(slide: &mut crate::contracts::VectorSlide, dim: &DimOptions) {
    let w = slide.width_emu as f32;
    let h = slide.height_emu as f32;
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    // `globalAlpha` composes with the fill's own alpha (ref renderer.ts:8114).
    let fill = modulate_alpha(dim.color, dim.opacity);
    slide.commands.push(VectorCommand::draw_path(
        path,
        Some(VectorPaint::Solid(fill)),
        None,
        AffineTransform {
            sx: 1.0,
            ky: 0.0,
            kx: 0.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        },
    ));
}
