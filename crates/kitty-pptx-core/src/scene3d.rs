//! Shared 3D scene mathematics for PRD 08 §3.6.
//!
//! Both render backends (the `tiny-skia` rasterizer and the SVG serializer)
//! consume the resolution-independent [`VectorScene3d`] / [`VectorShape3d`]
//! contracts. This module centralises the camera projection approximation,
//! bevel profile curves, and Blinn-Phong material lighting so both backends
//! produce consistent output.

use crate::contracts::{AffineTransform, VectorScene3d};

pub mod bevel_inputs;
pub mod camera;
pub mod homography;
pub mod projection;

pub use bevel_inputs::{build_bevel_inputs, BevelInput, PresetMaterial};
pub use camera::{
    camera_rotation, compute_depth_offset, compute_scene3d_quad, is_back_facing,
    is_scene3d_non_identity, scene3d_surface_frame, CameraInput, Rot3d, Scene3dQuad,
};
pub use homography::{expand_projected_quad, invert_h, project_quad_point, unit_square_to_quad};
pub use projection::{
    authored_alignment_anchor, projected_effect_geometry, projected_surface,
    transformed_effect_bounds, Alignment, Bounds, EffectTransform, ProjectedEffectGeometry,
    ProjectedSurface,
};

/// EMUs per CSS/SVG user unit at 96 DPI (AGENTS.md §4).
pub const EMU_PER_PX: f32 = 9525.0;

/// Convert an EMU length to abstract 96-DPI pixels.
#[inline]
pub fn emu_to_px(emu: i64) -> f32 {
    emu as f32 / EMU_PER_PX
}

/// Camera orientation derived from an `ST_PresetCameraType` value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAngles {
    /// Rotation about the horizontal (X) axis, degrees.
    pub lat_deg: f32,
    /// Rotation about the vertical (Y) axis, degrees.
    pub lon_deg: f32,
    /// Default field of view for perspective presets, degrees.
    pub default_fov_deg: f32,
}

impl Default for CameraAngles {
    fn default() -> Self {
        Self {
            lat_deg: 0.0,
            lon_deg: 0.0,
            default_fov_deg: 45.0,
        }
    }
}

/// Whether the preset uses a perspective (depth-dividing) projection.
pub fn is_perspective(camera_type: &str) -> bool {
    camera_type.starts_with("perspective") || camera_type.starts_with("legacyPerspective")
}

/// Whether the preset is a modern *parallel* (orthographic/isometric/oblique)
/// camera. The reference's oblique presets shear the rotated depth, so they are
/// affine but not identity (ref `scene3d-camera.ts:151-206`).
pub fn is_scene3d_orthographic(camera_type: &str) -> bool {
    !is_perspective(camera_type)
}

