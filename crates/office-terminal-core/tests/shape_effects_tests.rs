use crossbeam_channel::unbounded;
use office_terminal_core::*;
use pptx_model::Slide;

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound(rel_id.to_string()))
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats::default()
    }
}

fn full_effect_shape() -> serde_json::Value {
    serde_json::json!({
        "type": "shape",
        "x": 100000,
        "y": 100000,
        "width": 1000000,
        "height": 500000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "roundRect",
        "fill": { "fillType": "solid", "color": "4472C4" },
        "shadow": {
            "color": "000000",
            "alpha": 0.4,
            "blur": 38100,
            "dist": 25400,
            "dir": 45.0,
            "sx": 1.1,
            "sy": 0.9,
            "kx": 5.0,
            "ky": 2.0,
            "rotWithShape": true
        },
        "innerShadow": {
            "color": "112233",
            "alpha": 0.6,
            "blur": 12700,
            "dist": 6350,
            "dir": 270.0
        },
        "glow": { "color": "4F81BD", "alpha": 0.75, "radius": 38100 },
        "softEdge": { "radius": 12700 },
        "reflection": {
            "blur": 12700,
            "dist": 19050,
            "dir": 90.0,
            "stA": 0.5,
            "stPos": 0.0,
            "endA": 0.0,
            "endPos": 1.0,
            "sx": 1.0,
            "sy": -1.0
        },
        "scene3d": {
            "camera": { "prst": "perspectiveRelaxed", "fov": 45.0, "zoom": 1.0 },
            "lightRig": { "rig": "threePt", "dir": "tl" }
        },
        "sp3d": {
            "z": 0,
            "extrusionH": 12700,
            "contourW": 6350,
            "contourClr": "969696",
            "extrusionClr": "7F7F7F",
            "prstMaterial": "plastic",
            "bevelT": { "w": 25400, "h": 12700, "prst": "circle" }
        }
    })
}

fn compile_elements(elements: Vec<serde_json::Value>) -> (VectorSlide, Vec<WarningEvent>) {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();
    let slide_json = serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "background": { "fillType": "solid", "color": "FFFFFF" },
        "elements": elements,
        "elementSources": [],
        "comments": []
    });
    let slide: Slide = serde_json::from_value(slide_json).expect("deserialize slide");
    let vector_slide = compiler.compile(&slide, 2000000, 1000000, &tx).expect("compile");
    (vector_slide, rx.try_iter().collect())
}

fn draw_path_effects(slide: &VectorSlide) -> &ShapeEffects {
    for cmd in &slide.commands {
        if let VectorCommand::DrawPath { effects, .. } = cmd {
            return effects;
        }
    }
    panic!("expected a DrawPath command");
}

