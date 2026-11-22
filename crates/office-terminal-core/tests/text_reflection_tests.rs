use office_terminal_core::contracts::*;
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use office_terminal_core::svg::{DefaultSlideSvgSerializer, SlideSvgSerializer};

struct DummyAssets;

impl AssetStore for DummyAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, office_terminal_core::error::AssetError> {
        Err(office_terminal_core::error::AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn make_text_slide(refl: Option<VectorReflection>) -> VectorSlide {
    let mut slide = VectorSlide::new(2_000_000, 1_000_000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));

    let font_size_pt = 32.0;
    let sz_emu = font_size_pt * 12700.0;
    let mut glyphs = Vec::new();
    let text = "MIRROR";
    for (i, _) in text.chars().enumerate() {
        glyphs.push(PositionedGlyph {
            glyph_id: i as u16,
            x: 200_000.0 + i as f32 * sz_emu * 0.6,
            y: 300_000.0,
            advance: sz_emu * 0.6,
            warp_u: None,
        });
    }

    slide.commands.push(VectorCommand::DrawText {
        runs: vec![PositionedGlyphRun {
            text: text.to_string(),
            font_family: "DejaVu Sans".to_string(),
            font_size_pt,
            color: [0, 0, 0, 255],
            bold: false,
            italic: false,
            glyphs,
            decorations: TextDecorations::default(),
            single_face: false,
            warp: None,
            reflection: refl,
        }],
        transform: AffineTransform::default(),
    });
    slide
}

#[test]
fn text_reflection_raster_renders_ink_below_baseline() {
    let rasterizer = TinySkiaRasterizer::new();

    let spec = RenderTargetSpec {
        width_px: 400,
        height_px: 200,
    };

    // 1. Without reflection
    let plain_slide = make_text_slide(None);
    let plain_img = rasterizer
        .rasterize(&plain_slide, &spec, &DummyAssets)
        .expect("rasterize plain");

    // 2. With reflection
    let refl = VectorReflection {
        start_opacity: 0.8,
        end_opacity: 0.0,
        start_pos: 0.0,
        end_pos: 1.0,
        distance_emu: 20_000,
        blur_radius_emu: 0,
        ..VectorReflection::default()
    };
    let refl_slide = make_text_slide(Some(refl));
    let refl_img = rasterizer
        .rasterize(&refl_slide, &spec, &DummyAssets)
        .expect("rasterize refl");

    assert_eq!(plain_img.width, 400);
    assert_eq!(plain_img.height, 200);
    assert_eq!(refl_img.width, 400);
    assert_eq!(refl_img.height, 200);

    let mut plain_refl_area_dark_pixels = 0;
    let mut refl_area_dark_pixels = 0;
    for y in 65..200 {
        for x in 0..400 {
            let idx = (y * 400 + x) * 4;
            if plain_img.pixels[idx] < 250 {
                plain_refl_area_dark_pixels += 1;
            }
            if refl_img.pixels[idx] < 250 {
                refl_area_dark_pixels += 1;
            }
        }
    }

    assert_eq!(
        plain_refl_area_dark_pixels, 0,
        "plain text should have no ink in the reflection zone"
    );
    assert!(
        refl_area_dark_pixels > 20,
        "reflected text must have visible ink below the baseline, got {}",
        refl_area_dark_pixels
    );
}

#[test]
fn text_reflection_raster_distance_aware_blur() {
    let rasterizer = TinySkiaRasterizer::new();

    let spec = RenderTargetSpec {
        width_px: 400,
        height_px: 200,
    };

    // Sharp reflection (blur = 0)
    let sharp_refl = VectorReflection {
        start_opacity: 1.0,
        end_opacity: 0.2,
        start_pos: 0.0,
        end_pos: 1.0,
        distance_emu: 0,
        blur_radius_emu: 0,
        ..VectorReflection::default()
    };
    let sharp_slide = make_text_slide(Some(sharp_refl));
    let sharp_img = rasterizer
        .rasterize(&sharp_slide, &spec, &DummyAssets)
        .expect("rasterize sharp");

    // Blurry reflection (blur = 60_000 emu)
    let blur_refl = VectorReflection {
        start_opacity: 1.0,
        end_opacity: 0.2,
        start_pos: 0.0,
        end_pos: 1.0,
        distance_emu: 0,
        blur_radius_emu: 60_000,
        ..VectorReflection::default()
    };
    let blur_slide = make_text_slide(Some(blur_refl));
    let blur_img = rasterizer
        .rasterize(&blur_slide, &spec, &DummyAssets)
        .expect("rasterize blur");

    // The blurred reflection should bleed ink outside the horizontal extents of the sharp reflection
    let mut blur_bleed_count = 0;
    // Check left of text (x between 20 and 38, y between 90 and 160)
    for y in 90..160 {
        for x in 20..38 {
            let idx = (y * 400 + x) * 4;
            let sharp_pixel = sharp_img.pixels[idx];
            let blur_pixel = blur_img.pixels[idx];
            if sharp_pixel == 255 && blur_pixel < 255 {
                blur_bleed_count += 1;
            }
        }
    }
    assert!(
        blur_bleed_count > 0,
        "distance-aware blur must bleed ink past the sharp edge, got {blur_bleed_count}"
    );
}

#[test]
fn text_reflection_svg_export_parses_and_contains_mirror() {
    let serializer = DefaultSlideSvgSerializer::new();
    let refl = VectorReflection {
        start_opacity: 0.7,
        end_opacity: 0.0,
        start_pos: 0.0,
        end_pos: 1.0,
        distance_emu: 25_400,
        blur_radius_emu: 38_100,
        direction_deg: 90.0,
        ..VectorReflection::default()
    };
    let slide = make_text_slide(Some(refl));

    // Test SelectableText mode
    let svg_selectable = serializer
        .serialize_svg(&slide, &DummyAssets, SvgTextMode::SelectableText)
        .expect("svg selectable");
    assert!(
        svg_selectable.contains(r#"<g filter="url(#refl-fx-"#),
        "SVG must contain reflection filter group"
    );
    assert!(
        svg_selectable.contains("refl-text-mask-"),
        "SVG must contain reflection mask"
    );
    assert!(
        svg_selectable.contains("scale("),
        "SVG must contain mirror scale"
    );

    // Verify SVG is well-formed XML and parseable by usvg/resvg
    let opt = resvg::usvg::Options::default();
    resvg::usvg::Tree::from_str(&svg_selectable, &opt).expect("selectable text reflection SVG must parse");

    // Test VectorGlyphs mode
    let svg_vector = serializer
        .serialize_svg(&slide, &DummyAssets, SvgTextMode::VectorGlyphs)
        .expect("svg vector");
    assert!(
        svg_vector.contains(r#"<g filter="url(#refl-fx-"#),
        "Vector SVG must contain reflection filter group"
    );
    resvg::usvg::Tree::from_str(&svg_vector, &opt).expect("vector glyphs reflection SVG must parse");
}