/// Map a DrawingML preset camera name to its base orientation.
///
/// The table covers the standard `ST_PresetCameraType` families. Unknown
/// presets degrade to the front-facing orthographic camera.
pub fn camera_angles(camera_type: &str) -> CameraAngles {
    // 35.264° = atan(1/sqrt(2)), the classic isometric elevation.
    const ISO: f32 = 35.264;
    match camera_type {
        "isometricTopUp" => CameraAngles { lat_deg: -ISO, lon_deg: 0.0, ..Default::default() },
        "isometricTopDown" => CameraAngles { lat_deg: ISO, lon_deg: 0.0, ..Default::default() },
        "isometricLeftUp" => CameraAngles { lat_deg: -ISO, lon_deg: -45.0, ..Default::default() },
        "isometricLeftDown" => CameraAngles { lat_deg: ISO, lon_deg: -45.0, ..Default::default() },
        "isometricRightUp" => CameraAngles { lat_deg: -ISO, lon_deg: 45.0, ..Default::default() },
        "isometricRightDown" => CameraAngles { lat_deg: ISO, lon_deg: 45.0, ..Default::default() },
        "isometricOffAxis1Left" | "isometricOffAxis2Left" => {
            CameraAngles { lat_deg: -ISO, lon_deg: -30.0, ..Default::default() }
        }
        "isometricOffAxis1Right" | "isometricOffAxis2Right" => {
            CameraAngles { lat_deg: -ISO, lon_deg: 30.0, ..Default::default() }
        }
        "isometricOffAxis1Top" | "isometricOffAxis2Top" => {
            CameraAngles { lat_deg: -ISO, lon_deg: 0.0, ..Default::default() }
        }
        "perspectiveLeft" | "perspectiveRelaxedLeft" | "legacyPerspectiveLeft" => {
            CameraAngles { lat_deg: 0.0, lon_deg: -20.0, ..Default::default() }
        }
        "perspectiveRight" | "perspectiveRelaxedRight" | "legacyPerspectiveRight" => {
            CameraAngles { lat_deg: 0.0, lon_deg: 20.0, ..Default::default() }
        }
        "perspectiveContrasting" => CameraAngles { default_fov_deg: 60.0, ..Default::default() },
        "obliqueTopLeft" => CameraAngles { lat_deg: 20.0, lon_deg: -20.0, ..Default::default() },
        "obliqueTop" => CameraAngles { lat_deg: 20.0, lon_deg: 0.0, ..Default::default() },
        "obliqueTopRight" => CameraAngles { lat_deg: 20.0, lon_deg: 20.0, ..Default::default() },
        "obliqueLeft" => CameraAngles { lat_deg: 0.0, lon_deg: -20.0, ..Default::default() },
        "obliqueRight" => CameraAngles { lat_deg: 0.0, lon_deg: 20.0, ..Default::default() },
        "obliqueBottomLeft" => CameraAngles { lat_deg: -20.0, lon_deg: -20.0, ..Default::default() },
        "obliqueBottom" => CameraAngles { lat_deg: -20.0, lon_deg: 0.0, ..Default::default() },
        "obliqueBottomRight" => CameraAngles { lat_deg: -20.0, lon_deg: 20.0, ..Default::default() },
        _ => CameraAngles::default(),
    }
}

/// Project a local point `(x, y, 0)` through the camera and return slide-local
/// coordinates. `d_cam` is `None` for orthographic presets.
#[allow(dead_code)] // Kept for the legacy `CameraAngles` lookup API.
fn project_point(
    x: f32,
    y: f32,
    center: (f32, f32),
    angles: CameraAngles,
    d_cam: Option<f32>,
) -> (f32, f32) {
    let px = x - center.0;
    let py = y - center.1;

    let lon = angles.lon_deg.to_radians();
    let lat = angles.lat_deg.to_radians();
    let (sin_lon, cos_lon) = lon.sin_cos();
    let (sin_lat, cos_lat) = lat.sin_cos();

    // Rotate about Y (lon), then about X (lat). The source plane is z = 0.
    let x1 = px * cos_lon;
    let z1 = -px * sin_lon;
    let y2 = py * cos_lat - z1 * sin_lat;
    let z2 = py * sin_lat + z1 * cos_lat;

    let factor = match d_cam {
        Some(d) if d + z2 > f32::EPSILON => d / (d + z2),
        _ => 1.0,
    };

    (center.0 + x1 * factor, center.1 + y2 * factor)
}

