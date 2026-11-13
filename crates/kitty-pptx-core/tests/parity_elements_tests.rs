//! Element-renderer parity: tiled blip fills, the cropped background
//! destination rectangle and the raster decode hint.
//!
//! Ported from the reference vitest specs (read-only @ `1f5c6e1d`):
//! `tile-fill.test.ts`, `renderer-picture-raster-target.test.ts` (the
//! cropped-destination half of the `fillRect` case) and
//! `renderer-adaptive-image-budget.test.ts` (the default native-decode case).
//!
//! All numbers are the reference's own; the reference's `measureText`-style
//! stubs are not needed here because every probe reads painted pixels or
//! compiled display-list fields. The chart ports live in
//! `parity_elements_chart_tests.rs` and the table-cell clip ports in
//! `parity_elements_table_tests.rs`.

// The table fixture is a deeply nested `json!` literal; serde_json's macro
// expands a few internal layers per JSON level.
#![recursion_limit = "512"]

use std::sync::{Arc, Mutex};

use kitty_pptx_core::compiler::background::compile_slide_background;
use kitty_pptx_core::contracts::{
    AffineTransform, BackgroundTile, NormalizedRect, RectF32, RenderTargetSpec, RgbaSlide,
    VectorBackground, VectorCommand, VectorSlide,
};
use kitty_pptx_core::error::AssetError;
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use kitty_pptx_core::svg::{DefaultSlideSvgSerializer, SlideSvgSerializer};
use kitty_pptx_core::contracts::SvgTextMode;
use serde_json::json;

/// 96-DPI pixel in EMU (ref renderer.ts:2453-2455, `pixelToEmu`).
const EMU_PER_PX: i64 = 9525;

// ───────────────────────────────────────────────────────────────────────────
// Asset store: deterministic synthetic bitmaps plus a request log.
// ───────────────────────────────────────────────────────────────────────────

struct Image {
    rel_id: &'static str,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// White bitmap with a 2x2 black marker at (`mx`, `my`). A tiled fill repeats
/// it at every tile origin, so the marker's device position *is* the tile
/// anchor / repeat period the reference's `tileAnchorOffset` returns.
fn marker_image(rel_id: &'static str, width: u32, height: u32, mx: u32, my: u32) -> Image {
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    for y in my..(my + 2).min(height) {
        for x in mx..(mx + 2).min(width) {
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
    Image {
        rel_id,
        width,
        height,
        pixels,
    }
}

/// Quadrant bitmap: TL red, TR green, BL blue, BR yellow.
fn quad_image(rel_id: &'static str, width: u32, height: u32) -> Image {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let rgba = match (x < width / 2, y < height / 2) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 0, 255],
            };
            pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
    Image {
        rel_id,
        width,
        height,
        pixels,
    }
}

#[derive(Default)]
struct Store {
    images: Vec<Image>,
    requests: Mutex<Vec<String>>,
}

impl Store {
    fn new(images: Vec<Image>) -> Self {
        Self {
            images,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request log").clone()
    }
}

impl AssetStore for Store {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let label = match hint {
            SizeHint::Native => format!("native:{rel_id}"),
            SizeHint::AtMost { width, height } => format!("atmost:{width}x{height}:{rel_id}"),
        };
        self.requests.lock().expect("request log").push(label);
        let image = self
            .images
            .iter()
            .find(|image| image.rel_id == rel_id)
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        Ok(DecodedImage {
            width: image.width,
            height: image.height,
            pixels: Arc::from(image.pixels.clone().into_boxed_slice()),
        })
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        let image = self
            .images
            .iter()
            .find(|image| image.rel_id == rel_id)
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        let rgba = image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
            .ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        let mut png = Vec::new();
        rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|_| AssetError::NotFound(rel_id.to_string()))?;
        Ok(png)
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

/// Serialize the slide to standalone SVG (the `a:tile` pattern carries the
/// exact EMU anchor `algn`-registered grid phase, `x`/`y` = `ax + tx`,
/// `ay + ty`). ref renderer.ts:2543-2559.
fn serialize_svg(slide: &VectorSlide, store: &Store) -> String {
    DefaultSlideSvgSerializer::new()
        .serialize_svg(slide, store, SvgTextMode::SelectableText)
        .expect("serialize svg")
}

/// The `(x, y, width, height)` an SVG tile pattern must declare, in EMU.
fn pattern_rect(ax: f32, ay: f32, w: f32, h: f32) -> String {
    format!(
        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
        ax * EMU_PER_PX as f32,
        ay * EMU_PER_PX as f32,
        w * EMU_PER_PX as f32,
        h * EMU_PER_PX as f32,
    )
}

/// A picture frame holding one `DrawImage` over a 9144000×6858000 EMU slide,
/// with the `srcRect` window `crop` = `(x, y, w, h)`.
fn picture_slide(rel_id: &str, frame: (f32, f32, f32, f32), crop: (f32, f32, f32, f32)) -> VectorSlide {
    let mut slide = VectorSlide::new(9_144_000, 6_858_000);
    slide.commands.push(VectorCommand::draw_image(
        rel_id.to_string(),
        NormalizedRect {
            x: crop.0,
            y: crop.1,
            w: crop.2,
            h: crop.3,
        },
        RectF32::new(frame.0, frame.1, frame.2, frame.3),
        1.0,
        AffineTransform::identity(),
    ));
    slide
}

fn target(width: u32, height: u32) -> RenderTargetSpec {
    RenderTargetSpec { width_px: width, height_px: height }
}

fn rasterize(slide: &VectorSlide, width: u32, height: u32, store: &Store) -> RgbaSlide {
    TinySkiaRasterizer::new()
        .rasterize(
            slide,
            &RenderTargetSpec {
                width_px: width,
                height_px: height,
            },
            store,
        )
        .expect("rasterize")
}

fn pixel(rendered: &RgbaSlide, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * rendered.width + x) * 4) as usize;
    [
        rendered.pixels[i],
        rendered.pixels[i + 1],
        rendered.pixels[i + 2],
        rendered.pixels[i + 3],
    ]
}

fn is_dark(px: [u8; 4]) -> bool {
    px[0] < 40 && px[1] < 40 && px[2] < 40
}

fn is_light(px: [u8; 4]) -> bool {
    px[0] > 215 && px[1] > 215 && px[2] > 215
}

// ───────────────────────────────────────────────────────────────────────────
// tile-fill.test.ts §20.1.8.41 ST_RectAlignment — `tileAnchorOffset`.
//
// Box 100x100 px, tile 40x30 px (distinct W/H so an axis mix-up surfaces).
// Horizontal: left = 0, centre = (100-40)/2 = 30, right = 100-40 = 60.
// Vertical:   top  = 0, middle = (100-30)/2 = 35, bottom = 100-30 = 70.
// ref renderer.ts:2377-2403 (`tileAnchor`).
// ───────────────────────────────────────────────────────────────────────────

fn tile_background(rel_id: &str, algn: &str) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect::default(),
        fill_rect: None,
        tile: Some(BackgroundTile {
            algn: algn.to_string(),
            ..BackgroundTile::default()
        }),
        opacity: 1.0,
    }
}

