//! Measurement seam shared by the layout kernels (placeholder).

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StyleId(pub usize);

pub trait TextMeasure {
    fn advance(&self, text: &str, style: StyleId) -> f32;
    fn space(&self, style: StyleId) -> f32;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabAlign {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Copy, Debug)]
pub struct TabStop {
    pub pos: f32,
    pub align: TabAlign,
}
