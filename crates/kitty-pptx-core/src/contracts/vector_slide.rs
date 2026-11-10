use super::geometry::{AffineTransform, NormalizedRect, RectF32, VectorPath};
use super::paint::{VectorGradient, VectorPaint, VectorStroke};
use super::text::PositionedGlyphRun;

/// Resolution-independent 2D vector display list for a PowerPoint slide.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorSlide {
    pub width_emu: i64,
    pub height_emu: i64,
    pub background: Option<VectorBackground>,
    pub commands: Vec<VectorCommand>,
}

impl VectorSlide {
    pub fn new(width_emu: i64, height_emu: i64) -> Self {
        Self {
            width_emu,
            height_emu,
            background: None,
            commands: Vec::new(),
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
    },
    PushClip(VectorPath),
    PopClip,
}
