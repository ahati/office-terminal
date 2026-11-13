//! Parity tests for `<a:srcRect>` picture cropping (ECMA-376 §20.1.8.55).
//!
//! Reference: `packages/core/src/image/crop.ts:76-111` (`imageCropProjection`)
//! and `crop.ts:191-231` (`drawImageCropped`) in `/tmp/oooxml`.
//!
//! The source fractions are already normalised by the parser
//! (`ooxml-common/src/blip.rs:132` divides the raw `ST_Percentage` by 100000),
//! so the compiler must pass them through untouched: a regression here collapses
//! every crop to "full frame" and stretches the image.

use std::sync::Arc;
use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{
    NormalizedRect, RectF32, RenderTargetSpec, RgbaSlide, SvgTextMode, VectorBackground, VectorCommand,
    VectorSlide,
};
use kitty_pptx_core::error::AssetError;
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use kitty_pptx_core::{DefaultSlideCompiler, SlideCompiler, DefaultSlideSvgSerializer, SlideSvgSerializer};
use pptx_model::Slide;

/// Synthetic source bitmap: 100x100, left quarter red, right three quarters
/// green, so a horizontal crop is observable pixel-by-pixel.
const W: u32 = 100;
const H: u32 = 100;
const REL_ID: &str = "ppt/media/crop.png";

fn source_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            let rgba = if x < 25 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
    pixels
}

struct CropAssetStore {
    pixels: Vec<u8>,
    png: Vec<u8>,
}

impl CropAssetStore {
    fn new() -> Self {
        let pixels = source_pixels();
        let img = image::RgbaImage::from_raw(W, H, pixels.clone()).expect("rgba buffer");
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .expect("encode png");
        Self { pixels, png }
    }
}

impl AssetStore for CropAssetStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        if rel_id == REL_ID {
            Ok(DecodedImage {
                width: W,
                height: H,
                pixels: Arc::from(self.pixels.clone().into_boxed_slice()),
            })
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id == REL_ID {
            Ok(self.png.clone())
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

/// Compile one 100x100 picture at the slide origin with the given
/// `(l, t, r, b)` fractions.
fn compile_picture(crop: Option<(f64, f64, f64, f64)>) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    let mut pic = serde_json::json!({
        "type": "picture",
        "x": 0,
        "y": 0,
        "width": W,
        "height": H,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "imagePath": REL_ID,
        "mimeType": "image/png",
    });
    if let Some((l, t, r, b)) = crop {
        pic["srcRect"] = serde_json::json!({ "l": l, "t": t, "r": r, "b": b });
    }

    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "elements": [pic],
        "elementSources": [],
        "comments": []
    });
    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");
    let scene = DefaultSlideCompiler::new()
        .compile(&slide, W as i64, H as i64, &tx)
        .expect("compile slide");
    scene.commands
}

fn only_image(commands: &[VectorCommand]) -> (NormalizedRect, RectF32) {
    let images: Vec<(NormalizedRect, RectF32)> = commands
        .iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawImage {
                src_rect,
                dest_rect,
                ..
            } => Some((*src_rect, *dest_rect)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1, "expected exactly one DrawImage, got {:?}", commands);
    images[0]
}

/// A distinct background colour separates "nothing painted" from "painted
/// white"; the synthetic asset is red/green and never blue.
const BG: [u8; 4] = [0, 0, 255, 255];

fn rasterize(commands: &[VectorCommand]) -> RgbaSlide {
    let mut slide = VectorSlide::new(W as i64, H as i64);
    slide.background = Some(VectorBackground::Solid(BG));
    slide.commands = commands.to_vec();
    let assets = CropAssetStore::new();
    TinySkiaRasterizer::new()
        .rasterize(
            &slide,
            &RenderTargetSpec {
                width_px: 40,
                height_px: 40,
            },
            &assets,
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

fn serialize(commands: &[VectorCommand]) -> String {
    let mut slide = VectorSlide::new(W as i64, H as i64);
    slide.commands = commands.to_vec();
    let assets = CropAssetStore::new();
    DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &assets, SvgTextMode::SelectableText)
        .expect("serialize svg")
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "{}: expected {}, got {}",
        what,
        expected,
        actual
    );
}

#[test]
fn left_quarter_crop_keeps_only_the_right_three_quarters() {
    let commands = compile_picture(Some((0.25, 0.0, 0.0, 0.0)));
    let (src, dest) = only_image(&commands);

    // ref crop.ts:86-97 — visible window is [l, t, 1-r, 1-b] in source space.
    assert_close(src.x, 0.25, "src.x");
    assert_close(src.y, 0.0, "src.y");
    assert_close(src.w, 0.75, "src.w");
    assert_close(src.h, 1.0, "src.h");
    // A positive inset stretches the slice back over the full picture frame.
    assert_eq!(dest, RectF32::new(0.0, 0.0, W as f32, H as f32));

    // Only green pixels (source x >= 25) may reach the output.
    let rendered = rasterize(&commands);
    for y in 0..rendered.height {
        for x in 0..rendered.width {
            let p = pixel(&rendered, x, y);
            assert!(
                p[1] > 200 && p[0] < 40 && p[3] == 255,
                "pixel ({}, {}) should be green, got {:?}",
                x,
                y,
                p
            );
        }
    }
}