/// Paint the tiled background over a square box and assert the first tile's
/// top-left device pixel is exactly (`ax`, `ay`) — the marker is painted only
/// at the tile's own origin, so the sampled pixels pin the anchor.
fn assert_tile_anchor(algn: &str, ax: u32, ay: u32) {
    const TILE: &str = "ppt/media/tile.png";
    let store = Store::new(vec![marker_image(TILE, 40, 30, 0, 0)]);
    let mut slide = VectorSlide::new(100 * EMU_PER_PX, 100 * EMU_PER_PX);
    slide.background = Some(tile_background(TILE, algn));
    let rendered = rasterize(&slide, 100, 100, &store);

    assert!(
        is_dark(pixel(&rendered, ax, ay)),
        "algn={algn}: no tile origin at ({ax}, {ay})"
    );
    assert!(is_dark(pixel(&rendered, ax + 1, ay + 1)), "algn={algn}: 2x2 marker");
    if ax > 0 {
        assert!(
            is_light(pixel(&rendered, ax - 1, ay)),
            "algn={algn}: tile origin starts before x={ax}"
        );
    }
    if ay > 0 {
        assert!(
            is_light(pixel(&rendered, ax, ay - 1)),
            "algn={algn}: tile origin starts before y={ay}"
        );
    }

    // The raster grid is periodic, so also pin the exact authored phase in EMU:
    // `tileAnchorOffset` is (ax, ay) *before* the tx/ty offset.
    let svg = serialize_svg(&slide, &store);
    assert!(
        svg.contains(&pattern_rect(ax as f32, ay as f32, 40.0, 30.0)),
        "algn={algn}: pattern must register at ({ax}, {ay}) px:\n{svg}"
    );
}

#[test]
fn tile_algn_tl_anchors_at_0_0() {
    assert_tile_anchor("tl", 0, 0);
}

#[test]
fn tile_algn_t_anchors_at_30_0() {
    assert_tile_anchor("t", 30, 0);
}

#[test]
fn tile_algn_tr_anchors_at_60_0() {
    assert_tile_anchor("tr", 60, 0);
}

#[test]
fn tile_algn_l_anchors_at_0_35() {
    assert_tile_anchor("l", 0, 35);
}

#[test]
fn tile_algn_ctr_anchors_at_30_35() {
    assert_tile_anchor("ctr", 30, 35);
}

