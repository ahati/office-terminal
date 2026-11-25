//! Ports of the `shapes-b` effect-geometry specs:
//! `bevel-band-conversion.test.ts`, `scene3d-transform-order.test.ts` and the
//! geometry cases of `shape-effect-transform.test.ts`.
//!
//! Reference: `/tmp/oooxml/packages/pptx/src/{bevel-band-conversion,
//! scene3d-transform-order,shape-effect-transform}.test.ts` @ `1f5c6e1d`.
//! Loaded as a module of the `parity_shapes_b_tests` target.
//!
//! `shape-effect-transform` cases 7-9 stay N/A (an `OffscreenCanvas` /
//! `drawImage` call spy on a device-space paint cache the Rust backend does not
//! have). `shape-orthographic-bevel` stays BLOCKED: its assertions are browser
//! PNG oracles over a full homography warp, extrusion walls, wireframe rims and
//! per-material shading, none of which this crate paints.

use super::*;
use office_terminal_core::compiler::shape_effects::split_scene3d_shape_transform;
use office_terminal_core::scene3d::{self, Alignment, CameraInput, EffectTransform, Rot3d};

/// The reference slide-6 conversion factor: `cssScale = 1920 / 12192000`
/// (EMU → CSS px) for a 12 192 000 EMU slide rendered 1920 px wide.
fn css_scale() -> f64 {
    1920.0 / 12_192_000.0
}

/// slide-6 declared widths: bevel `w` = 304 800 EMU (24 pt), `h` = 152 400 EMU.
const BEVEL_W_EMU: i64 = 304_800;
const BEVEL_H_EMU: i64 = 152_400;

fn beveled(width_emu: i64, height_emu: i64, material: &str) -> VectorShape3d {
    VectorShape3d {
        extrusion_depth_emu: 0,
        extrusion_color: None,
        contour_width_emu: 0,
        contour_color: None,
        prst_material: material.to_string(),
        bevel_top: Some(VectorBevel {
            width_emu,
            height_emu,
            preset: "hardEdge".to_string(),
        }),
        bevel_bottom: None,
    }
}

// ── bevel-band-conversion.test.ts ───────────────────────────────────────────

/// `widthPx == w(EMU) × scale × devScale` exactly, for every device scale the
/// reference renders at (ref `bevel-band-conversion.test.ts:20-30`).
///
/// The two raster cases already ported above can only reach devScale 1 and ¼
/// (they mirror the render target); the API-level assertion covers the full
/// `[1, 4, 8]` loop of the spec, including devScale 4 (192 px) and 8 (384 px)
/// which the old 48 px band clamp made unreachable.
#[test]
fn bevel_inputs_match_the_declared_emu_width_at_every_device_scale() {
    let shape = beveled(BEVEL_W_EMU, BEVEL_H_EMU, "matte");
    for dev_scale in [1.0, 4.0, 8.0] {
        let inputs = scene3d::build_bevel_inputs(Some(&shape), None, css_scale(), dev_scale);
        assert_eq!(inputs.len(), 1, "one front lip at devScale {dev_scale}");
        let bevel = &inputs[0];
        let expect_width = BEVEL_W_EMU as f64 * css_scale() * dev_scale;
        let expect_height = BEVEL_H_EMU as f64 * css_scale() * dev_scale;
        assert!(
            (bevel.width_px - expect_width).abs() <= 1.0,
            "widthPx {} must be w(EMU)×scale×devScale {expect_width} ±1 (devScale {dev_scale})",
            bevel.width_px
        );
        assert!(
            (bevel.height_px - expect_height).abs() <= 1.0,
            "heightPx {} must be h(EMU)×scale×devScale {expect_height} ±1 (devScale {dev_scale})",
            bevel.height_px
        );
        // Linear in devScale: 24 pt at 1920 px wide is 24 px per devScale unit.
        assert!(
            (bevel.width_px / dev_scale - BEVEL_W_EMU as f64 * css_scale()).abs() < 1e-6,
            "band must stay linear in devScale (devScale {dev_scale})"
        );
        assert!(!bevel.bottom, "bevelT is the front lip");
    }
}

