//! Integration tests for S7a multi-light rigs, material presets, and bevel shading (#15).

use office_terminal_core::contracts::*;
use office_terminal_core::loader::{AssetStore, AssetStoreStats, DecodedImage, SizeHint};
use office_terminal_core::rasterizer::{SlideRasterizer, TinySkiaRasterizer};
use office_terminal_core::scene3d::lighting::{lookup_legacy_rig, lookup_modern_rig, lookup_rig_definition};

struct DummyAssets;

impl AssetStore for DummyAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, office_terminal_core::error::AssetError> {
        Err(office_terminal_core::error::AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn make_beveled_slide(geom_type: &str, rig_name: &str, material: &str) -> VectorSlide {
    let mut slide = VectorSlide::new(1_000_000, 1_000_000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));

    let mut segments = Vec::new();
    if geom_type == "ellipse" {
        let cx = 500_000.0;
        let cy = 500_000.0;
        let r = 300_000.0;
        let k = r * 0.552_284_8;
        segments.push(PathSegment::MoveTo { x: cx, y: cy - r });
        segments.push(PathSegment::CubicTo {
            x1: cx + k,
            y1: cy - r,
            x2: cx + r,
            y2: cy - k,
            x: cx + r,
            y: cy,
        });
        segments.push(PathSegment::CubicTo {
            x1: cx + r,
            y1: cy + k,
            x2: cx + k,
            y2: cy + r,
            x: cx,
            y: cy + r,
        });
        segments.push(PathSegment::CubicTo {
            x1: cx - k,
            y1: cy + r,
            x2: cx - r,
            y2: cy + k,
            x: cx - r,
            y: cy,
        });
        segments.push(PathSegment::CubicTo {
            x1: cx - r,
            y1: cy - k,
            x2: cx - k,
            y2: cy - r,
            x: cx,
            y: cy - r,
        });
        segments.push(PathSegment::Close);
    } else {
        segments.push(PathSegment::MoveTo {
            x: 200_000.0,
            y: 200_000.0,
        });
        segments.push(PathSegment::LineTo {
            x: 800_000.0,
            y: 200_000.0,
        });
        segments.push(PathSegment::LineTo {
            x: 800_000.0,
            y: 800_000.0,
        });
        segments.push(PathSegment::LineTo {
            x: 200_000.0,
            y: 800_000.0,
        });
        segments.push(PathSegment::Close);
    }
    let path = VectorPath::new(segments, FillRule::NonZero);

    let shape_3d = Some(VectorShape3d {
        extrusion_depth_emu: 0,
        extrusion_color: None,
        contour_width_emu: 0,
        contour_color: None,
        prst_material: material.to_string(),
        bevel_top: Some(VectorBevel {
            width_emu: 100_000,
            height_emu: 50_000,
            preset: "circle".to_string(),
        }),
        bevel_bottom: None,
    });
    let scene_3d = Some(VectorScene3d {
        camera_type: "orthographicFront".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: None,
        light_rig_type: rig_name.to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 315.0,
        units_per_inch: 914_400.0,
    });
    let effects = ShapeEffects {
        shape_3d,
        scene_3d,
        ..Default::default()
    };

    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: Some(VectorPaint::Solid([80, 120, 200, 255])),
        stroke: None,
        transform: AffineTransform::default(),
        effects: Box::new(effects),
    });
    slide
}

#[test]
fn test_rig_definitions_count_and_resolution() {
    let modern_names = [
        "threePt",
        "balanced",
        "soft",
        "harsh",
        "flood",
        "contrasting",
        "morning",
        "sunrise",
        "sunset",
        "chilly",
        "freezing",
        "flat",
        "twoPt",
        "glow",
        "brightRoom",
    ];
    assert_eq!(modern_names.len(), 15);
    for name in modern_names {
        assert!(lookup_modern_rig(name).is_some(), "missing modern rig {name}");
        let rig = lookup_rig_definition(name);
        assert!(!rig.lights.is_empty(), "rig {name} should have lights");
    }

    let legacy_names = [
        "legacyFlat1",
        "legacyFlat2",
        "legacyFlat3",
        "legacyFlat4",
        "legacyNormal1",
        "legacyNormal2",
        "legacyNormal3",
        "legacyNormal4",
        "legacyHarsh1",
        "legacyHarsh2",
        "legacyHarsh3",
        "legacyHarsh4",
    ];
    assert_eq!(legacy_names.len(), 12);
    for name in legacy_names {
        assert!(lookup_legacy_rig(name).is_some(), "missing legacy rig {name}");
        let rig = lookup_rig_definition(name);
        assert!(!rig.lights.is_empty(), "rig {name} should have lights");
    }

    // Default fallback
    let fallback = lookup_rig_definition("unknownRig");
    let three_pt = lookup_rig_definition("threePt");
    assert_eq!(fallback.ambient, three_pt.ambient);
    assert_eq!(fallback.lights.len(), three_pt.lights.len());
}

