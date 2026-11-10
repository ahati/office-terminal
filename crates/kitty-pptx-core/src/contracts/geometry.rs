/// 2D Fill rule for vector paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// A segment in a 2D vector path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadTo { x1: f32, y1: f32, x: f32, y: f32 },
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    Close,
}

/// Resolution-independent 2D path geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub fill_rule: FillRule,
}

impl VectorPath {
    pub fn new(segments: Vec<PathSegment>, fill_rule: FillRule) -> Self {
        Self { segments, fill_rule }
    }

    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
            fill_rule: FillRule::NonZero,
        }
    }
}

/// Line cap styles for vector strokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// Line join styles for vector strokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// 2D 2x3 affine transformation matrix:
/// [ sx  kx  tx ]
/// [ ky  sy  ty ]
/// [  0   0   1 ]
#[derive(Clone, Copy, Debug, PartialEq)]
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
        Self {
            sx: 1.0,
            ky: 0.0,
            kx: 0.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

impl AffineTransform {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            sx: 1.0,
            ky: 0.0,
            kx: 0.0,
            sy: 1.0,
            tx,
            ty,
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            sx,
            ky: 0.0,
            kx: 0.0,
            sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Multiply two affine transforms: self * other
    pub fn post_concat(&self, other: &AffineTransform) -> Self {
        Self {
            sx: self.sx * other.sx + self.kx * other.ky,
            ky: self.ky * other.sx + self.sy * other.ky,
            kx: self.sx * other.kx + self.kx * other.sy,
            sy: self.ky * other.kx + self.sy * other.sy,
            tx: self.sx * other.tx + self.kx * other.ty + self.tx,
            ty: self.ky * other.tx + self.sy * other.ty + self.ty,
        }
    }

    /// Apply transform to a point (x, y)
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.sx * x + self.kx * y + self.tx,
            self.ky * x + self.sy * y + self.ty,
        )
    }
}

/// Normalized sub-rectangle in [0.0, 1.0] coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for NormalizedRect {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }
}

/// 2D rectangle with f32 dimensions (often in EMU coordinates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectF32 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF32 {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}