/// A zero-sized bevel contributes no band at all (ref
/// `bevel-band-conversion.test.ts:32-35`).
#[test]
fn bevel_inputs_omit_a_zero_sized_bevel() {
    for (w, h) in [(0, 100), (100, 0)] {
        let shape = beveled(w, h, "matte");
        assert!(
            scene3d::build_bevel_inputs(Some(&shape), None, css_scale(), 4.0).is_empty(),
            "bevel w={w} h={h} must produce no band"
        );
    }
    // No `a:sp3d` at all ⇒ no bevel to shade either.
    assert!(scene3d::build_bevel_inputs(None, None, css_scale(), 4.0).is_empty());
}

// ── scene3d-transform-order.test.ts ─────────────────────────────────────────

/// Authored flips warp with the local face; the 2-D rotation stays outside the
/// projection (ref `scene3d-transform-order.test.ts:5-12`).
#[test]
fn scene3d_transform_order_keeps_rotation_outside_the_face() {
    let split = split_scene3d_shape_transform(25.0, true, false);
    assert_eq!(split.outer_rotation, 25.0);
    assert!(split.local_flip_h);
    assert!(!split.local_flip_v);
    // An unflipped, unrotated shape keeps both halves neutral.
    let neutral = split_scene3d_shape_transform(0.0, false, false);
    assert_eq!(neutral.outer_rotation, 0.0);
    assert!(!neutral.local_flip_h && !neutral.local_flip_v);
}

// ── shape-effect-transform.test.ts 3-6 ──────────────────────────────────────

fn front_camera() -> CameraInput {
    CameraInput {
        prst: "orthographicFront".to_string(),
        ..CameraInput::default()
    }
}

/// The authored shadow anchor stays distinct from the projected AABB corner
/// (ref `shape-effect-transform.test.ts:36-49`).
#[test]
fn projected_effect_anchor_is_distinct_from_the_aabb_corner() {
    let geometry = scene3d::projected_effect_geometry(
        &front_camera(),
        &EffectTransform {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 75.0,
            f: -25.0,
        },
        0.0,
        0.0,
        100.0,
        50.0,
        0.0,
        Alignment::TopRight,
        0.0,
    );
    assert_eq!(
        geometry.bbox,
        scene3d::Bounds {
            x: 25.0,
            y: -25.0,
            w: 50.0,
            h: 100.0
        }
    );
    assert_eq!(geometry.anchor, (75.0, 75.0));
    // The AABB corner of `tr` would be (25, -25): the anchor is the authored
    // corner pushed through the transform, not the bounding-box corner.
    assert_ne!(geometry.anchor, (geometry.bbox.x + geometry.bbox.w, geometry.bbox.y));
}

/// Raster padding rounds up to device px and camera zoom scales the local
/// margin (ref `shape-effect-transform.test.ts:51-62`).
#[test]
fn projected_effect_bounds_round_the_edge_pad_to_device_pixels() {
    let camera = CameraInput {
        zoom: 4.0,
        ..front_camera()
    };
    let geometry = scene3d::projected_effect_geometry(
        &camera,
        &EffectTransform::uniform_scale(2.0),
        0.0,
        0.0,
        100.0,
        50.0,
        1.1,
        Alignment::TopRight,
        0.0,
    );
    // 1.1 CSS px rounds to 3 device px; camera zoom scales that local margin.
    assert!((geometry.bbox.x + 312.0).abs() < 1e-10, "bbox.x {}", geometry.bbox.x);
    assert!((geometry.bbox.w - 824.0).abs() < 1e-10, "bbox.w {}", geometry.bbox.w);
}

