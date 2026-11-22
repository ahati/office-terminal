//! Slide background `<a:blipFill>` parity tests (ECMA-376 §20.1.8.14).
//!
//! Reference: `/tmp/oooxml/packages/pptx/src/renderer.ts:2218-2362`
//! (`prepareBackground`) — an opaque white base, a clip to the slide rectangle,
//! then either a `srcRect`/`fillRect` stretch (§20.1.8.55/.56) or a native-size
//! `a:tile` repeat (§20.1.8.58), with `alphaModFix` as a global alpha.

use std::sync::Arc;

use office_terminal_core::compiler::background::compile_slide_background;
use office_terminal_core::contracts::{
    BackgroundTile, NormalizedRect, RectF32, RenderTargetSpec, RgbaSlide, SvgTextMode,
    VectorBackground, VectorSlide,
};
use office_terminal_core::error::AssetError;
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use office_terminal_core::svg::{DefaultSlideSvgSerializer, SlideSvgSerializer};

/// `bands.png`: 100x100 vertical bands of 25 px — magenta, red, green, blue.
const BANDS: &str = "ppt/media/bands.png";
const BANDS_W: u32 = 100;
/// `quad.png`: 50x50 quadrant image — TL red, TR green, BL blue, BR yellow.
const QUAD: &str = "ppt/media/quad.png";
const QUAD_W: u32 = 50;
/// `black.png`: 8x8 opaque black, used for the opacity checks.
const BLACK: &str = "ppt/media/black.png";

fn bands_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (BANDS_W * BANDS_W * 4) as usize];
    for y in 0..BANDS_W {
        for x in 0..BANDS_W {
            let i = ((y * BANDS_W + x) * 4) as usize;
            let rgba = match x / 25 {
                0 => [255, 0, 255, 255],
                1 => [255, 0, 0, 255],
                2 => [0, 255, 0, 255],
                _ => [0, 0, 255, 255],
            };
            pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
    pixels
}

fn quad_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (QUAD_W * QUAD_W * 4) as usize];
    for y in 0..QUAD_W {
        for x in 0..QUAD_W {
            let i = ((y * QUAD_W + x) * 4) as usize;
            let rgba = match (x < QUAD_W / 2, y < QUAD_W / 2) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 0, 255],
            };
            pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
    pixels
}

fn black_pixels() -> Vec<u8> {
    [0u8, 0, 0, 255].repeat(8 * 8)
}

struct TestAssets;

impl TestAssets {
    fn pixels(rel_id: &str) -> Option<(u32, u32, Vec<u8>)> {
        match rel_id {
            BANDS => Some((BANDS_W, BANDS_W, bands_pixels())),
            QUAD => Some((QUAD_W, QUAD_W, quad_pixels())),
            BLACK => Some((8, 8, black_pixels())),
            _ => None,
        }
    }
}

impl AssetStore for TestAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let (width, height, pixels) =
            Self::pixels(rel_id).ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        Ok(DecodedImage {
            width,
            height,
            pixels: Arc::from(pixels.into_boxed_slice()),
        })
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        let (width, height, pixels) =
            Self::pixels(rel_id).ok_or_else(|| AssetError::NotFound(rel_id.to_string()))?;
        let rgba = image::RgbaImage::from_raw(width, height, pixels)
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

fn background(rel_id: &str) -> VectorBackground {
    VectorBackground::full_bleed_image(rel_id)
}

fn with_crop(rel_id: &str, l: f32, t: f32, r: f32, b: f32) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect {
            x: l,
            y: t,
            w: 1.0 - l - r,
            h: 1.0 - t - b,
        },
        fill_rect: None,
        tile: None,
        opacity: 1.0,
    }
}

fn with_fill_rect(rel_id: &str, fr: RectF32) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect::default(),
        fill_rect: Some(fr),
        tile: None,
        opacity: 1.0,
    }
}

fn with_opacity(rel_id: &str, opacity: f32) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect::default(),
        fill_rect: None,
        tile: None,
        opacity,
    }
}

fn with_tile(rel_id: &str, tile: BackgroundTile) -> VectorBackground {
    VectorBackground::Image {
        rel_id: rel_id.to_string(),
        src_rect: NormalizedRect::default(),
        fill_rect: None,
        tile: Some(tile),
        opacity: 1.0,
    }
}

