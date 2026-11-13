use super::geometry::{LineCap, LineJoin, NormalizedRect};
use super::vector_slide::BlipEffect;

/// One preset-pattern cell in EMU: the measured 8×8-cell tile spans 8 pt, so a
/// square cell is 1/8 pt (`12700 / 8`). ref packages/core/src/shape/paint.ts:459-476
/// (`sampleToUser = ptToUserUnits / 8`).
pub const PATTERN_CELL_EMU: f32 = 12_700.0 / 8.0;

/// Paint descriptor for filling vector shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorPaint {
    Solid([u8; 4]),
    LinearGradient(VectorGradient),
    RadialGradient(VectorGradient),
    /// ECMA-376 §20.1.8.14 `a:blipFill` on a shape, table cell or text box.
    /// The reference paints it inside the shape's own geometry path
    /// (ref packages/pptx/src/renderer.ts:2620-2700).
    Image(Box<VectorImageFill>),
    /// ECMA-376 §20.1.8.40 `a:pattFill` preset pattern, painted as a tiled
    /// bitmap on the [`PATTERN_CELL_EMU`] cell grid.
    Pattern(VectorPattern),
}

/// Color gradient descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorGradient {
    /// Authored `a:lin@ang` / `a:path` angle in degrees, kept verbatim: the
    /// renderers project it onto the fill frame ([`GradientFrame`]).
    pub angle_deg: f32,
    pub stops: Vec<(f32, [u8; 4])>,
    /// `a:gradFill` frame options (§20.1.8.32).
    pub frame: GradientFrame,
}

impl VectorGradient {
    /// A gradient that rotates with its shape and covers the whole fill box
    /// (the reference defaults, ref core/src/shape/paint.ts:283-315).
    pub fn new(angle_deg: f32, stops: Vec<(f32, [u8; 4])>) -> Self {
        Self {
            angle_deg,
            stops,
            frame: GradientFrame::default(),
        }
    }
}

/// `a:gradFill` frame options (ECMA-376 §20.1.8.32 `CT_GradientFillProperties`).
///
/// Every field defaults to the reference's behaviour for an omitted attribute:
/// the paint frame rotates with the shape, is not `scaled`, has no authored
/// `a:path`, focus (`a:fillToRect`) or repeat (`a:tileRect`) frame.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientFrame {
    /// §20.1.8.33 `a:gradFill@rotWithShape`; `false` counter-rotates the
    /// complete paint frame about the fill-box centre
    /// (ref core/src/shape/paint.ts:307-315).
    pub rot_with_shape: bool,
    /// Authored shape rotation in degrees. A reflected local frame reverses
    /// the counter-rotation sign (ref core/src/shape/paint.ts:262-265).
    pub rotation_deg: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// §20.1.8.41 `a:lin@scaled`: the gradient vector is scaled by the fill
    /// box before it is normalised (ref paint.ts:302-311).
    pub scaled: bool,
    /// §20.1.8.31 `a:path@path`: `""` (unset) | `circle` | `rect` | `shape`.
    /// `rect` selects the max-axis radius instead of the half-diagonal
    /// (ref paint.ts:129).
    pub path: String,
    /// §20.1.8.32 `a:fillToRect` focus frame as fractions of the tile box
    /// (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`); `None` = the whole box.
    pub fill_to_rect: Option<NormalizedRect>,
    /// §20.1.8.59 `a:tileRect` repeat frame in the same fractional form;
    /// `None` = no repetition.
    pub tile_rect: Option<NormalizedRect>,
    /// §20.1.8.59 `a:tileRect@flip`: `none` | `x` | `y` | `xy`.
    pub flip: String,
}

impl Default for GradientFrame {
    fn default() -> Self {
        Self {
            rot_with_shape: true,
            rotation_deg: 0.0,
            flip_h: false,
            flip_v: false,
            scaled: false,
            path: String::new(),
            fill_to_rect: None,
            tile_rect: None,
            flip: "none".to_string(),
        }
    }
}

impl GradientFrame {
    /// Rotation applied to the paint frame, in degrees: `0` when it rotates
    /// with the shape, `-rotation` for an unreflected shape and `+rotation` for
    /// a reflected one (`R F R = F`, ref core/src/shape/paint.ts:262-265 and
    /// `fill-frame.ts:4-7`). Mirrors
    /// [`VectorImageFill::counter_rotation_radians`].
    pub fn counter_rotation_deg(&self) -> f32 {
        if self.rot_with_shape || self.rotation_deg == 0.0 {
            return 0.0;
        }
        if self.flip_h != self.flip_v {
            self.rotation_deg
        } else {
            -self.rotation_deg
        }
    }

    /// §20.1.8.59 tile flip along x.
    pub fn flip_x(&self) -> bool {
        self.flip == "x" || self.flip == "xy"
    }

    /// §20.1.8.59 tile flip along y.
    pub fn flip_y(&self) -> bool {
        self.flip == "y" || self.flip == "xy"
    }
}

