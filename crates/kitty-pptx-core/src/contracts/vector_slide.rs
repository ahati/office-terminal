use super::geometry::{AffineTransform, NormalizedRect, RectF32, VectorPath};
use super::paint::{VectorGradient, VectorPaint, VectorStroke};
use super::text::PositionedGlyphRun;
use super::shape_effects::ShapeEffects;
use super::hyperlink::SlideLink;
pub use ooxml_common::blip::BlipEffect;

/// Resolution-independent 2D vector display list for a PowerPoint slide.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorSlide {
    pub width_emu: i64,
    pub height_emu: i64,
    pub background: Option<VectorBackground>,
    pub commands: Vec<VectorCommand>,
    pub hyperlinks: Vec<SlideLink>,
}

impl VectorSlide {
    pub fn new(width_emu: i64, height_emu: i64) -> Self {
        Self {
            width_emu,
            height_emu,
            background: None,
            commands: Vec::new(),
            hyperlinks: Vec::new(),
        }
    }
}

/// Slide background style.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorBackground {
    Solid([u8; 4]),
    Gradient(VectorGradient),
    Image { rel_id: String },
}

/// Vector display command.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorCommand {
    DrawPath {
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
        effects: Box<ShapeEffects>,
    },
    DrawText {
        runs: Vec<PositionedGlyphRun>,
        transform: AffineTransform,
    },
    DrawImage {
        rel_id: String,
        src_rect: NormalizedRect, // [0, 1] crop coordinates
        dest_rect: RectF32,       // EMU bounding box
        opacity: f32,
        transform: AffineTransform,
        effects: Vec<BlipEffect>,
    },
    PushClip(VectorPath),
    PopClip,
}

impl VectorCommand {
    /// Helper to construct a DrawPath command with default effects.
    pub fn draw_path(
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
    ) -> Self {
        Self::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects: Box::new(ShapeEffects::default()),
        }
    }

    /// Helper to construct a DrawPath command with explicit shape effects.
    pub fn draw_path_with_effects(
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
        effects: ShapeEffects,
    ) -> Self {
        Self::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects: Box::new(effects),
        }
    }

    /// Helper to construct a DrawImage command without blip effects.
    pub fn draw_image(
        rel_id: String,
        src_rect: NormalizedRect,
        dest_rect: RectF32,
        opacity: f32,
        transform: AffineTransform,
    ) -> Self {
        Self::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            effects: Vec::new(),
        }
    }
}
