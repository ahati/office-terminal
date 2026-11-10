use std::sync::Arc;
use crate::error::RenderError;

/// RgbaSlide is the pixel seam between composition and terminal graphics display.
/// Pixels are straight (non-premultiplied) RGBA8, tightly packed in row-major order.
#[derive(Clone)]
pub struct RgbaSlide {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // len == (width * height * 4) as usize
}

/// RenderTargetSpec is an opaque target resolution produced by display negotiation or CLI export.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RenderTargetSpec {
    pub width_px: u32,
    pub height_px: u32,
}

/// Resolution-independent 2D vector display list.
#[derive(Clone, Debug)]
pub struct VectorSlide {
    pub width_emu: i64,
    pub height_emu: i64,
    pub background: Option<VectorBackground>,
    pub commands: Vec<VectorCommand>,
}

#[derive(Clone, Debug)]
pub enum VectorBackground {
    Solid([u8; 4]),
    Gradient(VectorGradient),
    Image { rel_id: String },
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub fill_rule: FillRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug)]
pub enum PathSegment {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadTo { x1: f32, y1: f32, x: f32, y: f32 },
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    Close,
}

#[derive(Clone, Debug)]
pub enum VectorPaint {
    Solid([u8; 4]),
    LinearGradient(VectorGradient),
    RadialGradient(VectorGradient),
}

#[derive(Clone, Debug)]
pub struct VectorGradient {
    pub angle_deg: f32,
    pub stops: Vec<(f32, [u8; 4])>,
}

#[derive(Clone, Debug)]
pub struct VectorStroke {
    pub color: [u8; 4],
    pub width_emu: i64,
    pub dash_segments: Option<Vec<f32>>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug)]
pub struct AffineTransform {
    pub sx: f32,
    pub ky: f32,
    pub kx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for AffineTransform {
    fn default() -> Self {
        Self { sx: 1.0, ky: 0.0, kx: 0.0, sy: 1.0, tx: 0.0, ty: 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NormalizedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RectF32 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub struct PositionedGlyphRun {
    pub font_family: String,
    pub font_size_pt: f32,
    pub color: [u8; 4],
    pub glyphs: Vec<PositionedGlyph>,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32, // Offset in EMU
    pub y: f32,
    pub advance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WarningEvent {
    FontSubstituted { requested: String, substitute: String },
    UnsupportedPreset { name: String },
    MediaFailed { rel_id: String, reason: String },
    FidelityDegraded { element_id: String, feature: &'static str, class: FidelityClass },
    ResourceLimitHit { limit: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidelityClass {
    Full,
    Degraded,
    Placeholder,
    Omitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgTextMode {
    SelectableText,
    VectorGlyphs,
}

pub struct PrefetchPlan {
    pub center: usize,
    pub behind: usize,
    pub ahead: usize,
    pub stage: bool,
}

pub enum PipelineEntry {
    Ready { bitmap: RgbaSlide, staged: bool },
    Rendering,
    NotLoaded,
    Failed { error: RenderError },
}

pub enum InvalidateCause {
    Resize,
    TargetChanged,
    DeckReload,
}

pub trait RenderPipeline: Send + Sync {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec);
    fn entry(&self, index: usize) -> PipelineEntry;
    fn invalidate(&self, cause: InvalidateCause);
}
