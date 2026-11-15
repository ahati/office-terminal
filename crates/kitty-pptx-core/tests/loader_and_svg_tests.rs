use std::sync::Arc;
use kitty_pptx_core::*;

struct MockAssetStore {
    sample_png: Vec<u8>,
}

impl MockAssetStore {
    fn new() -> Self {
        // Generate a 2x2 solid red PNG in memory
        let img = image::RgbaImage::from_fn(2, 2, |_, _| image::Rgba([255, 0, 0, 255]));
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        Self { sample_png: png_bytes }
    }
}

impl AssetStore for MockAssetStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        if rel_id == "image1.png" {
            let img = image::load_from_memory(&self.sample_png).unwrap().to_rgba8();
            Ok(DecodedImage {
                width: 2,
                height: 2,
                pixels: Arc::from(img.into_raw().into_boxed_slice()),
            })
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError> {
        if rel_id == "image1.png" {
            Ok(self.sample_png.clone())
        } else {
            Err(AssetError::NotFound(rel_id.to_string()))
        }
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 1,
            cached_bytes: 16,
        }
    }
}

#[test]
fn test_svg_export_modes_and_image_embedding() {
    let mock_assets = Arc::new(MockAssetStore::new());

    let mut slide = VectorSlide::new(9144000, 6858000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));

    // Add an image command
    slide.commands.push(VectorCommand::DrawImage {
        rel_id: "image1.png".to_string(),
        src_rect: NormalizedRect::default(),
        dest_rect: RectF32::new(100000.0, 100000.0, 500000.0, 500000.0),
        opacity: 0.8,
        transform: AffineTransform::default(),
        effects: Vec::new(),
    });

    // Add a text command
    slide.commands.push(VectorCommand::DrawText {
        runs: vec![
            PositionedGlyphRun {
                text: "Hello Vector World".to_string(),
                font_family: "DejaVu Sans".to_string(),
                font_size_pt: 20.0,
                color: [10, 20, 30, 255],
                bold: false,
                italic: false,
                glyphs: vec![
                    PositionedGlyph {
                        glyph_id: 0,
                        x: 200000.0,
                        y: 300000.0,
                        advance: 50000.0,
                        warp_u: None,
                    },
                ],
                decorations: kitty_pptx_core::contracts::TextDecorations {
                    underline: Some(kitty_pptx_core::contracts::DecorationLine {
                        color: [10, 20, 30, 255],
                        top_emu: -152_400.0,
                        thickness_emu: 12_700.0,
                        gap_emu: 0.0,
                        style: kitty_pptx_core::contracts::DecorationStyle::Solid,
                    }),
                    strike: None,
                },
                warp: None,
            },
        ],
        transform: AffineTransform::default(),
    });

    let serializer = DefaultSlideSvgSerializer::new();

    // 1. SelectableText Mode
    let svg_selectable = serializer
        .serialize_svg(&slide, mock_assets.as_ref(), SvgTextMode::SelectableText)
        .expect("svg serialization failed");

    assert!(svg_selectable.contains("<image href=\"data:image/png;base64,"));
    assert!(svg_selectable.contains("<text "));
    assert!(svg_selectable.contains("Hello Vector World"));
    assert!(svg_selectable.contains("text-decoration=\"underline\""));

    // 2. VectorGlyphs Mode
    let svg_glyphs = serializer
        .serialize_svg(&slide, mock_assets.as_ref(), SvgTextMode::VectorGlyphs)
        .expect("svg serialization failed");

    assert!(svg_glyphs.contains("<image href=\"data:image/png;base64,"));
    // In vector glyphs mode it either emits exact <path> glyphs or falls back to <text>
    assert!(svg_glyphs.contains("<path") || svg_glyphs.contains("<text"));

    // 3. Test Rasterizer on the same slide
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 640, height_px: 480 };
    let rendered = rasterizer
        .rasterize(&slide, &target, mock_assets.as_ref())
        .expect("rasterization failed");

    assert_eq!(rendered.width, 640);
    assert_eq!(rendered.height, 480);
    assert_eq!(rendered.pixels.len(), 640 * 480 * 4);
}