#[test]
fn extraction_maps_every_effect_into_the_ir() {
    let (slide, warnings) = compile_elements(vec![full_effect_shape()]);
    let effects = draw_path_effects(&slide);

    let shadow = effects.outer_shadow.as_ref().expect("outer shadow");
    assert_eq!(shadow.color, [0, 0, 0, 102]);
    assert_eq!(shadow.blur_radius_emu, 38100);
    assert_eq!(shadow.distance_emu, 25400);
    assert_eq!(shadow.direction_deg, 45.0);
    assert!((shadow.scale_x - 1.1).abs() < 1e-4);
    assert!((shadow.scale_y - 0.9).abs() < 1e-4);
    assert!((shadow.skew_x_deg - 5.0).abs() < 1e-4);
    assert!((shadow.skew_y_deg - 2.0).abs() < 1e-4);
    assert!(shadow.rotate_with_shape);

    let inner = effects.inner_shadow.as_ref().expect("inner shadow");
    assert_eq!(inner.direction_deg, 270.0);
    assert_eq!(inner.color, [0x11, 0x22, 0x33, 153]);

    let glow = effects.glow.as_ref().expect("glow");
    assert_eq!(glow.radius_emu, 38100);
    assert!((glow.color[3] as i32 - 191).abs() <= 1);

    assert_eq!(effects.soft_edge.as_ref().expect("soft edge").radius_emu, 12700);

    let reflection = effects.reflection.as_ref().expect("reflection");
    assert_eq!(reflection.blur_radius_emu, 12700);
    assert_eq!(reflection.distance_emu, 19050);
    assert!((reflection.start_opacity - 0.5).abs() < 1e-4);
    assert!((reflection.end_opacity - 0.0).abs() < 1e-4);

    let scene = effects.scene_3d.as_ref().expect("scene3d");
    assert_eq!(scene.camera_type, "perspectiveRelaxed");
    assert!((scene.fov_deg - 45.0).abs() < 1e-4);
    assert_eq!(scene.light_rig_type, "threePt");
    assert_eq!(scene.light_dir, "tl");
    assert!(scene.light_rot.is_none());
    assert!((scene.light_direction_deg - 225.0).abs() < 1e-4);
    assert!((scene.zoom - 1.0).abs() < 1e-6);
    assert!(scene.rot.is_none());
    assert!((scene.units_per_inch - 914_400.0).abs() < 1e-4);

    let shape_3d = effects.shape_3d.as_ref().expect("sp3d");
    assert_eq!(shape_3d.extrusion_depth_emu, 12700);
    assert_eq!(shape_3d.extrusion_color, Some([0x7F, 0x7F, 0x7F, 255]));
    assert_eq!(shape_3d.contour_width_emu, 6350);
    assert_eq!(shape_3d.contour_color, Some([0x96, 0x96, 0x96, 255]));
    let bevel_top = shape_3d.bevel_top.as_ref().expect("bevelT");
    assert_eq!(bevel_top.width_emu, 25400);
    assert_eq!(bevel_top.height_emu, 12700);
    assert_eq!(bevel_top.preset, "circle");
    assert!(shape_3d.bevel_bottom.is_none());

    // 3D surfaces are approximated, so exactly one degraded warning is
    // appropriate; 2D effects are fully supported.
    assert_eq!(warnings.len(), 1);
    match &warnings[0] {
        WarningEvent::FidelityDegraded { feature, class, .. } => {
            assert_eq!(*feature, "shape_3d_approximated");
            assert_eq!(*class, FidelityClass::Degraded);
        }
        other => panic!("unexpected warning: {other:?}"),
    }
}

#[test]
fn shapes_without_effects_stay_empty() {
    let shape = serde_json::json!({
        "type": "shape",
        "x": 0,
        "y": 0,
        "width": 100000,
        "height": 100000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "rect",
        "fill": { "fillType": "solid", "color": "00FF00" }
    });
    let (slide, warnings) = compile_elements(vec![shape]);
    assert!(draw_path_effects(&slide).is_empty());
    assert!(warnings.is_empty());
}

fn effects_slide(effects: ShapeEffects) -> VectorSlide {
    let mut slide = VectorSlide::new(2000000, 1000000);
    slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    slide.commands.push(VectorCommand::DrawPath {
        path: VectorPath::new(
            vec![
                PathSegment::MoveTo { x: 100000.0, y: 100000.0 },
                PathSegment::LineTo { x: 900000.0, y: 100000.0 },
                PathSegment::LineTo { x: 900000.0, y: 500000.0 },
                PathSegment::LineTo { x: 100000.0, y: 500000.0 },
                PathSegment::Close,
            ],
            FillRule::NonZero,
        ),
        fill: Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        stroke: None,
        transform: AffineTransform::default(),
        effects: Box::new(effects),
    });
    slide
}

#[test]
fn svg_emits_filter_graphs_only_when_effects_are_present() {
    let serializer = DefaultSlideSvgSerializer::new();
    let assets = NoAssets;

    let plain = serializer
        .serialize_svg(&effects_slide(ShapeEffects::default()), &assets, SvgTextMode::SelectableText)
        .expect("svg");
    assert!(!plain.contains("<filter"));
    assert!(!plain.contains("filter=\"url(#"));

    let effects = ShapeEffects {
        outer_shadow: Some(VectorOuterShadow::default()),
        glow: Some(VectorGlow { color: [0x4F, 0x81, 0xBD, 191], radius_emu: 38100 }),
        inner_shadow: Some(VectorInnerShadow {
            color: [0, 0, 0, 128],
            blur_radius_emu: 12700,
            distance_emu: 6350,
            direction_deg: 270.0,
        }),
        soft_edge: Some(VectorSoftEdge { radius_emu: 12700 }),
        ..Default::default()
    };
    let svg = serializer
        .serialize_svg(&effects_slide(effects), &assets, SvgTextMode::SelectableText)
        .expect("svg");

    assert!(svg.contains("<filter id=\"fx-0\""));
    assert!(svg.contains("filter=\"url(#fx-0)\""));
    assert!(svg.contains("<feGaussianBlur"));
    assert!(svg.contains("<feOffset"));
    assert!(svg.contains("feFlood"));
    assert!(svg.contains("flood-color=\"#4f81bd\""));
    assert!(svg.contains("feComposite"));

    // The emitted filter graph must remain well-formed SVG.
    let options = resvg::usvg::Options::default();
    resvg::usvg::Tree::from_str(&svg, &options).expect("effects SVG must parse");
}