#[test]
fn crop_covering_half_each_side_paints_nothing() {
    let commands = compile_picture(Some((0.5, 0.0, 0.5, 0.0)));
    // ref crop.ts:100-105 — no positive logical width means no paint at all; the
    // picture has no fill/stroke, so the display list stays empty.
    assert!(commands.is_empty(), "expected no commands, got {:?}", commands);

    let rendered = rasterize(&commands);
    let (pixels, _) = rendered.pixels.as_chunks::<4>();
    assert!(
        pixels.iter().all(|p| *p == BG),
        "a fully cropped picture must not paint any pixel"
    );
}

#[test]
fn crop_outside_the_bitmap_paints_nothing() {
    // l = 2 leaves a negative logical width (crop.ts:100-105).
    let commands = compile_picture(Some((2.0, 0.0, 0.0, 0.0)));
    assert!(commands.is_empty(), "expected no commands, got {:?}", commands);
}

#[test]
fn outset_crop_shrinks_the_destination_box() {
    // l = t = r = b = -0.1 -> logical 1.2; the clamped [0, 1] source maps to
    // (0.1 / 1.2) .. (1.1 / 1.2) of the frame (crop.ts:88-97).
    let commands = compile_picture(Some((-0.1, -0.1, -0.1, -0.1)));
    let (src, dest) = only_image(&commands);

    assert_eq!(src, NormalizedRect::default());
    assert_close(dest.x, W as f32 * 0.1 / 1.2, "dest.x");
    assert_close(dest.y, H as f32 * 0.1 / 1.2, "dest.y");
    assert_close(dest.w, W as f32 / 1.2, "dest.w");
    assert_close(dest.h, H as f32 / 1.2, "dest.h");

    // The gutters outside the destination box show the slide background.
    let rendered = rasterize(&commands);
    assert_eq!(pixel(&rendered, 0, 0), BG, "top-left gutter is empty");
    assert_eq!(pixel(&rendered, 39, 39), BG, "bottom-right gutter is empty");
    let centre = pixel(&rendered, 20, 20);
    assert_eq!(centre[3], 255, "centre is painted");
    assert!(centre[1] > 200 && centre[0] < 40, "centre is green: {:?}", centre);
}

#[test]
fn no_crop_leaves_the_display_list_unchanged() {
    let commands = compile_picture(None);
    let (src, dest) = only_image(&commands);

    assert_eq!(src, NormalizedRect::default());
    assert_eq!(dest, RectF32::new(0.0, 0.0, W as f32, H as f32));

    // The full source (left red band included) reaches the output.
    let rendered = rasterize(&commands);
    let left = pixel(&rendered, 0, 20);
    assert!(left[0] > 200 && left[1] < 40, "left band is red: {:?}", left);
    let right = pixel(&rendered, 39, 20);
    assert!(right[1] > 200 && right[0] < 40, "right band is green: {:?}", right);
}

#[test]
fn svg_emits_a_crop_viewport_for_a_cropped_picture() {
    let svg = serialize(&compile_picture(Some((0.25, 0.0, 0.0, 0.0))));

    assert!(
        svg.contains(r#"viewBox="0.250000 0.000000 0.750000 1.000000""#),
        "cropped svg must map the source window: {}",
        svg
    );
    assert!(
        svg.contains(r#"preserveAspectRatio="none""#),
        "the source slice must stretch, not letterbox: {}",
        svg
    );
    assert!(
        svg.contains(r#"width="100" height="100""#),
        "the destination box must stay the full frame: {}",
        svg
    );
    assert!(svg.contains(r#"<image href="data:image/png;base64,"#), "{}", svg);
    assert!(svg.contains(r#"x="0" y="0" width="1" height="1""#), "{}", svg);
}

#[test]
fn svg_without_a_crop_emits_the_plain_image_element() {
    let svg = serialize(&compile_picture(None));

    assert!(
        svg.contains(r#"<image href="data:image/png;base64,"#),
        "{}",
        svg
    );
    assert!(
        svg.contains(r#"x="0" y="0" width="100" height="100""#),
        "uncropped images keep the plain destination rect: {}",
        svg
    );
    assert!(
        !svg.contains("preserveAspectRatio"),
        "no crop viewport for an uncropped picture: {}",
        svg
    );
}

#[test]
fn svg_paints_nothing_for_an_empty_source_window() {
    let mut slide = VectorSlide::new(W as i64, H as i64);
    slide.commands.push(VectorCommand::DrawImage {
        rel_id: REL_ID.to_string(),
        src_rect: NormalizedRect {
            x: 0.5,
            y: 0.0,
            w: 0.0,
            h: 1.0,
        },
        dest_rect: RectF32::new(0.0, 0.0, W as f32, H as f32),
        opacity: 1.0,
        transform: Default::default(),
        effects: Vec::new(),
    });
    let assets = CropAssetStore::new();
    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &assets, SvgTextMode::SelectableText)
        .expect("serialize svg");

    assert!(
        !svg.contains("<image"),
        "an empty source window must paint nothing: {}",
        svg
    );
}
