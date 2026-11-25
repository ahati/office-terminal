use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{
    AffineTransform, FidelityClass, NormalizedRect, RectF32, RenderTargetSpec, VectorCommand, VectorSlide, WarningEvent,
};
use office_terminal_core::error::AssetError;
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::rasterizer::effects::{
    apply_alpha_mod, apply_bilevel, apply_blip_effects, apply_duotone, apply_grayscale, bt709_luma_u8,
};
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use office_terminal_core::{DefaultSlideCompiler, SlideCompiler};
use ooxml_common::blip::BlipEffect;
use pptx_model::*;
use std::sync::Arc;

#[test]
fn test_bt709_grayscale_conversion() {
    // Test pure RGB and edge cases
    assert_eq!(bt709_luma_u8(255, 0, 0), 54);
    assert_eq!(bt709_luma_u8(0, 255, 0), 182);
    assert_eq!(bt709_luma_u8(0, 0, 255), 18);
    assert_eq!(bt709_luma_u8(255, 255, 255), 255);
    assert_eq!(bt709_luma_u8(0, 0, 0), 0);

    // Test buffer conversion and alpha preservation
    let mut pixels = vec![
        255, 0, 0, 200, // Red
        0, 255, 0, 150, // Green
        0, 0, 255, 100, // Blue
        255, 255, 255, 50, // White
        0, 0, 0, 25, // Black
        100, 150, 200, 255, // Mixed
    ];

    apply_grayscale(&mut pixels);

    assert_eq!(&pixels[0..4], &[54, 54, 54, 200]);
    assert_eq!(&pixels[4..8], &[182, 182, 182, 150]);
    assert_eq!(&pixels[8..12], &[18, 18, 18, 100]);
    assert_eq!(&pixels[12..16], &[255, 255, 255, 50]);
    assert_eq!(&pixels[16..20], &[0, 0, 0, 25]);
    assert_eq!(&pixels[20..24], &[143, 143, 143, 255]);
}

#[test]
fn test_bilevel_thresholding() {
    // Threshold 0.5 (127.5 on 0..255 scale)
    let mut pixels = vec![
        255, 0, 0, 255, // Red: luma 54.213 <= 127.5 -> black
        0, 255, 0, 255, // Green: luma 182.376 > 127.5 -> white
        0, 0, 255, 128, // Blue: luma 18.411 <= 127.5 -> black, alpha preserved
        255, 255, 255, 80, // White: luma 255 > 127.5 -> white, alpha preserved
        0, 0, 0, 255, // Black: luma 0 <= 127.5 -> black
    ];

    apply_bilevel(&mut pixels, 0.5);

    assert_eq!(&pixels[0..4], &[0, 0, 0, 255]);
    assert_eq!(&pixels[4..8], &[255, 255, 255, 255]);
    assert_eq!(&pixels[8..12], &[0, 0, 0, 128]);
    assert_eq!(&pixels[12..16], &[255, 255, 255, 80]);
    assert_eq!(&pixels[16..20], &[0, 0, 0, 255]);

    // Threshold 0.2: Red has luma 54.213 / 255 = 0.2126 > 0.2 -> white
    let mut red_pixel = vec![255, 0, 0, 255];
    apply_bilevel(&mut red_pixel, 0.2);
    assert_eq!(&red_pixel, &[255, 255, 255, 255]);
}

#[test]
fn test_duotone_linear_interpolation() {
    // Dark endpoint: Blue (0000FF); Light endpoint: Yellow (FFFF00)
    let mut pixels = vec![
        0, 0, 0, 255, // Black: luma 0.0 -> Blue (0, 0, 255)
        255, 255, 255, 200, // White: luma 1.0 -> Yellow (255, 255, 0), alpha preserved
    ];

    apply_duotone(&mut pixels, "0000FF", "FFFF00");

    assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[4..8], &[255, 255, 0, 200]);

    // Duotone from Black to White is equivalent to grayscale
    let mut gray_pixels = vec![255, 0, 0, 255, 0, 255, 0, 255];
    apply_duotone(&mut gray_pixels, "000000", "FFFFFF");
    assert_eq!(&gray_pixels[0..4], &[54, 54, 54, 255]);
    assert_eq!(&gray_pixels[4..8], &[182, 182, 182, 255]);
}

#[test]
fn test_alpha_modulation() {
    let mut pixels = vec![100, 150, 200, 255, 100, 150, 200, 100];

    // Modulate by 0.5
    apply_alpha_mod(&mut pixels, 0.5);

    assert_eq!(&pixels[0..4], &[100, 150, 200, 128]);
    assert_eq!(&pixels[4..8], &[100, 150, 200, 50]);

    // Raw OOXML 100000 scale: 80000 = 80%
    apply_alpha_mod(&mut pixels, 80000.0);
    assert_eq!(pixels[3], (128.0 * 0.8f32).round() as u8);
}