#[test]
fn test_different_rigs_produce_distinguishable_shading() {
    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 200,
        height_px: 200,
    };

    let rigs = ["threePt", "soft", "harsh", "contrasting", "morning"];
    let mut images = Vec::new();
    for rig in rigs {
        let slide = make_beveled_slide("rect", rig, "plastic");
        let img = rasterizer
            .rasterize(&slide, &spec, &DummyAssets)
            .unwrap_or_else(|_| panic!("rasterize {rig}"));
        images.push((rig, img));
    }

    // Compare pixels along the top bevel band (y between 42 and 55, x = 100)
    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            let (name1, img1) = &images[i];
            let (name2, img2) = &images[j];
            let mut diff_count = 0;
            for y in 42..58 {
                let idx = (y * 200 + 100) * 4;
                if img1.pixels[idx..idx + 3] != img2.pixels[idx..idx + 3] {
                    diff_count += 1;
                }
            }
            assert!(
                diff_count > 0,
                "rig {name1} and {name2} must produce distinct shading in the bevel band, got {diff_count} diffs"
            );
        }
    }
}

#[test]
fn test_different_materials_alter_raster() {
    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 200,
        height_px: 200,
    };

    let materials = ["matte", "metal", "clear", "plastic"];
    let mut images = Vec::new();
    for mat in materials {
        let slide = make_beveled_slide("rect", "threePt", mat);
        let img = rasterizer
            .rasterize(&slide, &spec, &DummyAssets)
            .unwrap_or_else(|_| panic!("rasterize {mat}"));
        images.push((mat, img));
    }

    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            let (name1, img1) = &images[i];
            let (name2, img2) = &images[j];
            let mut diff_count = 0;
            for y in 42..58 {
                let idx = (y * 200 + 100) * 4;
                if img1.pixels[idx..idx + 3] != img2.pixels[idx..idx + 3] {
                    diff_count += 1;
                }
            }
            assert!(
                diff_count > 0,
                "material {name1} and {name2} must produce distinct shading in the bevel band, got {diff_count} diffs"
            );
        }
    }
}

#[test]
fn test_rectangle_and_ellipse_bevel_pixel_oracle() {
    let rasterizer = TinySkiaRasterizer::new();
    let spec = RenderTargetSpec {
        width_px: 200,
        height_px: 200,
    };

    // 1. Rectangle
    let rect_slide = make_beveled_slide("rect", "threePt", "plastic");
    let rect_img = rasterizer
        .rasterize(&rect_slide, &spec, &DummyAssets)
        .expect("rasterize rect");
    // Top-left outside is white background
    assert_eq!(&rect_img.pixels[0..4], &[255, 255, 255, 255]);
    // Center is shaded/colored shape
    let center_idx = (100 * 200 + 100) * 4;
    assert_ne!(&rect_img.pixels[center_idx..center_idx + 3], &[255, 255, 255]);

    // 2. Ellipse
    let ellipse_slide = make_beveled_slide("ellipse", "threePt", "plastic");
    let ellipse_img = rasterizer
        .rasterize(&ellipse_slide, &spec, &DummyAssets)
        .expect("rasterize ellipse");
    // Corner is outside ellipse => white
    assert_eq!(&ellipse_img.pixels[0..4], &[255, 255, 255, 255]);
    // Center is inside => colored
    assert_ne!(&ellipse_img.pixels[center_idx..center_idx + 3], &[255, 255, 255]);
}
