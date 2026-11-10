use crossbeam_channel::unbounded;
use kitty_pptx_core::*;
use pptx_model::Slide;

fn slide(index: usize, part: &str, hyperlink: Option<&str>, action: Option<&str>) -> Slide {
    let mut shape = serde_json::json!({
        "type": "shape",
        "x": 100000,
        "y": 200000,
        "width": 400000,
        "height": 300000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect"
    });
    if let Some(link) = hyperlink {
        shape["hyperlink"] = serde_json::Value::String(link.to_string());
    }
    if let Some(action) = action {
        shape["hyperlinkAction"] = serde_json::Value::String(action.to_string());
    }
    serde_json::from_value(serde_json::json!({
        "index": index,
        "slideNumber": index + 1,
        "partName": part,
        "elements": [shape],
        "elementSources": [],
        "comments": []
    }))
    .expect("slide JSON")
}

#[test]
fn compiler_in_deck_populates_external_and_internal_links() {
    let deck = vec![
        slide(
            0,
            "ppt/slides/slide1.xml",
            Some("https://www.rust-lang.org"),
            None,
        ),
        slide(1, "ppt/slides/slide2.xml", None, None),
        slide(
            2,
            "ppt/slides/slide3.xml",
            Some("../slides/slide3.xml"),
            Some("ppaction://hlinksldjump"),
        ),
    ];
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();
    let compiled = compiler
        .compile_in_deck(&deck[0], &deck, 9_144_000, 6_858_000, &tx)
        .expect("compile slide 1");
    assert_eq!(compiled.hyperlinks.len(), 1);
    assert_eq!(
        compiled.hyperlinks[0].target,
        HyperlinkTarget::ExternalUri("https://www.rust-lang.org".into())
    );

    let compiled = compiler
        .compile_in_deck(&deck[2], &deck, 9_144_000, 6_858_000, &tx)
        .expect("compile slide 3");
    assert_eq!(compiled.hyperlinks.len(), 1);
    assert_eq!(
        compiled.hyperlinks[0].target,
        HyperlinkTarget::SlideIndex(2)
    );
}

#[test]
fn svg_wraps_linked_path_in_external_anchor() {
    let mut vector = VectorSlide::new(1000, 1000);
    vector.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 100.0 },
                PathSegment::LineTo { x: 0.0, y: 100.0 },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: Some(VectorPaint::Solid([10, 20, 30, 255])),
        stroke: None,
        transform: AffineTransform::translate(100.0, 100.0),
        effects: Box::default(),
    });
    vector.hyperlinks.push(SlideLink::new(
        HyperlinkTarget::ExternalUri("https://example.com/doc".into()),
        RectF32::new(100.0, 100.0, 100.0, 100.0),
        Some("Click to visit documentation".into()),
    ));

    struct EmptyAssets;
    impl AssetStore for EmptyAssets {
        fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
            Err(AssetError::NotFound("none".into()))
        }
        fn stats(&self) -> AssetStoreStats {
            AssetStoreStats {
                cached_entries: 0,
                cached_bytes: 0,
            }
        }
    }

    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(
            &vector,
            &EmptyAssets,
            SvgTextMode::SelectableText,
        )
        .expect("serialize");
    assert!(svg.contains("xmlns:xlink=\"http://www.w3.org/1999/xlink\""));
    assert!(svg.contains(
        "<a xlink:href=\"https://example.com/doc\" href=\"https://example.com/doc\" target=\"_blank\" rel=\"noopener noreferrer\">"
    ));
    assert!(svg.contains("<title>Click to visit documentation</title>"));
    assert!(svg.contains("</a>"));

    // No links means the legacy SVG output, without the xlink namespace.
    let plain = VectorSlide::new(1000, 1000);
    let plain_svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&plain, &EmptyAssets, SvgTextMode::SelectableText)
        .expect("serialize");
    assert!(!plain_svg.contains("xmlns:xlink"));
}

#[test]
fn svg_internal_jump_uses_relative_anchor_fragment() {
    let mut vector = VectorSlide::new(1000, 1000);
    vector.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 50.0, y: 50.0 },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: None,
        stroke: None,
        transform: AffineTransform::default(),
        effects: Box::default(),
    });
    vector.hyperlinks.push(SlideLink::new(
        HyperlinkTarget::SlideIndex(4),
        RectF32::new(0.0, 0.0, 50.0, 50.0),
        None,
    ));

    struct EmptyAssets;
    impl AssetStore for EmptyAssets {
        fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
            Err(AssetError::NotFound("none".into()))
        }
        fn stats(&self) -> AssetStoreStats {
            AssetStoreStats {
                cached_entries: 0,
                cached_bytes: 0,
            }
        }
    }

    let svg = DefaultSlideSvgSerializer::new()
        .serialize_svg(&vector, &EmptyAssets, SvgTextMode::SelectableText)
        .expect("serialize");
    assert!(svg.contains("xlink:href=\"#slide-5\""));
    assert!(svg.contains("target=\"_self\""));
}
