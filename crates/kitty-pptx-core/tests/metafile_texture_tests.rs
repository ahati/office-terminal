//! Tests for EMF+ texture brush vector playback and rendering (Issue #12).

use kitty_pptx_core::contracts::{
    AffineTransform, FillRule, PathSegment, RenderTargetSpec, VectorPaint, VectorPath, VectorSlide,
    VectorTextureFill,
};
use kitty_pptx_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use kitty_pptx_core::metafile::engine_metafile_rs::mapping::map_paint;
use kitty_pptx_core::metafile::mapping::paint::map_paint as map_display_paint;
use kitty_pptx_core::metafile::MetafilePaint;
use kitty_pptx_core::rasterizer::SlideRasterizer;
use kitty_pptx_core::{
    AssetError, DefaultSlideSvgSerializer, SlideSvgSerializer, SvgTextMode, TinySkiaRasterizer,
};
use metafile_core as mc;

struct NullStore;

impl AssetStore for NullStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }
    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn sample_bitmap(w: u32, h: u32) -> mc::Bitmap {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            if (x + y) % 2 == 0 {
                rgba.extend_from_slice(&[255, 0, 0, 255]); // red
            } else {
                rgba.extend_from_slice(&[0, 0, 255, 255]); // blue
            }
        }
    }
    mc::Bitmap {
        width: w,
        height: h,
        rgba,
    }
}

#[test]
fn texture_brush_tile_mode_maps_to_vector_texture() {
    let bitmap = sample_bitmap(4, 4);
    let transform = mc::Transform {
        m11: 2.0,
        m12: 0.0,
        m21: 0.0,
        m22: 3.0,
        dx: 10.0,
        dy: 20.0,
    };
    let (paint, tag) = map_paint(&mc::Paint::Texture {
        bitmap,
        transform,
        wrap_mode: 0, // Tile
        opacity: 0.9,
        do_not_transform: false,
    });
    assert_eq!(tag, None, "supported tile mode emits no degraded warning");
    let metafile_paint = paint.expect("must produce MetafilePaint::Texture");
    assert!(matches!(metafile_paint, MetafilePaint::Texture { repeat: true, .. }));

    let unit_to_emu = AffineTransform::scale(10.0, 10.0);
    let vector_paint = map_display_paint(&metafile_paint, &unit_to_emu)
        .expect("must map to VectorPaint::Texture");
    match vector_paint {
        VectorPaint::Texture(tex) => {
            assert_eq!(tex.width, 4);
            assert_eq!(tex.height, 4);
            assert!(tex.repeat);
            assert!((tex.opacity - 0.9).abs() < 1e-4);
            assert_eq!(tex.transform.sx, 20.0);
            assert_eq!(tex.transform.sy, 30.0);
            assert_eq!(tex.transform.tx, 100.0);
            assert_eq!(tex.transform.ty, 200.0);
        }
        other => panic!("expected VectorPaint::Texture, got {other:?}"),
    }
}

#[test]
fn texture_brush_clamp_mode_maps_to_clamp_texture() {
    let bitmap = sample_bitmap(2, 2);
    let (paint, tag) = map_paint(&mc::Paint::Texture {
        bitmap,
        transform: mc::Transform::IDENTITY,
        wrap_mode: 4, // Clamp
        opacity: 1.0,
        do_not_transform: false,
    });
    assert_eq!(tag, None);
    let metafile_paint = paint.expect("must produce MetafilePaint::Texture");
    assert!(matches!(metafile_paint, MetafilePaint::Texture { repeat: false, .. }));
}

#[test]
fn texture_brush_unsupported_wrap_falls_back_to_average() {
    let bitmap = sample_bitmap(2, 2);
    let (paint, tag) = map_paint(&mc::Paint::Texture {
        bitmap,
        transform: mc::Transform::IDENTITY,
        wrap_mode: 1, // TileFlipX
        opacity: 1.0,
        do_not_transform: false,
    });
    assert_eq!(tag, Some("metafile_pattern_brush"));
    assert!(matches!(paint, Some(MetafilePaint::Solid(_))));
}

#[test]
fn texture_brush_rasterizes_without_panics() {
    let png = kitty_pptx_core::metafile::mapping::bitmap::encode_raw_png(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255],
    )
    .expect("png encoding failed");

    let fill = VectorPaint::Texture(Box::new(VectorTextureFill {
        png,
        width: 2,
        height: 2,
        transform: AffineTransform::scale(100.0, 100.0),
        repeat: true,
        opacity: 1.0,
    }));

    let rect = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 500.0, y: 0.0 },
            PathSegment::LineTo { x: 500.0, y: 500.0 },
            PathSegment::LineTo { x: 0.0, y: 500.0 },
            PathSegment::Close,
        ],
        fill_rule: FillRule::NonZero,
    };

    let slide = VectorSlide {
        width_emu: 1000,
        height_emu: 1000,
        background: None,
        commands: vec![kitty_pptx_core::contracts::VectorCommand::DrawPath {
            path: rect,
            fill: Some(fill),
            stroke: None,
            transform: AffineTransform::identity(),
            effects: Box::new(kitty_pptx_core::contracts::ShapeEffects::default()),
        }],
        hyperlinks: Vec::new(),
    };

    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let rgba = rasterizer
        .rasterize(&slide, &spec, &NullStore)
        .expect("rasterization failed");
    assert_eq!(rgba.width, 100);
    assert_eq!(rgba.height, 100);
    // Ensure some non-zero pixels were drawn
    assert!(rgba.pixels.iter().any(|&b| b > 0));
}

#[test]
fn texture_brush_serializes_to_svg_pattern() {
    let png = kitty_pptx_core::metafile::mapping::bitmap::encode_raw_png(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255],
    )
    .expect("png encoding failed");

    let fill = VectorPaint::Texture(Box::new(VectorTextureFill {
        png,
        width: 2,
        height: 2,
        transform: AffineTransform::scale(10.0, 10.0),
        repeat: true,
        opacity: 1.0,
    }));

    let rect = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 50.0 },
            PathSegment::LineTo { x: 0.0, y: 50.0 },
            PathSegment::Close,
        ],
        fill_rule: FillRule::NonZero,
    };

    let slide = VectorSlide {
        width_emu: 100,
        height_emu: 100,
        background: None,
        commands: vec![kitty_pptx_core::contracts::VectorCommand::DrawPath {
            path: rect,
            fill: Some(fill),
            stroke: None,
            transform: AffineTransform::identity(),
            effects: Box::new(kitty_pptx_core::contracts::ShapeEffects::default()),
        }],
        hyperlinks: Vec::new(),
    };

    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&slide, &NullStore, SvgTextMode::SelectableText)
        .expect("SVG serialization failed");

    assert!(svg.contains("<pattern"), "SVG should contain pattern definition");
    assert!(svg.contains("data:image/png;base64,"), "Pattern should contain base64 image");
    assert!(svg.contains(r#"fill="url(#texfill_1)""#), "Path should reference pattern id");
}
