use super::geometry::{LineCap, LineJoin};

/// Paint descriptor for filling vector shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorPaint {
    Solid([u8; 4]),
    LinearGradient(VectorGradient),
    RadialGradient(VectorGradient),
}

/// Color gradient descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorGradient {
    pub angle_deg: f32,
    pub stops: Vec<(f32, [u8; 4])>,
}

/// Stroke descriptor for outlining vector shapes.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorStroke {
    pub color: [u8; 4],
    pub width_emu: i64,
    pub dash_segments: Option<Vec<f32>>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
}
