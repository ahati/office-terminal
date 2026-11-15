//! Measurement seam shared by the layout kernels (placeholder).

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StyleId(pub usize);

pub trait TextMeasure {
    fn advance(&self, text: &str, style: StyleId) -> f32;
    fn space(&self, style: StyleId) -> f32;

    /// Host evidence that kinsoku may retract across a run seam. Defaults to
    /// false (C08); the real measurer enables it for matching East Asian runs.
    fn kinsoku_across_runs(&self, _left: StyleId, _right: StyleId) -> bool {
        false
    }
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