fn rasterize(bg: VectorBackground, box_emu: i64, px: u32) -> RgbaSlide {
    let mut slide = VectorSlide::new(box_emu, box_emu);
    slide.background = Some(bg);
    TinySkiaRasterizer::new()
        .rasterize(
            &slide,
            &RenderTargetSpec {
                width_px: px,
                height_px: px,
            },
            &TestAssets,
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

fn assert_rgb(actual: [u8; 4], expected: [u8; 3], what: &str) {
    assert!(
        actual[3] == 255
            && (actual[0] as i32 - expected[0] as i32).abs() <= 2
            && (actual[1] as i32 - expected[1] as i32).abs() <= 2
            && (actual[2] as i32 - expected[2] as i32).abs() <= 2,
        "{}: expected {:?}, got {:?}",
        what,
        expected,
        actual
    );
}

const WHITE: [u8; 3] = [255, 255, 255];

// ---------------------------------------------------------------------------
// Rasterizer
// ---------------------------------------------------------------------------

#[test]
fn no_crop_background_is_full_bleed() {
    // ref renderer.ts:2298-2306 — absent fillRect leaves the destination at the
    // whole fill box; the four 25 px source bands land across the slide.
    let rendered = rasterize(background(BANDS), 1000, 100);
    assert_rgb(pixel(&rendered, 10, 50), [255, 0, 255], "band 0");
    assert_rgb(pixel(&rendered, 40, 50), [255, 0, 0], "band 1");
    assert_rgb(pixel(&rendered, 60, 50), [0, 255, 0], "band 2");
    assert_rgb(pixel(&rendered, 90, 50), [0, 0, 255], "band 3");
}

#[test]
fn src_rect_left_quarter_shows_only_the_right_seventy_five_percent() {
    // ref crop.ts:76-111 — the visible window [l, t, 1-r, 1-b] is stretched
    // over the full destination, so the magenta band is gone entirely.
    let rendered = rasterize(with_crop(BANDS, 0.25, 0.0, 0.0, 0.0), 1000, 100);
    for y in (0..rendered.height).step_by(7) {
        for x in 0..rendered.width {
            let p = pixel(&rendered, x, y);
            assert!(
                !(p[0] > 200 && p[1] < 40 && p[2] > 200),
                "pixel ({}, {}) still shows the cropped magenta band: {:?}",
                x,
                y,
                p
            );
        }
    }
    assert_rgb(pixel(&rendered, 5, 50), [255, 0, 0], "left of crop");
    assert_rgb(pixel(&rendered, 50, 50), [0, 255, 0], "middle of crop");
    assert_rgb(pixel(&rendered, 95, 50), [0, 0, 255], "right of crop");
}

#[test]
fn fill_rect_insets_leave_the_white_base_visible() {
    // ref renderer.ts:2298-2306 — l/t/r/b are destination insets of the box.
    let rendered = rasterize(
        with_fill_rect(BANDS, RectF32::new(0.25, 0.25, 0.5, 0.5)),
        1000,
        100,
    );
    for (x, y) in [(2, 2), (97, 2), (2, 97), (97, 97)] {
        assert_rgb(pixel(&rendered, x, y), WHITE, "inset corner");
    }
    assert_rgb(pixel(&rendered, 50, 50), [0, 255, 0], "centre is image");
}

#[test]
fn negative_fill_rect_inset_overscans_and_is_clipped_to_the_slide() {
    // Negative edges overscan past the box (ref renderer.ts:2331-2335) and the
    // paint is clipped to the slide rect (ref renderer.ts:2309-2314), so the
    // central half of the source fills the whole slide and nothing spills.
    let rendered = rasterize(
        with_fill_rect(BANDS, RectF32::new(-0.5, -0.5, 2.0, 2.0)),
        1000,
        100,
    );
    assert_eq!((rendered.width, rendered.height), (100, 100));
    for y in (0..rendered.height).step_by(7) {
        for x in 0..rendered.width {
            let p = pixel(&rendered, x, y);
            assert!(p[3] == 255, "overscan must stay opaque at ({}, {})", x, y);
            assert!(
                !(p[0] > 250 && p[1] > 250 && p[2] > 250),
                "white base leaked through at ({}, {})",
                x,
                y
            );
        }
    }
    assert_rgb(pixel(&rendered, 10, 50), [255, 0, 0], "left overscan");
    assert_rgb(pixel(&rendered, 90, 50), [0, 255, 0], "right overscan");
}

#[test]
fn opacity_multiplies_over_the_white_base() {
    // ref renderer.ts:2315 — `ctx.globalAlpha = fill.alpha` over the white base.
    let rendered = rasterize(with_opacity(BLACK, 0.5), 1000, 20);
    let p = pixel(&rendered, 10, 10);
    assert_eq!(p[3], 255);
    for channel in &p[..3] {
        assert!(
            (*channel as i32 - 127).abs() <= 3,
            "0.5 alpha black over white should be ~127, got {:?}",
            p
        );
    }

    // Full opacity keeps the source colour.
    let opaque = rasterize(with_opacity(BLACK, 1.0), 1000, 20);
    assert_rgb(pixel(&opaque, 10, 10), [0, 0, 0], "opaque black");
}

#[test]
fn tiled_2x2_produces_four_repeated_regions() {
    // ref renderer.ts:2434-2585 — a 50x50 blip repeats at its native 96-DPI
    // size (50 * 9525 EMU), so a 952500 EMU box holds exactly 2x2 tiles.
    let tile = BackgroundTile::default();
    let rendered = rasterize(with_tile(QUAD, tile), 952_500, 80);
    // Each tile is 40x40 device px; sample each tile's four quadrants.
    for (tile_x, tile_y) in [(0, 0), (40, 0), (0, 40), (40, 40)] {
        let base_x = tile_x as u32;
        let base_y = tile_y as u32;
        assert_rgb(
            pixel(&rendered, base_x + 10, base_y + 10),
            [255, 0, 0],
            "tile TL",
        );
        assert_rgb(
            pixel(&rendered, base_x + 30, base_y + 10),
            [0, 255, 0],
            "tile TR",
        );
        assert_rgb(
            pixel(&rendered, base_x + 10, base_y + 30),
            [0, 0, 255],
            "tile BL",
        );
        assert_rgb(
            pixel(&rendered, base_x + 30, base_y + 30),
            [255, 255, 0],
            "tile BR",
        );
    }
}

#[test]
fn tiled_flip_mirrors_the_second_cell() {
    // ref renderer.ts:2464-2471 — flip builds a 2x2 mirrored super-cell, so the
    // tile immediately right of the first one is mirrored left-to-right.
    let tile = BackgroundTile {
        flip: "x".to_string(),
        ..BackgroundTile::default()
    };
    let rendered = rasterize(with_tile(QUAD, tile), 952_500, 80);
    // Column 0 un-mirrored: red left, green right.
    assert_rgb(pixel(&rendered, 10, 10), [255, 0, 0], "tile 0 TL");
    assert_rgb(pixel(&rendered, 30, 10), [0, 255, 0], "tile 0 TR");
    // Column 1 mirrored: green left, red right.
    assert_rgb(pixel(&rendered, 50, 10), [0, 255, 0], "mirrored tile TL");
    assert_rgb(pixel(&rendered, 70, 10), [255, 0, 0], "mirrored tile TR");
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

fn model_image(
    src_rect: Option<(f64, f64, f64, f64)>,
    fill_rect: Option<pptx_model::FillRect>,
    tile: Option<pptx_model::TileInfo>,
    effects: Vec<ooxml_common::blip::BlipEffect>,
) -> pptx_model::Fill {
    pptx_model::Fill::Image {
        image_path: BANDS.to_string(),
        mime_type: "image/png".to_string(),
        svg_image_path: None,
        dpi: None,
        rot_with_shape: None,
        src_rect: src_rect.map(|(l, t, r, b)| ooxml_common::blip::SrcRect { l, t, r, b }),
        fill_rect,
        stretch: tile.is_none(),
        tile,
        alpha: None,
        duotone: None,
        blip_effects: effects,
    }
}

#[test]
fn compiler_carries_crop_insets_and_alpha() {
    let fill = model_image(
        Some((0.25, 0.0, 0.0, 0.0)),
        Some(pptx_model::FillRect {
            l: 0.1,
            t: 0.1,
            r: 0.1,
            b: 0.1,
        }),
        None,
        vec![ooxml_common::blip::BlipEffect::AlphaModFix { amt: 0.5 }],
    );
    match compile_slide_background(Some(&fill)) {
        Some(VectorBackground::Image {
            rel_id,
            src_rect,
            fill_rect,
            tile,
            opacity,
        }) => {
            assert_eq!(rel_id, BANDS);
            assert!((src_rect.x - 0.25).abs() < 1e-5);
            assert!((src_rect.w - 0.75).abs() < 1e-5);
            let fr = fill_rect.expect("fillRect");
            assert!((fr.x - 0.1).abs() < 1e-5);
            assert!((fr.w - 0.8).abs() < 1e-5);
            assert!(tile.is_none());
            assert!((opacity - 0.5).abs() < 1e-5);
        }
        other => panic!("expected an image background, got {:?}", other),
    }
}

#[test]
fn compiler_carries_tile_placement() {
    let fill = model_image(
        None,
        None,
        Some(pptx_model::TileInfo {
            tx: Some(123),
            ty: Some(-45),
            sx: Some(2.0),
            sy: Some(3.0),
            flip: Some("xy".to_string()),
            algn: Some("ctr".to_string()),
        }),
        Vec::new(),
    );
    match compile_slide_background(Some(&fill)) {
        Some(VectorBackground::Image { tile, opacity, .. }) => {
            let tile = tile.expect("tile");
            assert_eq!(tile.tx, 123);
            assert_eq!(tile.ty, -45);
            assert!((tile.sx - 2.0).abs() < 1e-5);
            assert!((tile.sy - 3.0).abs() < 1e-5);
            assert_eq!(tile.flip, "xy");
            assert_eq!(tile.algn, "ctr");
            assert!((opacity - 1.0).abs() < 1e-5);
        }
        other => panic!("expected an image background, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

fn serialize(bg: VectorBackground, box_emu: i64) -> String {
    let mut slide = VectorSlide::new(box_emu, box_emu);
    slide.background = Some(bg);
    DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &TestAssets, SvgTextMode::SelectableText)
        .expect("serialize svg")
}

#[test]
fn svg_no_crop_background_is_full_bleed_with_a_white_base() {
    let svg = serialize(background(BANDS), 1000);
    assert!(
        svg.contains(r##"<rect width="1000" height="1000" fill="#ffffff"/>"##),
        "white base missing:\n{svg}"
    );
    assert!(
        svg.contains(r#"x="0" y="0" width="1000" height="1000""#),
        "full-bleed image missing:\n{svg}"
    );
}

#[test]
fn svg_src_rect_emits_a_nested_viewbox_without_letterboxing() {
    let svg = serialize(with_crop(BANDS, 0.25, 0.0, 0.0, 0.0), 1000);
    assert!(
        svg.contains(r#"viewBox="0.250000 0.000000 0.750000 1.000000""#),
        "srcRect viewBox missing:\n{svg}"
    );
    assert!(
        svg.contains(r#"preserveAspectRatio="none""#),
        "the cropped slice must stretch, not letterbox:\n{svg}"
    );
}

#[test]
fn svg_fill_rect_sets_the_image_destination() {
    let svg = serialize(
        with_fill_rect(BANDS, RectF32::new(0.25, 0.25, 0.5, 0.5)),
        1000,
    );
    assert!(
        svg.contains(r#"x="250" y="250" width="500" height="500""#),
        "fillRect destination missing:\n{svg}"
    );
    assert!(
        svg.contains(r##"<rect width="1000" height="1000" fill="#ffffff"/>"##),
        "white base must remain visible outside the insets:\n{svg}"
    );
}

#[test]
fn svg_opacity_is_emitted() {
    let svg = serialize(with_opacity(BANDS, 0.5), 1000);
    assert!(svg.contains(r#"opacity="0.50""#), "opacity missing:\n{svg}");
}

#[test]
fn svg_tiled_background_emits_a_native_size_pattern() {
    let svg = serialize(with_tile(QUAD, BackgroundTile::default()), 952_500);
    assert!(
        svg.contains(
            r#"<pattern id="bg_tile" patternUnits="userSpaceOnUse" x="0" y="0" width="476250" height="476250">"#
        ),
        "tile pattern missing:\n{svg}"
    );
    assert!(
        svg.contains(r#"fill="url(#bg_tile)""#),
        "pattern fill missing:\n{svg}"
    );
    assert_eq!(svg.matches("<use ").count(), 1, "no flip ⇒ one tile cell");
}

#[test]
fn svg_tiled_flip_emits_a_two_by_two_super_cell() {
    let svg = serialize(
        with_tile(
            QUAD,
            BackgroundTile {
                flip: "xy".to_string(),
                ..BackgroundTile::default()
            },
        ),
        952_500,
    );
    assert!(
        svg.contains(r#"width="952500" height="952500""#),
        "flipped super-cell must double both axes:\n{svg}"
    );
    assert_eq!(
        svg.matches("<use ").count(),
        4,
        "flip needs the four mirrored cells:\n{svg}"
    );
}
