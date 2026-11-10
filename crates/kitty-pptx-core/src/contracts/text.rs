/// Positioned text run containing shaped glyphs and run styling.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyphRun {
    pub text: String,
    pub font_family: String,
    pub font_size_pt: f32,
    pub color: [u8; 4],
    pub glyphs: Vec<PositionedGlyph>,
    pub underline: bool,
    pub strikethrough: bool,
}

/// A single positioned glyph within a glyph run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32, // Offset in EMU from run origin
    pub y: f32,
    pub advance: f32,
}

/// SVG text rendering mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgTextMode {
    SelectableText,
    TextTags,
    VectorGlyphs,
}