/// The exposed back surface and its effect anchor sit at the extrusion base
/// (ref `shape-effect-transform.test.ts:64-77`).
#[test]
fn projected_effect_bounds_use_the_extrusion_base_back_surface() {
    let camera = CameraInput {
        rot: Some(Rot3d {
            lat: 0.0,
            lon: 150.0,
            rev: 0.0,
        }),
        ..front_camera()
    };
    let geometry = scene3d::projected_effect_geometry(
        &camera,
        &EffectTransform::IDENTITY,
        0.0,
        0.0,
        100.0,
        50.0,
        0.0,
        Alignment::TopRight,
        40.0,
    );
    // Parallel projection: width = 100·cos(30°), centre = 50 + 40·sin(150°).
    // The back-facing top-right corner is the left end of the projected box.
    assert!(
        (geometry.bbox.x - (70.0 - 50.0 * (std::f64::consts::PI / 6.0).cos())).abs() < 1e-10,
        "bbox.x {}",
        geometry.bbox.x
    );
    assert!(
        (geometry.bbox.w - 100.0 * (std::f64::consts::PI / 6.0).cos()).abs() < 1e-10,
        "bbox.w {}",
        geometry.bbox.w
    );
    assert!((geometry.anchor.0 - geometry.bbox.x).abs() < 1e-10);
    assert_eq!(geometry.anchor.1, 0.0);
}

/// A camera-crossing rear face and its effect anchor stay in one finite scene
/// (ref `shape-effect-transform.test.ts:79-101`).
#[test]
fn projected_effect_bounds_keep_a_camera_crossing_rear_face_finite() {
    let units = 96.0;
    let (w, h, depth) = (8.0 * units, units, 8.0 * units);
    // Independent pinhole bounds: fixed 6.28-inch lens, 90° fov, 2 % grace gap.
    let extent = 2.0 * 3.0f64.sqrt() * units;
    let distance = extent + 4.0 * units + 0.02 * (2.0 * extent + 4.0 * units);
    let mut q = Vec::new();
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            let z = sx * extent + 4.0 * units;
            q.push((
                w / 2.0 + (-sx * 2.0 + 4.0 * 3.0f64.sqrt()) * units / (1.0 - z / distance),
                h / 2.0 + sy * h / 2.0 / (1.0 - z / distance),
            ));
        }
    }
    let span = |axis: usize| {
        let values: Vec<f64> = q.iter().map(|p| if axis == 0 { p.0 } else { p.1 }).collect();
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (min, max)
    };

    let camera = CameraInput {
        prst: "perspectiveFront".to_string(),
        fov: Some(90.0),
        rot: Some(Rot3d {
            lat: 0.0,
            lon: 120.0,
            rev: 0.0,
        }),
        ..CameraInput::default()
    };
    let transform = EffectTransform {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        e: 5.0,
        f: 7.0,
    };
    let geometry =
        scene3d::projected_effect_geometry(&camera, &transform, 0.0, 0.0, w, h, 0.0, Alignment::TopRight, depth);
    let (x0, x1) = span(0);
    let (y0, y1) = span(1);
    assert!(
        (geometry.bbox.x - (5.0 + 2.0 * x0)).abs() < 1e-7,
        "bbox.x {}",
        geometry.bbox.x
    );
    assert!(
        (geometry.bbox.y - (7.0 + 2.0 * y0)).abs() < 1e-7,
        "bbox.y {}",
        geometry.bbox.y
    );
    assert!(
        (geometry.bbox.w - 2.0 * (x1 - x0)).abs() < 1e-7,
        "bbox.w {}",
        geometry.bbox.w
    );
    assert!(
        (geometry.bbox.h - 2.0 * (y1 - y0)).abs() < 1e-7,
        "bbox.h {}",
        geometry.bbox.h
    );
    // `q[2]` is the unit-frame `(1, 0)` (top-right) corner the anchor uses.
    assert!(
        (geometry.anchor.0 - (5.0 + 2.0 * q[2].0)).abs() < 1e-7,
        "anchor {}",
        geometry.anchor.0
    );
    assert!(
        (geometry.anchor.1 - (7.0 + 2.0 * q[2].1)).abs() < 1e-7,
        "anchor {}",
        geometry.anchor.1
    );
}

