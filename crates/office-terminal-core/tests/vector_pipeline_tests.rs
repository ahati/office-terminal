use office_terminal_core::*;
use std::sync::Arc;

struct EmptyAssetStore;
impl AssetStore for EmptyAssetStore {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("no asset in mock store".to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

#[test]
fn test_vector_slide_rasterization_and_svg_export() {
    let mut slide = VectorSlide {
        width_emu: 914400,
        height_emu: 685800,
        background: Some(VectorBackground::Solid([240, 240, 240, 255])),
        commands: Vec::new(),
        hyperlinks: Vec::new(),
    };

    slide.commands.push(VectorCommand::DrawPath {
        path: VectorPath {
            segments: vec![
                PathSegment::MoveTo {
                    x: 100000.0,
                    y: 100000.0,
                },
                PathSegment::LineTo {
                    x: 300000.0,
                    y: 100000.0,
                },
                PathSegment::LineTo {
                    x: 300000.0,
                    y: 250000.0,
                },
                PathSegment::LineTo {
                    x: 100000.0,
                    y: 250000.0,
                },
                PathSegment::Close,
            ],
            fill_rule: FillRule::NonZero,
        },
        fill: Some(VectorPaint::Solid([20, 40, 200, 255])),
        stroke: Some(VectorStroke {
            color: [10, 10, 10, 255],
            width_emu: 5000,
            dash_segments: None,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            paint: None,
            ..VectorStroke::default()
        }),
        transform: AffineTransform::default(),
        effects: Box::default(),
    });

    // Test Rasterization
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 800,
        height_px: 600,
    };
    let empty_store = Arc::new(EmptyAssetStore);
    let rendered = rasterizer
        .rasterize(&slide, &target, empty_store.as_ref())
        .expect("rasterization failed");
    assert_eq!(rendered.width, 800);
    assert_eq!(rendered.height, 600);
    assert_eq!(rendered.pixels.len(), 800 * 600 * 4);

    // Verify SVG Export
    let svg_serializer = DefaultSlideSvgSerializer::new();
    let svg_text = svg_serializer
        .serialize_svg(&slide, empty_store.as_ref(), SvgTextMode::VectorGlyphs)
        .expect("svg serialization failed");
    assert!(svg_text.contains("<svg"));
    assert!(svg_text.contains("viewBox=\"0 0 914400 685800\""));
    assert!(svg_text.contains("</svg>"));
    assert!(svg_text.contains("d=\"M 100000 100000 L 300000 100000 L 300000 250000 L 100000 250000 Z\""));
}