#[test]
fn tile_algn_r_anchors_at_60_35() {
    assert_tile_anchor("r", 60, 35);
}

#[test]
fn tile_algn_bl_anchors_at_0_70() {
    assert_tile_anchor("bl", 0, 70);
}

#[test]
fn tile_algn_b_anchors_at_30_70() {
    assert_tile_anchor("b", 30, 70);
}

#[test]
fn tile_algn_br_anchors_at_60_70() {
    assert_tile_anchor("br", 60, 70);
}

#[test]
fn tile_unknown_algn_defaults_to_top_left() {
    // §20.1.8.58 gives no schema default; PowerPoint treats absent/unknown as
    // `tl` (ref `tile.algn ?? 'tl'`, renderer.ts:2377-2403).
    assert_tile_anchor("???", 0, 0);
}

// ───────────────────────────────────────────────────────────────────────────
// tile-fill.test.ts §20.1.8.55 — `tileSourceExtent`: the cropped logical
// window is duplicated at its cropped *native* extent, negative outsets
// included (they widen the tile instead of being clamped).
// ref renderer.ts:2408-2417.
// ───────────────────────────────────────────────────────────────────────────

fn cropped_tile(rel_id: &str, l: f32, t: f32, r: f32, b: f32) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect {
            x: l,
            y: t,
            w: 1.0 - l - r,
            h: 1.0 - t - b,
        },
        fill_rect: None,
        tile: Some(BackgroundTile::default()),
        opacity: 1.0,
    }
}

#[test]
fn tile_duplicates_the_cropped_window_at_its_cropped_native_extent() {
    // tileSourceExtent(800, 600, {l:.25,t:.1,r:.15,b:.2}) => {width: 480,
    // height: 420}. The marker sits at the cropped window's own origin, so it
    // repeats every 480x420 px.
    const IMAGE: &str = "ppt/media/cropped.png";
    let store = Store::new(vec![marker_image(IMAGE, 800, 600, 200, 60)]);
    let mut slide = VectorSlide::new(2000 * EMU_PER_PX, 2000 * EMU_PER_PX);
    slide.background = Some(cropped_tile(IMAGE, 0.25, 0.1, 0.15, 0.2));
    let rendered = rasterize(&slide, 2000, 2000, &store);

    assert!(is_dark(pixel(&rendered, 0, 0)), "tile origin marker missing");
    assert!(
        is_dark(pixel(&rendered, 480, 420)),
        "the second tile must start at the 480x420 cropped extent"
    );
    assert!(is_light(pixel(&rendered, 479, 420)));
    assert!(is_light(pixel(&rendered, 480, 419)));
    let svg = serialize_svg(&slide, &store);
    assert!(
        svg.contains(&pattern_rect(0.0, 0.0, 480.0, 420.0)),
        "the tile cell must be the 480x420 cropped native extent:\n{svg}"
    );
}