#[test]
fn test_apply_blip_effects_sequence() {
    let mut pixels = vec![255, 0, 0, 255]; // Red

    let effects = vec![BlipEffect::Grayscale, BlipEffect::AlphaModFix { amt: 0.5 }];

    apply_blip_effects(&mut pixels, &effects);
    assert_eq!(&pixels, &[54, 54, 54, 128]);
}

#[test]
fn test_warning_events_blip_effects() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "elements": [
            {
                "type": "picture",
                "x": 0,
                "y": 0,
                "width": 100000,
                "height": 100000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "imagePath": "ppt/media/photo.jpg",
                "mimeType": "image/jpeg",
                "blipEffects": [
                    { "type": "grayscale" },
                    { "type": "biLevel", "thresh": 0.5 }
                ]
            },
            {
                "type": "picture",
                "x": 200000,
                "y": 200000,
                "width": 100000,
                "height": 100000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "imagePath": "ppt/media/logo.png",
                "mimeType": "image/png",
                "duotone": {
                    "clr1": "000000",
                    "clr2": "4472C4"
                }
            }
        ],
        "elementSources": [],
        "comments": []
    });

    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");
    let vector_slide = compiler.compile(&slide, 9144000, 6858000, &tx).expect("compile");

    assert_eq!(vector_slide.commands.len(), 2);

    // Check warning events
    let warnings: Vec<WarningEvent> = rx.try_iter().collect();

    // 2 blip effects from picture 1, 1 duotone effect from picture 2 -> 3 total
    assert_eq!(warnings.len(), 3);

    for w in &warnings {
        match w {
            WarningEvent::FidelityDegraded {
                element_id,
                feature,
                class,
            } => {
                assert_eq!(*feature, "blip_effect");
                assert_eq!(*class, FidelityClass::Degraded);
                assert!(element_id == "ppt/media/photo.jpg" || element_id == "ppt/media/logo.png");
            }
            _ => panic!("Unexpected warning event: {:?}", w),
        }
    }
}

#[test]
fn test_warning_events_shape_effects_supported() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "elements": [
            {
                "type": "shape",
                "x": 0,
                "y": 0,
                "width": 100000,
                "height": 100000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "roundRect",
                "shadow": {
                    "color": "000000",
                    "alpha": 0.5,
                    "blur": 10000,
                    "dist": 5000,
                    "dir": 45.0
                }
            },
            {
                "type": "shape",
                "x": 150000,
                "y": 150000,
                "width": 100000,
                "height": 100000,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false,
                "geometry": "ellipse"
            }
        ],
        "elementSources": [],
        "comments": []
    });

    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");
    let _ = compiler.compile(&slide, 9144000, 6858000, &tx).expect("compile");

    let warnings: Vec<WarningEvent> = rx.try_iter().collect();

    // Both shapes are fully supported: 2D effects must not fire a fidelity
    // warning anymore (PRD 08 §7.1).
    assert!(warnings.is_empty());
}

struct SinglePixelAssetStore {
    color: [u8; 4],
}

impl AssetStore for SinglePixelAssetStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Ok(DecodedImage {
            width: 1,
            height: 1,
            pixels: Arc::from(self.color.to_vec().into_boxed_slice()),
        })
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

#[test]
fn test_rasterize_with_blip_effects() {
    let mut slide = VectorSlide::new(100, 100);
    // Draw 1x1 image covering entire 100x100 canvas, with Grayscale effect
    slide.commands.push(VectorCommand::DrawImage {
        rel_id: "image.png".to_string(),
        src_rect: NormalizedRect::default(),
        dest_rect: RectF32::new(0.0, 0.0, 100.0, 100.0),
        opacity: 1.0,
        transform: AffineTransform::default(),
        effects: vec![BlipEffect::Grayscale],
    });

    // Asset returns pure red [255, 0, 0, 255]
    let assets = SinglePixelAssetStore {
        color: [255, 0, 0, 255],
    };
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 10,
        height_px: 10,
    };

    let rendered = rasterizer.rasterize(&slide, &target, &assets).expect("rasterize");

    // The pure red pixel should be transformed to BT.709 grayscale luma 54
    let pixels = &rendered.pixels;
    assert_eq!(pixels[0], 54); // R
    assert_eq!(pixels[1], 54); // G
    assert_eq!(pixels[2], 54); // B
    assert_eq!(pixels[3], 255); // A
}