#[test]
fn svg_reflection_emits_mirrored_use_with_mask() {
    let serializer = DefaultSlideSvgSerializer::new();
    let effects = ShapeEffects {
        reflection: Some(VectorReflection {
            blur_radius_emu: 12700,
            start_opacity: 0.5,
            end_opacity: 0.0,
            distance_emu: 19050,
            direction_deg: 90.0,
            ..VectorReflection::default()
        }),
        ..Default::default()
    };
    let svg = serializer
        .serialize_svg(&effects_slide(effects), &NoAssets, SvgTextMode::SelectableText)
        .expect("svg");

    assert!(svg.contains("<linearGradient id=\"refl-grad-0\""));
    assert!(svg.contains("<mask id=\"refl-mask-0\""));
    assert!(svg.contains("id=\"shape-0\""));
    assert!(svg.contains("mask=\"url(#refl-mask-0)\""));
    assert!(svg.contains("scale(1, -1)"));

    let options = resvg::usvg::Options::default();
    resvg::usvg::Tree::from_str(&svg, &options).expect("reflection SVG must parse");
}

#[test]
fn rasterizer_composites_shadow_below_the_shape() {
    let shadow = VectorOuterShadow {
        color: [0, 0, 0, 200],
        blur_radius_emu: 0,
        distance_emu: 200000,
        direction_deg: 90.0,
        ..VectorOuterShadow::default()
    };
    let effects = ShapeEffects { outer_shadow: Some(shadow), ..Default::default() };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    // Shape occupies y = 10..50 px and x = 10..90 px at scale 0.0001.
    // The 20 px downward offset projects a shadow band below the shape.
    let px = |x: usize, y: usize| {
        let base = (y * 200 + x) * 4;
        [rendered.pixels[base], rendered.pixels[base + 1], rendered.pixels[base + 2]]
    };
    let inside = px(50, 30);
    let below = px(50, 62);
    assert_eq!(inside, [0x44, 0x72, 0xC4]);
    assert!(below[0] < 220, "shadow must darken the background, got {below:?}");
}

#[test]
fn rasterizer_glow_halo_surrounds_the_shape() {
    let effects = ShapeEffects {
        glow: Some(VectorGlow { color: [0xFF, 0x00, 0x00, 255], radius_emu: 300000 }),
        ..Default::default()
    };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    // Immediately left of the shape (x < 10 px) the red halo must be visible.
    let base = (30 * 200 + 3) * 4;
    let pixel = [rendered.pixels[base], rendered.pixels[base + 1], rendered.pixels[base + 2]];
    assert!(pixel[0] > 180 && pixel[1] < 250, "glow halo expected, got {pixel:?}");
}

#[test]
fn rasterizer_soft_edge_feathers_up_to_the_border() {
    let effects = ShapeEffects {
        soft_edge: Some(VectorSoftEdge { radius_emu: 100000 }),
        ..Default::default()
    };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    // Interior keeps the solid fill; the border blends towards the white
    // background because its alpha was feathered.
    let center = (30 * 200 + 50) * 4;
    let border = (30 * 200 + 10) * 4;
    assert_eq!(
        [rendered.pixels[center], rendered.pixels[center + 1], rendered.pixels[center + 2]],
        [0x44, 0x72, 0xC4]
    );
    assert!(rendered.pixels[border] > 150, "border must be feathered");
}

