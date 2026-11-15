use std::sync::Arc;

use super::geometry::{AffineTransform, NormalizedRect, RectF32, VectorPath};
use super::paint::{VectorGradient, VectorPaint, VectorStroke};
use super::text::PositionedGlyphRun;
use super::shape_effects::ShapeEffects;
use super::hyperlink::SlideLink;
pub use ooxml_common::blip::BlipEffect;

/// Resolution-independent 2D vector display list for a PowerPoint slide.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorSlide {
    pub width_emu: i64,
    pub height_emu: i64,
    pub background: Option<VectorBackground>,
    pub commands: Vec<VectorCommand>,
    pub hyperlinks: Vec<SlideLink>,
}

impl VectorSlide {
    pub fn new(width_emu: i64, height_emu: i64) -> Self {
        Self {
            width_emu,
            height_emu,
            background: None,
            commands: Vec::new(),
            hyperlinks: Vec::new(),
        }
    }
}

/// Slide background style.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorBackground {
    Solid([u8; 4]),
    Gradient(VectorGradient),
    /// `<a:blipFill>` slide background (ECMA-376 §20.1.8.14). The reference
    /// paints an opaque white base, clips to the slide rectangle and only then
    /// places the blip, honouring the `srcRect` crop, the `fillRect`
    /// destination insets and `a:tile` repetition (ref renderer.ts:2218-2362).
    Image {
        rel_id: String,
        /// `<a:srcRect>` source window as fractions of the native bitmap. Held
        /// as the *logical* window (`x = l`, `y = t`, `w = 1-l-r`,
        /// `h = 1-t-b`) so negative insets stay outside `[0, 1]` and a tiled
        /// fill repeats the authored extent (`tileSourceExtent`,
        /// ref renderer.ts:2408-2417). Renderers clamp it to the bitmap.
        src_rect: NormalizedRect,
        /// `<a:stretch><a:fillRect>` destination as fractions of the slide box
        /// (`x = l`, `y = t`, `w = 1-l-r`, `h = 1-t-b`); negative insets
        /// overscan past the slide edge (ref renderer.ts:2298-2306).
        /// `None` = full-bleed.
        fill_rect: Option<RectF32>,
        /// `<a:tile>` repetition. `Some` selects the tiled placement and
        /// ignores `fill_rect` (the parser keeps them mutually exclusive).
        tile: Option<BackgroundTile>,
        /// `<a:alphaModFix>` opacity multiplier, `1.0` = opaque
        /// (ref renderer.ts:2315).
        opacity: f32,
    },
}

impl VectorBackground {
    /// Full-bleed image background: no crop, no insets, no tiling, opaque.
    /// The `Default` shape of [`VectorBackground::Image`].
    pub fn full_bleed_image(rel_id: impl Into<String>) -> Self {
        Self::Image {
            rel_id: rel_id.into(),
            src_rect: NormalizedRect::default(),
            fill_rect: None,
            tile: None,
            opacity: 1.0,
        }
    }
}

/// `<a:tile>` placement facts for a tiled slide background
/// (ECMA-376 §20.1.8.58 `CT_TileInfoProperties`).
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundTile {
    /// `<a:tile @tx>` phase offset in EMU.
    pub tx: i64,
    /// `<a:tile @ty>` phase offset in EMU.
    pub ty: i64,
    /// `<a:tile @sx>` horizontal scale as a fraction (`1.0` = native size).
    pub sx: f32,
    /// `<a:tile @sy>` vertical scale as a fraction.
    pub sy: f32,
    /// `<a:tile @flip>`: `none` | `x` | `y` | `xy`.
    pub flip: String,
    /// `<a:tile @algn>` `ST_RectAlignment`; absence registers the grid at the
    /// top-left (ref renderer.ts:2377-2403, `tile.algn ?? 'tl'`).
    pub algn: String,
}

impl Default for BackgroundTile {
    fn default() -> Self {
        Self {
            tx: 0,
            ty: 0,
            sx: 1.0,
            sy: 1.0,
            flip: "none".to_string(),
            algn: "tl".to_string(),
        }
    }
}

/// Sampling policy for [`VectorCommand::DrawBitmap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapSampling {
    /// Nearest-neighbour sampling (crisp pixels, GDI's 1:1 blit default).
    Nearest,
    /// Bilinear filtering (scaled or rotated placements).
    Linear,
}

/// Vector display command.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorCommand {
    DrawPath {
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
        effects: Box<ShapeEffects>,
    },
    /// One `a:headEnd` / `a:tailEnd` decoration.
    ///
    /// The reference paints decorations inside a translated tip frame after the
    /// shaft, so a tip can never be mistaken for a shaft segment
    /// (`drawArrowHead`, ref renderer.ts:4513-4535). The geometry is authored in
    /// that frame (tip at the origin, `-x` back into the line,
    /// `arrow-geometry.ts:161-188`); the compiler already places it at the
    /// tip by folding the frame into `transform`, so both backends paint it
    /// exactly like a path command.
    DrawLineEnd {
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
        effects: Vec<BlipEffect>,
    },
    /// Metafile picture marker (EMF / WMF / EMF+). The compiler cannot read
    /// archive bytes, so it emits this marker and `metafile::expand_metafiles`
    /// replaces it with playback output or the standard metafile placeholder.
    /// No marker may survive expansion into a backend.
    DrawMetafile {
        rel_id: String,
        /// Picture transform (position, size, rotation, flip).
        transform: AffineTransform,
        /// `<a:srcRect>` crop window (full when absent).
        src_rect: NormalizedRect,
        /// Visible destination box in picture-local EMU.
        dest_rect: RectF32,
        opacity: f32,
    },
    /// Bitmap embedded in another container (e.g. an EMF record stream).
    /// Carried inline as PNG so the display list stays self-contained across
    /// raster and SVG backends and cache targets.
    DrawBitmap {
        png: Arc<[u8]>,
        width: u32,
        height: u32,
        /// Maps the bitmap's `[0, 1]` square into element-local EMU.
        placement: AffineTransform,
        opacity: f32,
        sampling: BitmapSampling,
    },
    PushClip(VectorPath),
    PopClip,
}

impl VectorCommand {
    /// Helper to construct a DrawPath command with default effects.
    pub fn draw_path(
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
    ) -> Self {
        Self::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects: Box::new(ShapeEffects::default()),
        }
    }

    /// Helper to construct a DrawPath command with explicit shape effects.
    pub fn draw_path_with_effects(
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
        effects: ShapeEffects,
    ) -> Self {
        Self::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects: Box::new(effects),
        }
    }

    /// Helper to construct a DrawLineEnd command.
    pub fn draw_line_end(
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
    ) -> Self {
        Self::DrawLineEnd {
            path,
            fill,
            stroke,
            transform,
        }
    }

    /// Helper to construct a DrawImage command without blip effects.
    pub fn draw_image(
        rel_id: String,
        src_rect: NormalizedRect,
        dest_rect: RectF32,
        opacity: f32,
        transform: AffineTransform,
    ) -> Self {
        Self::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            effects: Vec::new(),
        }
    }
}