/// Build the affine approximation of the scene camera for a shape whose local
/// drawing occupies `(0, 0, local_w, local_h)`.
///
/// This is the SVG backend's *documented degradation* when the reference quad
/// is projective: the affine is fitted to the three local box corners `(0,0)`,
/// `(w,0)`, `(0,h)` of the reference [`compute_scene3d_quad`]. Orthographic
/// presets are reproduced exactly. The rasterizer uses the true homography
/// instead (`rasterizer::shape_effect::projection`).
pub fn camera_affine(
    scene: &VectorScene3d,
    local_w: f32,
    local_h: f32,
    _slide_width_emu: i64,
) -> AffineTransform {
    if local_w <= 0.0 || local_h <= 0.0 {
        return AffineTransform::identity();
    }
    let camera = CameraInput::from_scene(scene);
    let quad = compute_scene3d_quad(&camera, local_w as f64, local_h as f64, 0.0);
    let [q0, q1, _, q3] = quad.corners;

    AffineTransform {
        sx: ((q1.0 - q0.0) / local_w as f64) as f32,
        ky: ((q1.1 - q0.1) / local_w as f64) as f32,
        kx: ((q3.0 - q0.0) / local_h as f64) as f32,
        sy: ((q3.1 - q0.1) / local_h as f64) as f32,
        tx: q0.0 as f32,
        ty: q0.1 as f32,
    }
}

/// Bevel profile evaluation: returns `(f(u), f'(u))` for the normalized
/// boundary distance `u` in `[0, 1]` (PRD 08 §3.6.2).
pub fn bevel_curve(preset: &str, u: f32, height: f32) -> (f32, f32) {
    let u = u.clamp(0.0, 1.0);
    let h = height;
    match preset {
        "relaxed" => {
            let v = h * 0.5 * (1.0 - (std::f32::consts::PI * u).cos());
            let d = 0.5 * std::f32::consts::PI * h * (std::f32::consts::PI * u).sin();
            (v, d)
        }
        "convex" => {
            let a = std::f32::consts::FRAC_PI_2 * u;
            (h * a.sin(), 0.5 * std::f32::consts::PI * h * a.cos())
        }
        "slant" => (h * u, h),
        "riblet" => (
            h * u.powi(3) * (10.0 - 15.0 * u + 6.0 * u * u),
            30.0 * h * u * u * (1.0 - u) * (1.0 - u),
        ),
        // "circle" and unknown presets fall back to a circular arc.
        _ => {
            let one_minus = 1.0 - u;
            let root = (1.0 - one_minus * one_minus).max(0.0).sqrt();
            let value = h * root;
            let slope = if root > 1e-4 {
                h * one_minus / root
            } else {
                0.0
            };
            (value, slope)
        }
    }
}

/// Build the normalized surface normal for a bevel point.
///
/// `slope` is `f'(u)` and `grad` is the distance-field gradient
/// `(∂d/∂x, ∂d/∂y)` (PRD 08 §3.6.3).
pub fn bevel_normal(slope: f32, grad: (f32, f32)) -> [f32; 3] {
    let raw = [-slope * grad.0, -slope * grad.1, 1.0];
    normalize3(raw)
}

/// Blinn-Phong material coefficients (PRD 08 §3.6.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub ambient: f32,
    pub diffuse: f32,
    pub specular: f32,
    pub shininess: f32,
}

impl Material {
    /// Material presets from `ST_PresetMaterialType`.
    pub fn from_preset(name: &str) -> Self {
        match name {
            "metal" => Self { ambient: 0.10, diffuse: 0.30, specular: 0.90, shininess: 64.0 },
            "plastic" => Self { ambient: 0.20, diffuse: 0.65, specular: 0.45, shininess: 32.0 },
            // warmMatte is also the safe fallback for unknown presets.
            _ => Self { ambient: 0.35, diffuse: 0.70, specular: 0.05, shininess: 4.0 },
        }
    }
}

impl Default for Material {
    fn default() -> Self {
        Self::from_preset("warmMatte")
    }
}

/// Unit light vector for an azimuth (degrees clockwise from East in screen
/// coordinates) at the standard 45° elevation.
pub fn light_vector(direction_deg: f32) -> [f32; 3] {
    let rad = direction_deg.to_radians();
    let xy = (2.0f32 / 3.0).sqrt();
    [xy * rad.cos(), xy * rad.sin(), (1.0f32 / 3.0).sqrt()]
}

