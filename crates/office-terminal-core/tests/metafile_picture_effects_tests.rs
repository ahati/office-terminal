//! Tests for picture-level blip effects and opacity on metafile expansion (Issue #7).

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{
    AffineTransform, BlipEffect, NormalizedRect, RectF32, RenderTargetSpec, VectorCommand, VectorPaint, VectorPath,
    VectorSlide,
};
use office_terminal_core::loader::{DefaultAssetStore, DefaultPresentationLoader, PresentationLoader, Source};
use office_terminal_core::metafile::effects::apply_metafile_effects;
use office_terminal_core::metafile::expand_metafiles;
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use office_terminal_core::{DefaultSlideCompiler, SlideCompiler};
use std::path::Path;
use std::sync::Arc;

#[test]
fn color_change_white_to_transparent_remaps_fill_and_preserves_background() {
    let mut scene = VectorSlide::new(200, 200);
    // Slide background: solid black
    scene.background = Some(office_terminal_core::contracts::VectorBackground::Solid([0, 0, 0, 255]));

    let rect_path = VectorPath::new(
        vec![
            office_terminal_core::contracts::PathSegment::MoveTo { x: 0.0, y: 0.0 },
            office_terminal_core::contracts::PathSegment::LineTo { x: 200.0, y: 0.0 },
            office_terminal_core::contracts::PathSegment::LineTo { x: 200.0, y: 200.0 },
            office_terminal_core::contracts::PathSegment::LineTo { x: 0.0, y: 200.0 },
            office_terminal_core::contracts::PathSegment::Close,
        ],
        office_terminal_core::contracts::FillRule::NonZero,
    );

    // An expanded path from an EMF that originally had a solid white background
    let mut emf_path = VectorCommand::DrawPath {
        path: rect_path,
        fill: Some(VectorPaint::Solid([255, 255, 255, 255])),
        stroke: None,
        transform: AffineTransform::identity(),
        effects: Box::default(),
    };

    // Picture carries <a:clrChange> from white to transparent
    let effects = vec![BlipEffect::ColorChange {
        from: "FFFFFF".to_string(),
        from_alpha: 1.0,
        to: "FFFFFF".to_string(),
        to_alpha: 0.0,
        use_alpha: false,
    }];

    apply_metafile_effects(&mut emf_path, &effects, 1.0);

    // Verify fill alpha was transformed to 0
    match &emf_path {
        VectorCommand::DrawPath { fill, .. } => {
            assert_eq!(
                fill,
                &Some(VectorPaint::Solid([255, 255, 255, 0])),
                "white fill must become transparent"
            );
        }
        _ => panic!("expected DrawPath"),
    }

    scene.commands.push(emf_path);

    // Rasterize and assert black background is preserved (not wiped out to white)
    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 20,
        height_px: 20,
    };
    let empty_store = DefaultAssetStore::new(Arc::from(Vec::new()), None);
    let rendered = rasterizer
        .rasterize(&scene, &spec, &empty_store)
        .expect("rasterize should succeed");

    // All pixels should remain black
    for chunk in rendered.pixels.as_chunks::<4>().0 {
        assert_eq!(
            chunk,
            &[0, 0, 0, 255],
            "transparent EMF background must not paint opaque white over slide"
        );
    }
}

#[test]
fn marker_opacity_scales_expanded_vector_paints() {
    let mut cmd = VectorCommand::DrawPath {
        path: VectorPath::empty(),
        fill: Some(VectorPaint::Solid([100, 150, 200, 255])),
        stroke: None,
        transform: AffineTransform::identity(),
        effects: Box::default(),
    };

    apply_metafile_effects(&mut cmd, &[], 0.5);

    match cmd {
        VectorCommand::DrawPath { fill, .. } => {
            assert_eq!(
                fill,
                Some(VectorPaint::Solid([100, 150, 200, 128])),
                "marker opacity must scale vector fill alpha"
            );
        }
        _ => panic!("expected DrawPath"),
    }
}

#[test]
fn metafile_marker_carries_effects_through_expansion() {
    let store = DefaultAssetStore::new(Arc::from(Vec::new()), None);

    let mut scene = VectorSlide::new(100, 100);
    scene.commands.push(VectorCommand::DrawMetafile {
        rel_id: "ppt/media/test.emf".to_string(),
        transform: AffineTransform::identity(),
        src_rect: NormalizedRect::default(),
        dest_rect: RectF32 {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        },
        opacity: 0.8,
        effects: vec![BlipEffect::ColorChange {
            from: "FFFFFF".to_string(),
            from_alpha: 1.0,
            to: "FFFFFF".to_string(),
            to_alpha: 0.0,
            use_alpha: false,
        }],
    });

    let (tx, _rx) = unbounded();
    // Expanding empty bytes falls back to placeholder or empty, but must not panic
    let summary = expand_metafiles(&mut scene, &store, &tx);
    assert_eq!(summary.expanded, 0);
    assert_eq!(summary.placeholders, 1);
}

#[test]
fn tdf113163_regression_test() {
    let fixture_path = Path::new("target/external/fixtures/2ddb63be403dcead26b52f657ba869d28785c038/tdf113163.pptx");
    if !fixture_path.exists() {
        return;
    }

    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(fixture_path.to_path_buf()))
        .expect("load tdf113163");

    let slide = &presentation.slides[0];
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();
    let mut vector_slide = compiler
        .compile_in_deck(
            slide,
            &presentation.slides,
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        )
        .expect("compile tdf113163 slide");

    // Before expansion: contains DrawMetafile carrying ColorChange
    let has_metafile_with_color_change = vector_slide.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawMetafile { effects, .. } => effects.iter().any(|e| {
            matches!(
                e,
                BlipEffect::ColorChange {
                    from,
                    to_alpha,
                    ..
                } if from == "FFFFFF" && *to_alpha == 0.0
            )
        }),
        _ => false,
    });
    assert!(
        has_metafile_with_color_change,
        "tdf113163 DrawMetafile marker must carry ColorChange effect"
    );

    // Expand metafiles
    let summary = expand_metafiles(&mut vector_slide, assets.as_ref(), &tx);
    assert_eq!(summary.expanded, 1);
    assert_eq!(summary.placeholders, 0);

    // After expansion: NO opaque white DrawPath may exist
    for cmd in &vector_slide.commands {
        if let VectorCommand::DrawPath {
            fill: Some(VectorPaint::Solid(color)),
            ..
        } = cmd
        {
            assert!(
                *color != [255, 255, 255, 255],
                "tdf113163 must not contain opaque white DrawPath after expansion"
            );
        }
    }

    // Rasterize slide: must be black, not white
    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 400,
        height_px: 300,
    };
    let rendered = rasterizer
        .rasterize(&vector_slide, &spec, assets.as_ref())
        .expect("rasterize tdf113163");

    // Sample top-left and center pixels: they must be black
    let tl = &rendered.pixels[0..4];
    assert_eq!(tl, &[0, 0, 0, 255], "slide background must be black");
    let mid_idx = (150 * 400 + 200) * 4;
    let mid = &rendered.pixels[mid_idx..mid_idx + 4];
    assert_eq!(mid, &[0, 0, 0, 255], "center of slide must be black");
}