#[test]
fn tile_retains_negative_source_outsets_as_transparent_extent() {
    // tileSourceExtent(800, 600, {l:-.1,t:0,r:-.15,b:0}) => {width: 1000,
    // height: 600}: the widened logical window is the tile extent.
    const IMAGE: &str = "ppt/media/outset.png";
    let store = Store::new(vec![marker_image(IMAGE, 800, 600, 0, 0)]);
    let mut slide = VectorSlide::new(2000 * EMU_PER_PX, 2000 * EMU_PER_PX);
    slide.background = Some(cropped_tile(IMAGE, -0.1, 0.0, -0.15, 0.0));
    let rendered = rasterize(&slide, 2000, 2000, &store);

    assert!(is_dark(pixel(&rendered, 0, 0)), "tile origin marker missing");
    assert!(
        is_dark(pixel(&rendered, 1000, 600)),
        "the second tile must start at the widened 1000x600 extent"
    );
    assert!(is_light(pixel(&rendered, 999, 600)));
    assert!(is_light(pixel(&rendered, 1000, 599)));
    let svg = serialize_svg(&slide, &store);
    assert!(
        svg.contains(&pattern_rect(0.0, 0.0, 1000.0, 600.0)),
        "negative outsets widen the tile to 1000x600:\n{svg}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// renderer-picture-raster-target.test.ts — "applies background fillRect to
// both decode sizing and cropped destination paint": the destination half.
// A 9144000x6858000 EMU slide at 960 CSS px is 960x720, so
// fillRect {l:.1,t:-.1,r:.2,b:.05} paints the rect (96,-72,672,756).
// ref renderer.ts:2298-2306.
// ───────────────────────────────────────────────────────────────────────────

fn model_image(
    image_path: &str,
    src_rect: Option<(f64, f64, f64, f64)>,
    fill_rect: Option<(f64, f64, f64, f64)>,
) -> pptx_model::Fill {
    // Both rect types hold signed *fractions* of the reference box.
    let src_rect = src_rect.map(|(l, t, r, b)| json!({ "l": l, "t": t, "r": r, "b": b }));
    let fill_rect = fill_rect.map(|(l, t, r, b)| json!({ "l": l, "t": t, "r": r, "b": b }));
    let value = json!({
        "fillType": "image",
        "imagePath": image_path,
        "mimeType": "image/png",
        "stretch": fill_rect.is_some(),
        "srcRect": src_rect,
        "fillRect": fill_rect,
    });
    serde_json::from_value(value).expect("image fill fixture")
}

#[test]
fn background_fill_rect_paints_the_cropped_destination_rectangle() {
    const IMAGE: &str = "ppt/media/background.png";
    let fill = model_image(
        IMAGE,
        Some((0.25, 0.0, 0.1, 0.0)),
        Some((0.1, -0.1, 0.2, 0.05)),
    );
    let background = compile_slide_background(Some(&fill)).expect("image background");
    match &background {
        VectorBackground::Image {
            rel_id,
            src_rect,
            fill_rect,
            tile,
            ..
        } => {
            assert_eq!(rel_id, IMAGE);
            assert!((src_rect.x - 0.25).abs() < 1e-5);
            assert!((src_rect.w - 0.65).abs() < 1e-5);
            let fr = fill_rect.expect("fillRect");
            assert!((fr.x - 0.1).abs() < 1e-5);
            assert!((fr.y + 0.1).abs() < 1e-5);
            assert!((fr.w - 0.7).abs() < 1e-5);
            assert!((fr.h - 1.05).abs() < 1e-5);
            assert!(tile.is_none());
        }
        other => panic!("expected an image background, got {other:?}"),
    }

    let store = Store::new(vec![quad_image(IMAGE, 200, 200)]);
    let mut slide = VectorSlide::new(9_144_000, 6_858_000);
    slide.background = Some(background);
    let rendered = rasterize(&slide, 960, 720, &store);

    // device rect x = 96 .. 768, y = -72 .. 684.
    assert!(is_dark_or_colour(pixel(&rendered, 96, 0)), "left edge of the fillRect");
    assert!(is_light(pixel(&rendered, 95, 0)), "left of the fillRect stays white");
    assert!(is_dark_or_colour(pixel(&rendered, 767, 400)), "right edge of the fillRect");
    assert!(is_light(pixel(&rendered, 768, 400)), "right of the fillRect stays white");
    assert!(is_dark_or_colour(pixel(&rendered, 400, 683)), "bottom edge of the fillRect");
    assert!(is_light(pixel(&rendered, 400, 684)), "below the fillRect stays white");
}

/// A painted blip pixel (any opaque colour) — the quad palette is not white.
fn is_dark_or_colour(px: [u8; 4]) -> bool {
    px[3] == 255 && !is_light(px)
}

// ───────────────────────────────────────────────────────────────────────────
// renderer-adaptive-image-budget.test.ts — "preserves native decoding for an
// ordinary slide raster that fits by default": the rasterizer asks the store
// for the native bitmap and keeps its pixels. The rest of that spec, and the
// device-pixel derivation of `renderer-picture-raster-target.test.ts`, live in
// `parity_elements_support/decode_plan_cases.rs`.
// ref renderer.ts picture decode path, core/src/image/adaptive-image-budget.ts.
// ───────────────────────────────────────────────────────────────────────────

#[path = "parity_elements_support/decode_plan_cases.rs"]
mod decode_plan_cases;

#[test]
fn picture_raster_requests_the_native_source_grid_by_default() {
    const IMAGE: &str = "ppt/media/photo.png";
    let store = Store::new(vec![quad_image(IMAGE, 800, 600)]);
    let mut slide = VectorSlide::new(9_144_000, 6_858_000);
    slide.commands.push(VectorCommand::draw_image(
        IMAGE.to_string(),
        NormalizedRect::default(),
        RectF32::new(0.0, 0.0, 4_572_000.0, 6_858_000.0),
        1.0,
        AffineTransform::identity(),
    ));
    let rendered = rasterize(&slide, 960, 720, &store);

    assert_eq!(store.requests(), vec![format!("native:{IMAGE}")]);
    // 4572000 EMU of 9144000 = 480 px wide, full height.
    assert_eq!(pixel(&rendered, 100, 100), [255, 0, 0, 255]);
    assert_eq!(pixel(&rendered, 400, 100), [0, 255, 0, 255]);
    assert_eq!(pixel(&rendered, 100, 600), [0, 0, 255, 255]);
    assert_eq!(pixel(&rendered, 400, 600), [255, 255, 0, 255]);
    assert!(is_light(pixel(&rendered, 600, 300)), "outside the frame stays white");
}

