use serde::{Deserialize, Serialize};

/// English Metric Units per typographic point (1 pt = 12,700 EMU).
pub const EMU_PER_PT: f32 = 12_700.0;

/// English Metric Units per 96-DPI pixel (1 px = 9,525 EMU).
pub const EMU_PER_PX: f32 = 9_525.0;

/// 2D Fill rule for vector paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// A segment in a 2D vector path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        x1: f32,
        y1: f32,
        x: f32,
        y: f32,
    },
    CubicTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
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

    /// Rotation by `radians` clockwise (screen y-down).
    pub fn from_rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            sx: cos,
            ky: sin,
            kx: -sin,
            sy: cos,
            tx: 0.0,
            ty: 0.0,
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
        (self.sx * x + self.kx * y + self.tx, self.ky * x + self.sy * y + self.ty)
    }
}

/// Normalized sub-rectangle in [0.0, 1.0] coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    pub fn intersects(&self, other: &RectF32) -> bool {
        self.x <= other.right() && self.right() >= other.x && self.y <= other.bottom() && self.bottom() >= other.y
    }

    pub fn intersection_area(&self, other: &RectF32) -> f32 {
        let w = self.right().min(other.right()) - self.x.max(other.x);
        let h = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }
}