#[test]
fn rasterizer_scene_camera_projects_the_path() {
    let scene = VectorScene3d {
        camera_type: "isometricTopUp".to_string(),
        fov_deg: 0.0,
        zoom: 1.0,
        rot: None,
        light_rig_type: "threePt".to_string(),
        light_dir: "tl".to_string(),
        light_rot: None,
        light_direction_deg: 225.0,
        units_per_inch: 914_400.0,
    };
    let effects = ShapeEffects { scene_3d: Some(scene), ..Default::default() };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    // The projected shape is vertically compressed, so the original
    // rectangle's bottom-left corner is no longer painted.
    let corner = (50 * 200 + 12) * 4;
    let background = [rendered.pixels[corner], rendered.pixels[corner + 1], rendered.pixels[corner + 2]];
    assert_eq!(background, [255, 255, 255]);
}

#[test]
fn rasterizer_inner_shadow_darkens_the_interior_edge() {
    let effects = ShapeEffects {
        inner_shadow: Some(VectorInnerShadow {
            color: [0, 0, 0, 255],
            blur_radius_emu: 0,
            distance_emu: 100000,
            direction_deg: 90.0,
        }),
        ..Default::default()
    };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let px = |x: usize, y: usize| {
        let base = (y * 200 + x) * 4;
        [rendered.pixels[base], rendered.pixels[base + 1], rendered.pixels[base + 2]]
    };
    // The inset band hugs the top edge and leaves the centre untouched.
    assert!(px(50, 12)[0] < 100, "inner shadow expected at the top edge");
    assert_eq!(px(50, 30), [0x44, 0x72, 0xC4]);
}

#[test]
fn rasterizer_extrusion_layers_below_the_shape() {
    let effects = ShapeEffects {
        shape_3d: Some(VectorShape3d {
            extrusion_depth_emu: 200000,
            extrusion_color: Some([0x7F, 0x7F, 0x7F, 255]),
            contour_width_emu: 0,
            contour_color: None,
            prst_material: String::new(),
            bevel_top: None,
            bevel_bottom: None,
        }),
        ..Default::default()
    };
    let slide = effects_slide(effects);

    let rasterizer = TinySkiaRasterizer::new();
    let target = RenderTargetSpec { width_px: 200, height_px: 100 };
    let rendered = rasterizer.rasterize(&slide, &target, &NoAssets).expect("rasterize");

    let base = (55 * 200 + 50) * 4;
    let pixel = [rendered.pixels[base], rendered.pixels[base + 1], rendered.pixels[base + 2]];
    assert!(
        (pixel[0] as i32 - 0x7F).abs() < 8
            && (pixel[1] as i32 - 0x7F).abs() < 8
            && (pixel[2] as i32 - 0x7F).abs() < 8,
        "extrusion color expected below the shape, got {pixel:?}"
    );
}

#[test]
fn legacy_wireframe_paints_rims_not_a_filled_face() {
    let effects = ShapeEffects {
        shape_3d: Some(VectorShape3d {
            extrusion_depth_emu: 100_000,
            extrusion_color: None,
            contour_width_emu: 0,
            contour_color: None,
            prst_material: "legacyWireframe".to_string(),
            bevel_top: None,
            bevel_bottom: None,
        }),
        ..Default::default()
    };
    let slide = effects_slide(effects);
    let rendered = TinySkiaRasterizer::new()
        .rasterize(
            &slide,
            &RenderTargetSpec {
                width_px: 200,
                height_px: 100,
            },
            &NoAssets,
        )
        .expect("rasterize");
    let px = |x: usize, y: usize| {
        let base = (y * 200 + x) * 4;
        [
            rendered.pixels[base],
            rendered.pixels[base + 1],
            rendered.pixels[base + 2],
            rendered.pixels[base + 3],
        ]
    };
    // The face interior stays the white background (no solid fill).
    assert_eq!(px(50, 30), [255, 255, 255, 255], "wireframe has no face");
    // The silhouette rim is painted.
    assert_ne!(px(10, 30), [255, 255, 255, 255], "front rim is stroked");
}

#[test]
fn every_preset_material_has_a_surface_class() {
    use office_terminal_core::scene3d::Material;
    let names = [
        "legacyMatte", "legacyPlastic", "legacyMetal", "legacyWireframe", "matte",
        "plastic", "metal", "warmMatte", "translucentPowder", "powder", "dkEdge",
        "softEdge", "clear", "flat", "softmetal",
    ];
    for name in names {
        let material = Material::from_preset(name);
        assert!(
            material.diffuse > 0.0 || material.specular > 0.0 || name == "legacyWireframe",
            "{name} keeps a surface"
        );
    }
    assert!(Material::from_preset("metal").specular > Material::from_preset("matte").specular);
}
