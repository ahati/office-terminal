//! Tests for S7b: 3D solid compositor (rear-plane, bottom bevel, side-wall extrusion sweep).

use office_terminal_core::*;

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn make_3d_slide(
    scene_3d: Option<VectorScene3d>,
    shape_3d: Option<VectorShape3d>,
    stroke: Option<VectorStroke>,
) -> VectorSlide {
    let mut slide = VectorSlide::new(1_000_000, 1_000_000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    slide.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo {
                    x: 200_000.0,
                    y: 200_000.0,
                },
                PathSegment::LineTo {
                    x: 600_000.0,
                    y: 200_000.0,
                },
                PathSegment::LineTo {
                    x: 600_000.0,
                    y: 600_000.0,
                },
                PathSegment::LineTo {
                    x: 200_000.0,
                    y: 600_000.0,
                },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])), // blue
        stroke,
        transform: AffineTransform::default(),
        effects: Box::new(ShapeEffects {
            scene_3d,
            shape_3d,
            ..Default::default()
        }),
    });
    slide
}

#[test]
fn nonzero_extrusion_produces_visible_side_walls_when_tilted() {
    let scene = VectorScene3d {
        camera_type: "isometricLeftUp".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: None,
        light_rig_type: "threePt".to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 315.0,
        units_per_inch: 914_400.0,
    };
    let shape = VectorShape3d {
        extrusion_depth_emu: 300_000,
        extrusion_color: Some([0xE0, 0x30, 0x30, 255]), // distinct red
        contour_width_emu: 0,
        contour_color: None,
        prst_material: "plastic".to_string(),
        bevel_top: None,
        bevel_bottom: None,
    };
    let slide = make_3d_slide(Some(scene), Some(shape), None);
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let px = |x: usize, y: usize| {
        let base = (y * 100 + x) * 4;
        [
            rendered.pixels[base],
            rendered.pixels[base + 1],
            rendered.pixels[base + 2],
            rendered.pixels[base + 3],
        ]
    };

    // Body front face is projected blue
    let front = px(40, 40);
    assert_eq!(front[3], 255);
    assert!(
        front[0] < 100 && front[2] > 150,
        "front body face expected blue, got {front:?}"
    );

    // Side wall extruded pixels: red extrusion color must appear in the rendered image
    let mut red_count = 0;
    for y in 0..100 {
        for x in 0..100 {
            let p = px(x, y);
            if p[0] > 180 && p[1] < 100 && p[2] < 100 && p[3] == 255 {
                red_count += 1;
            }
        }
    }
    assert!(
        red_count > 50,
        "extrusion side walls must be visible with red pixels, found {red_count}"
    );
}

#[test]
fn face_on_camera_hides_extrusion_behind_front_body() {
    let scene = VectorScene3d {
        camera_type: "orthographicFront".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: None,
        light_rig_type: "threePt".to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 315.0,
        units_per_inch: 914_400.0,
    };
    let shape = VectorShape3d {
        extrusion_depth_emu: 300_000,
        extrusion_color: Some([0xE0, 0x30, 0x30, 255]),
        contour_width_emu: 0,
        contour_color: None,
        prst_material: "plastic".to_string(),
        bevel_top: None,
        bevel_bottom: None,
    };
    let slide = make_3d_slide(Some(scene), Some(shape), None);
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let px = |x: usize, y: usize| {
        let base = (y * 100 + x) * 4;
        [
            rendered.pixels[base],
            rendered.pixels[base + 1],
            rendered.pixels[base + 2],
            rendered.pixels[base + 3],
        ]
    };

    // Body front face is blue
    let front = px(40, 40);
    assert_eq!(front[3], 255);
    assert_eq!(front, [0x44, 0x72, 0xC4, 255]);

    // When face-on, extrusion is occluded behind the front body: no red pixels should be visible
    let mut red_count = 0;
    for y in 0..100 {
        for x in 0..100 {
            let p = px(x, y);
            if p[0] > 180 && p[1] < 100 && p[2] < 100 {
                red_count += 1;
            }
        }
    }
    assert_eq!(
        red_count, 0,
        "extrusion must be completely hidden behind front body when face-on"
    );
}

#[test]
fn bottom_bevel_shaded_when_camera_views_rear() {
    let scene = VectorScene3d {
        camera_type: "orthographicFront".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: Some(VectorRot3d {
            lat_deg: 0.0,
            lon_deg: 180.0, // looking at back face
            rev_deg: 0.0,
        }),
        light_rig_type: "threePt".to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 315.0,
        units_per_inch: 914_400.0,
    };
    let shape = VectorShape3d {
        extrusion_depth_emu: 200_000,
        extrusion_color: None,
        contour_width_emu: 0,
        contour_color: None,
        prst_material: "plastic".to_string(),
        bevel_top: None,
        bevel_bottom: Some(VectorBevel {
            width_emu: 100_000,
            height_emu: 50_000,
            preset: "circle".to_string(),
        }),
    };
    let slide = make_3d_slide(Some(scene), Some(shape), None);
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let px = |x: usize, y: usize| {
        let base = (y * 100 + x) * 4;
        [
            rendered.pixels[base],
            rendered.pixels[base + 1],
            rendered.pixels[base + 2],
            rendered.pixels[base + 3],
        ]
    };

    // Body is visible from the rear
    let center = px(40, 40);
    assert_eq!(center[3], 255);

    // Bevel shading modifies pixel colors near the border
    let edge = px(22, 22);
    assert_ne!(edge, [255, 255, 255, 255], "rear bevel edge should be rendered");
}

#[test]
fn legacy_wireframe_depth_renders_front_and_rear_rims_no_fill() {
    let scene = VectorScene3d {
        camera_type: "isometricOffAxis1Left".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: None,
        light_rig_type: "threePt".to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 315.0,
        units_per_inch: 914_400.0,
    };
    let shape = VectorShape3d {
        extrusion_depth_emu: 200_000,
        extrusion_color: None,
        contour_width_emu: 0,
        contour_color: None,
        prst_material: "legacyWireframe".to_string(),
        bevel_top: None,
        bevel_bottom: None,
    };
    let stroke = Some(VectorStroke {
        color: [0, 0, 0, 255],
        width_emu: 12_700,
        dash_segments: None,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        paint: None,
        ..VectorStroke::default()
    });
    let slide = make_3d_slide(Some(scene), Some(shape), stroke);
    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec {
        width_px: 100,
        height_px: 100,
    };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let px = |x: usize, y: usize| {
        let base = (y * 100 + x) * 4;
        [
            rendered.pixels[base],
            rendered.pixels[base + 1],
            rendered.pixels[base + 2],
            rendered.pixels[base + 3],
        ]
    };

    // Interior has no solid fill - remains background white
    let interior = px(40, 40);
    assert_eq!(
        interior,
        [255, 255, 255, 255],
        "legacyWireframe interior must have no fill"
    );

    // Rim wires are painted black
    let mut dark_wire_count = 0;
    for y in 0..100 {
        for x in 0..100 {
            let p = px(x, y);
            if p[0] < 50 && p[1] < 50 && p[2] < 50 && p[3] == 255 {
                dark_wire_count += 1;
            }
        }
    }
    assert!(
        dark_wire_count > 20,
        "rim wires must be rendered, found {dark_wire_count}"
    );
}
