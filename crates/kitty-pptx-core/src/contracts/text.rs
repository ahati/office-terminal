use super::shape_effects::VectorReflection;

/// A flattened warp boundary polyline in shape-local EMU.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpPolyline {
    pub points: Vec<(f32, f32)>,
    pub cumulative: Vec<f32>,
}

/// The two boundary curves of a text-warp envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpEnvelope {
    pub top: WarpPolyline,
    pub bottom: WarpPolyline,
    pub single_edge: bool,
}

/// Per-line parameters for a spec-driven WordArt warp.
#[derive(Clone, Debug, PartialEq)]
pub struct RunWarp {
    pub envelope: std::sync::Arc<WarpEnvelope>,
    pub band_frac: f32,
    pub warp_box_h: f32,
    pub h_scale: f32,
    pub line_w: f32,
}

/// DrawingML underline pattern (`ST_TextUnderlineType`, ECMA-376 §20.1.10.82).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DecorationStyle {
    #[default]
    Solid,
    Dotted,
    Dash,
    DashLong,
    DotDash,
    DotDotDash,
    Wavy,
}

/// One horizontal decoration rule: underline or strike.
///
/// Geometry is resolved by the compiler from the face's OpenType/OS/2 table
/// (or the reference's library fallback) and stored in EMU relative to the
/// run baseline. `gap_emu` is the centre-to-centre distance of a double rule;
/// zero means one rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationLine {
    pub color: [u8; 4],
    /// Top edge offset in slide EMU (y-down; positive = below the baseline).
    pub top_emu: f32,
    pub thickness_emu: f32,
    pub gap_emu: f32,
    pub style: DecorationStyle,
}

/// Run-level underline and strike geometry.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TextDecorations {
    pub underline: Option<DecorationLine>,
    pub strike: Option<DecorationLine>,
}

impl TextDecorations {
    pub fn is_empty(&self) -> bool {
        self.underline.is_none() && self.strike.is_none()
    }
}

/// Positioned text run containing shaped glyphs and run styling.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyphRun {
    pub text: String,
    pub font_family: String,
    pub font_size_pt: f32,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    pub glyphs: Vec<PositionedGlyph>,
    pub decorations: TextDecorations,
    /// Present when this run is mapped by a WordArt envelope.
    pub warp: Option<std::sync::Arc<RunWarp>>,
    /// Authored run-level reflection effect (`a:rPr/a:effectLst/a:reflection`).
    pub reflection: Option<VectorReflection>,
}

/// Per-glyph WordArt warp transform applied around the glyph baseline origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphTransform {
    /// Warped baseline origin in slide EMU.
    pub x: f32,
    pub y: f32,
    /// Rotation of the advance axis, radians (screen coordinates, y down).
    pub angle: f32,
    /// Horizontal scale of the glyph em-box.
    pub scale_x: f32,
    /// Vertical scale of the glyph em-box.
    pub scale_y: f32,
    /// Horizontal shear applied to the em-box before rotation.
    pub shear: f32,
}

/// A single positioned glyph within a glyph run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32, // Offset in EMU from run origin
    pub y: f32,
    pub advance: f32,
    /// Horizontal fraction of this glyph's centre along the warp line.
    pub warp_u: Option<f32>,
}

/// SVG text rendering mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgTextMode {
    SelectableText,
    TextTags,
    VectorGlyphs,
}