/// Blinn-Phong illumination for a normal and light direction.
pub fn blinn_phong(normal: [f32; 3], light: [f32; 3], material: Material) -> f32 {
    let n = normalize3(normal);
    let l = normalize3(light);
    let diffuse = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]).max(0.0);

    // Viewer fixed at (0, 0, 1); halfway vector between light and viewer.
    let h = normalize3([l[0], l[1], l[2] + 1.0]);
    let spec_base = (n[0] * h[0] + n[1] * h[1] + n[2] * h[2]).max(0.0);

    material.ambient
        + material.diffuse * diffuse
        + material.specular * spec_base.powf(material.shininess)
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-6 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(camera_type: &str, fov: f32) -> VectorScene3d {
        VectorScene3d {
            camera_type: camera_type.to_string(),
            fov_deg: fov,
            zoom: 1.0,
            rot: None,
            light_rig_type: "threePt".to_string(),
            light_dir: "t".to_string(),
            light_rot: None,
            light_direction_deg: 225.0,
            units_per_inch: 914_400.0,
        }
    }

    #[test]
    fn emu_conversions_match_the_domain_constants() {
        assert!((emu_to_px(9525) - 1.0).abs() < 1e-6);
        assert!((emu_to_px(12700) - 4.0 / 3.0).abs() < 1e-5);
        assert!((emu_to_px(914400) - 96.0).abs() < 1e-4);
    }

    #[test]
    fn front_camera_is_identity() {
        let affine = camera_affine(&scene("orthographicFront", 0.0), 100.0, 50.0, 9144000);
        assert!((affine.sx - 1.0).abs() < 1e-5);
        assert!((affine.sy - 1.0).abs() < 1e-5);
        assert!(affine.kx.abs() < 1e-5);
        assert!(affine.ky.abs() < 1e-5);
        assert!(affine.tx.abs() < 1e-5);
        assert!(affine.ty.abs() < 1e-5);
    }

    #[test]
    fn isometric_top_compresses_and_skews() {
        // `isometricTopUp` PRESETS entry (lat 324.6°, lon 314.7°, rev 60.2°,
        // ref scene3d-camera.ts:190-235). The reference maps the 200 × 100 box
        // to (0,0)-anchored basis (141.375,−81.150) along +X and
        // (70.734,81.150) along +Y.
        let affine = camera_affine(&scene("isometricTopUp", 0.0), 200.0, 100.0, 9144000);
        assert!((affine.sx - 0.706_874).abs() < 1e-4, "sx {}", affine.sx);
        assert!((affine.ky + 0.405_751).abs() < 1e-4, "ky {}", affine.ky);
        assert!((affine.kx - 0.707_340).abs() < 1e-4, "kx {}", affine.kx);
        assert!((affine.sy - 0.405_097).abs() < 1e-4, "sy {}", affine.sy);
        // Perspective preset still returns a finite affine.
        let p = camera_affine(&scene("perspectiveRelaxed", 45.0), 200.0, 100.0, 9144000);
        assert!(p.sx.is_finite() && p.sy.is_finite());
    }

    #[test]
    fn bevel_normals_are_unit_length() {
        for preset in ["circle", "relaxed", "convex", "slant", "riblet"] {
            for i in 0..=10 {
                let u = i as f32 / 10.0;
                let (_, slope) = bevel_curve(preset, u, 20.0);
                let n = bevel_normal(slope, (0.6, 0.8));
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-5, "{preset} u={u} len={len}");
            }
        }
    }

    #[test]
    fn light_vector_is_unit_and_top_left_matches_prd() {
        let l = light_vector(225.0);
        let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
        assert!(l[0] < -0.5 && l[1] < -0.5 && l[2] > 0.5);
    }

    #[test]
    fn authored_zoom_scales_a_face_on_perspective_face() {
        // ref scene3d-camera.ts:355-446: zoom multiplies the projected plane
        // about the shape centre, after the perspective divide.
        let camera = CameraInput {
            prst: "perspectiveFront".to_string(),
            fov: Some(90.0),
            zoom: 2.0,
            rot: None,
            units_per_inch: 96.0,
            scene_depth: 0.0,
        };
        let quad = compute_scene3d_quad(&camera, 200.0, 100.0, 0.0);
        assert!(!quad.is_identity);
        assert!(quad.is_affine);
        for (got, want) in quad
            .corners
            .iter()
            .zip([(-100.0, -50.0), (300.0, -50.0), (300.0, 150.0), (-100.0, 150.0)])
        {
            assert!((got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9);
        }
    }

    #[test]
    fn units_per_inch_drive_the_fixed_physical_lens() {
        // ref scene3d-camera.ts:283-333: d = 6.28 in · unitsPerInch /
        // tan(fov/2), independent of the shape size. Doubling the unit count
        // halves the foreshortening of the tilted face.
        let camera = |units| CameraInput {
            prst: "perspectiveAbove".to_string(),
            fov: None,
            zoom: 1.0,
            rot: None,
            units_per_inch: units,
            scene_depth: 0.0,
        };
        let near = compute_scene3d_quad(&camera(96.0), 200.0, 100.0, 0.0);
        let far = compute_scene3d_quad(&camera(192.0), 200.0, 100.0, 0.0);
        let top = |q: &Scene3dQuad| (q.corners[1].0 - q.corners[0].0).abs();
        let bottom = |q: &Scene3dQuad| (q.corners[2].0 - q.corners[3].0).abs();
        assert!((top(&near) - 197.677_412).abs() < 1e-6);
        assert!((bottom(&near) - 202.377_815).abs() < 1e-6);
        assert!((top(&far) - 198.831_924).abs() < 1e-6);
        assert!((bottom(&far) - 201.181_882).abs() < 1e-6);
        assert!(top(&far) > top(&near) && bottom(&far) < bottom(&near));
    }

    #[test]
    fn identity_gate_accounts_for_zoom_and_rear_depth() {
        let front = CameraInput {
            prst: "orthographicFront".to_string(),
            ..CameraInput::default()
        };
        assert!(!is_scene3d_non_identity(&front, Some(100.0), Some(100.0), 0.0));
        let zoomed = CameraInput { zoom: 2.0, ..front.clone() };
        assert!(is_scene3d_non_identity(&zoomed, Some(100.0), Some(100.0), 0.0));
        // An oblique preset keeps the front plane but shears the rear plane:
        // it only becomes visible once the solid has an extrusion depth.
        let oblique = CameraInput {
            prst: "obliqueBottom".to_string(),
            ..CameraInput::default()
        };
        assert!(!is_scene3d_non_identity(&oblique, Some(100.0), Some(100.0), 0.0));
        assert!(is_scene3d_non_identity(&oblique, Some(100.0), Some(100.0), 40.0));
        // A face-on modern perspective camera contracts the rear plane inside
        // the front face: the hidden surface adds no visible wall, so it must
        // not introduce another front-face resampling (ref
        // `isScene3dNonIdentity`, scene3d-camera.ts:486-490).
        let perspective = CameraInput {
            prst: "perspectiveFront".to_string(),
            fov: Some(45.0),
            ..CameraInput::default()
        };
        assert!(!is_scene3d_non_identity(&perspective, Some(100.0), Some(100.0), 0.0));
        assert!(!is_scene3d_non_identity(&perspective, Some(100.0), Some(100.0), 40.0));
        // Tilting the same lens exposes the rear plane and the front quad.
        let tilted = CameraInput {
            rot: Some(Rot3d { lat: 0.0, lon: 150.0, rev: 0.0 }),
            ..perspective.clone()
        };
        assert!(is_scene3d_non_identity(&tilted, Some(100.0), Some(100.0), 40.0));
        // The geometry-free form uses the mathematical identity test.
        assert!(!is_scene3d_non_identity(&front, None, None, 0.0));
        assert!(is_scene3d_non_identity(&zoomed, None, None, 0.0));
    }
}