// ── shape-orthographic-bevel.test.ts: geometry half of the back-bevel case ──

/// The reference case "shows a back bevel only when the camera exposes the back
/// face" (`shape-orthographic-bevel.test.ts:283-302`) asserts pixels of a
/// browser render, so the case itself stays BLOCKED; its geometry is pinned
/// here: `bevelB` is a bottom lip at the extrusion base, and only a back-facing
/// camera exposes it (MS-OI29500 §2.1.1268).
#[test]
fn bottom_bevel_inputs_sit_at_the_extrusion_base_behind_the_front_face() {
    let mut shape = beveled(BEVEL_W_EMU, BEVEL_H_EMU, "matte");
    shape.extrusion_depth_emu = 228_600; // 24 px at one px per 9525 EMU
    shape.bevel_bottom = Some(VectorBevel {
        width_emu: BEVEL_W_EMU,
        height_emu: BEVEL_H_EMU,
        preset: "hardEdge".to_string(),
    });
    let inputs = scene3d::build_bevel_inputs(Some(&shape), None, 1.0 / 9525.0, 1.0);
    assert_eq!(inputs.len(), 2, "front and bottom lips");
    let bottom = inputs.iter().find(|bevel| bevel.bottom).expect("bottom lip");
    assert!((bottom.width_px - 32.0).abs() < 1e-6, "bottom band {}", bottom.width_px);
    assert!((bottom.depth_px - 24.0).abs() < 1e-6, "base depth {}", bottom.depth_px);

    // Face-on: the back face is hidden, so only the front lip is visible.
    let front = scene3d::projected_surface(&front_camera(), 100.0, 100.0, 24.0, 0.0, 1.0);
    assert!(!front.back_facing, "orthographicFront looks at the front face");
    // lon 180 turns the face away: the rear plane (24 px back) is selected.
    let back = CameraInput {
        rot: Some(Rot3d {
            lat: 0.0,
            lon: 180.0,
            rev: 0.0,
        }),
        ..front_camera()
    };
    assert!(scene3d::projected_surface(&back, 100.0, 100.0, 24.0, 0.0, 1.0).back_facing);
    // lon 150 exposes the base to the right by extrusionH·sin(150°) = 12 px.
    let skewed = CameraInput {
        rot: Some(Rot3d {
            lat: 0.0,
            lon: 150.0,
            rev: 0.0,
        }),
        ..front_camera()
    };
    assert!(scene3d::projected_surface(&skewed, 100.0, 100.0, 24.0, 0.0, 1.0).back_facing);
    let offset = scene3d::compute_depth_offset(&skewed, 100.0, 100.0, 24.0);
    assert!((offset.0 - 12.0).abs() < 1e-9, "back plane x {}", offset.0);
}

/// Divergence fix: the raster band is no longer clamped to 48 px
/// (`rasterizer/bevel.rs` before this pass), so a 60 px authored bevel shades a
/// 60 px band exactly as `w(EMU) × scale` says.
#[test]
fn raster_bevel_band_is_not_clamped() {
    // 60 device px at the fixture scale (6350 EMU per device px).
    const BAND_EMU: i64 = 60 * 6350;
    let plain = raster(
        &compile_slide_emu(vec![beveled_rect(None)], REF_SLIDE_W, REF_SLIDE_H),
        1920,
        400,
    );
    let beveled = raster(
        &compile_slide_emu(vec![beveled_rect(Some((BAND_EMU, BAND_EMU)))], REF_SLIDE_W, REF_SLIDE_H),
        1920,
        400,
    );
    let depth = measured_band_depth(&plain, &beveled, 960, 100, 70);
    assert!(
        (depth as f32 - 59.0).abs() <= 2.0,
        "a 60 px bevel must shade a 60 px band (dist starts at 1), measured {depth}"
    );
    // Inside the band, past the old 48 px clamp, the row is shaded ...
    assert_ne!(
        at(&beveled, 960, 155),
        at(&plain, 960, 155),
        "row at distance 55 is shaded"
    );
    // ... and one row beyond the authored band is flat again.
    assert_eq!(
        at(&beveled, 960, 170),
        at(&plain, 960, 170),
        "row at distance 70 is flat"
    );
}