/// `<a:tile>` placement for a shape blip fill (ECMA-376 §20.1.8.58
/// `CT_TileInfoProperties`). An omitted attribute keeps its authored absence:
/// `sx`/`sy` default to 1 and `algn` to `tl`, matching the reference
/// (`tile.sx ?? 1`, `tile.algn ?? 'tl'`, ref renderer.ts:2453-2560).
#[derive(Clone, Debug, PartialEq)]
pub struct VectorTile {
    /// `ST_RectAlignment`; `tl` when absent or unknown.
    pub algn: String,
    /// Phase offset in EMU.
    pub tx: f32,
    /// Phase offset in EMU.
    pub ty: f32,
    /// Horizontal scale as a fraction (`1.0` = native size).
    pub sx: f32,
    /// Vertical scale as a fraction.
    pub sy: f32,
    /// `none` | `x` | `y` | `xy`.
    pub flip: String,
}

impl Default for VectorTile {
    fn default() -> Self {
        Self {
            algn: "tl".to_string(),
            tx: 0.0,
            ty: 0.0,
            sx: 1.0,
            sy: 1.0,
            flip: "none".to_string(),
        }
    }
}

impl VectorTile {
    pub fn flip_x(&self) -> bool {
        self.flip == "x" || self.flip == "xy"
    }

    pub fn flip_y(&self) -> bool {
        self.flip == "y" || self.flip == "xy"
    }
}

/// A non-background `a:blipFill` (ECMA-376 §20.1.8.14).
///
/// The two fill modes are `a:tile` (§20.1.8.58) and `a:stretch` (§20.1.8.56);
/// exactly one must be authored for the fill to paint
/// (`shapeImageFillModeIsPaintable`, ref renderer.ts:2610-2616).
#[derive(Clone, Debug, PartialEq)]
pub struct VectorImageFill {
    /// Embedded zip path of the blip (SVG twin preferred when present).
    pub rel_id: String,
    /// §20.1.8.55 source window as fractions of the native bitmap, held as the
    /// *logical* window (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`) so negative
    /// outsets stay outside `[0, 1]` (`tileSourceExtent`,
    /// ref renderer.ts:2408-2417). Renderers clamp it to the bitmap.
    pub src_rect: NormalizedRect,
    /// `<a:stretch><a:fillRect>` destination as fractions of the shape box
    /// (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`); `None` = the full box.
    /// ref renderer.ts:2668-2677.
    pub fill_rect: Option<NormalizedRect>,
    /// `<a:tile>` repetition; `Some` selects tile placement.
    pub tile: Option<VectorTile>,
    /// `a:stretch` presence. Kept beside `tile` so a schema-invalid pair stays
    /// observable (it fails closed at paint time).
    pub stretch: bool,
    /// `a:alphaModFix` opacity multiplier (`1.0` = opaque).
    pub alpha: f32,
    /// §20.1.8.33: `false` cancels the shape rotation about the fill-box centre
    /// (ref renderer.ts:2644-2656).
    pub rot_with_shape: bool,
    /// Authored blip DPI; `0` keeps the 96-DPI EMU-per-pixel basis.
    pub dpi: f32,
    /// `CT_Blip` pixel effects (§20.1.8.13) in document order.
    pub effects: Vec<BlipEffect>,
    /// Authored shape rotation in degrees, for the `rotWithShape="0"` counter
    /// rotation.
    pub rotation_deg: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl VectorImageFill {
    /// §20.1.8.14 `EG_FillModeProperties` is an optional choice of `tile` or
    /// `stretch` with no schema default, and the two are mutually exclusive:
    /// only an exactly-one-mode fill paints a blip.
    /// ref renderer.ts:2610-2616 (`shapeImageFillModeIsPaintable`).
    pub fn mode_is_paintable(&self) -> bool {
        self.tile.is_some() != self.stretch
    }

    /// Counter-rotation applied to the fill brush about the fill-box centre,
    /// in radians, for `rotWithShape="0"`. A reflected local frame reverses the
    /// rotation sign (`R F R = F`); the reflection itself is retained.
    /// ref renderer.ts:2644-2656.
    pub fn counter_rotation_radians(&self) -> f32 {
        if self.rot_with_shape || self.rotation_deg == 0.0 {
            return 0.0;
        }
        let sign = if self.flip_h != self.flip_v { 1.0 } else { -1.0 };
        sign * self.rotation_deg.to_radians()
    }
}

/// A preset pattern fill cell source (ECMA-376 §20.1.10.59 ST_PresetPatternVal).
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPattern {
    /// Preset name (`pct50`, `horz`, `diagCross`, …).
    pub preset: String,
    /// Foreground colour (`#` cells).
    pub fg: [u8; 4],
    /// Background colour (`.` cells).
    pub bg: [u8; 4],
}

/// Stroke descriptor for outlining vector shapes.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorStroke {
    pub color: [u8; 4],
    pub width_emu: i64,
    pub dash_segments: Option<Vec<f32>>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    /// Complete gradient/pattern paint of the line (`a:ln><a:gradFill>` /
    /// `a:ln><a:pattFill>`). The reference replaces the flat `color` with it
    /// whenever the line authors a non-solid fill, framing it on the stroked
    /// box (`applyStroke`, ref renderer.ts:7542-7565). `None` paints `color`.
    /// Boxed so `VectorCommand` stays small.
    pub paint: Option<Box<VectorPaint>>,
}

impl Default for VectorStroke {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            width_emu: 0,
            dash_segments: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            paint: None,
        }
    }
}