// ── scene3d camera homography (supplementary S6 coverage) ───────────────────

fn scene3d(camera: &str, fov: f32, zoom: f32, units_per_inch: f32) -> VectorScene3d {
    VectorScene3d {
        camera_type: camera.to_string(),
        fov_deg: fov,
        zoom,
        rot: None,
        light_rig_type: "threePt".to_string(),
        light_dir: "t".to_string(),
        light_rot: None,
        light_direction_deg: 270.0,
        units_per_inch,
    }
}

fn projected_slide(scene: VectorScene3d) -> VectorSlide {
    let path = VectorPath::new(
        vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 200.0, y: 0.0 },
            PathSegment::LineTo { x: 200.0, y: 100.0 },
            PathSegment::LineTo { x: 0.0, y: 100.0 },
            PathSegment::Close,
        ],
        FillRule::NonZero,
    );
    let mut slide = VectorSlide::new(220, 120);
    slide.background = Some(VectorBackground::Solid(WHITE));
    slide.commands.push(VectorCommand::DrawPath {
        path,
        fill: Some(VectorPaint::Solid([0x44, 0x72, 0xC4, 255])),
        stroke: None,
        transform: AffineTransform::identity(),
        effects: Box::new(ShapeEffects {
            scene_3d: Some(scene),
            ..ShapeEffects::default()
        }),
    });
    slide
}

/// A perspective camera warps the authored face through the quad homography
/// (ref `renderer.ts:6400-6640` `projectScene3dPaint` + `drawProjected`).
///
/// `perspectiveContrastingRightFacing` (lat 10.4°, lon 316.1°) maps a 200 × 100
/// face at the origin to the quad (23.907,11.284) (169.240,−9.286)
/// (168.425,84.814) (24.891,114.311): the authored top-left corner is far from
/// the painted silhouette, so a flat billboard would ink it.
#[test]
fn perspective_camera_warps_the_face_through_the_quad_homography() {
    let slide = projected_slide(scene3d("perspectiveContrastingRightFacing", 0.0, 1.0, 96.0));
    let img = raster(&slide, 220, 120);

    // The camera quad's interior is painted, including the far right lobe.
    assert_eq!(at(&img, 100, 50), [0x44, 0x72, 0xC4, 255], "projected face is painted");
    assert_eq!(
        at(&img, 150, 40),
        [0x44, 0x72, 0xC4, 255],
        "projected face reaches x=150"
    );
    // The authored top-left corner is outside the projected quad.
    assert_eq!(at(&img, 2, 2), WHITE, "no flat billboard at the authored corner");
    assert_eq!(at(&img, 20, 20), WHITE, "perspective leaves the top-left corner empty");
}

/// A face-on perspective camera still multiplies the face by `a:camera@zoom`
/// (ref `computeScene3dQuad`: `projected * zoom`).
#[test]
fn perspective_camera_zoom_scales_the_projected_face() {
    let slide = projected_slide(scene3d("perspectiveFront", 0.0, 2.0, 96.0));
    let img = raster(&slide, 220, 120);

    // zoom 2 about the centre: the 200 × 100 face covers x ∈ [−100, 300],
    // y ∈ [−50, 150], so the authored bottom-right corner and beyond paint.
    assert_eq!(at(&img, 100, 50), [0x44, 0x72, 0xC4, 255], "face centre");
    assert_eq!(at(&img, 210, 110), [0x44, 0x72, 0xC4, 255], "zoom 2 reaches (210,110)");
    assert_eq!(at(&img, 5, 5), [0x44, 0x72, 0xC4, 255], "zoom 2 covers the top-left");
}
