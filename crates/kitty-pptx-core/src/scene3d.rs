//! Shared 3D scene mathematics for PRD 08 §3.6.
//!
//! Both render backends (the `tiny-skia` rasterizer and the SVG serializer)
//! consume the resolution-independent [`VectorScene3d`] / [`VectorShape3d`]
//! contracts. This module centralises the camera projection approximation,
//! bevel profile curves, and Blinn-Phong material lighting so both backends
//! produce consistent output.

use crate::contracts::{AffineTransform, VectorScene3d};

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
/// Orthographic presets are exactly affine. Perspective presets are projected
/// through their homography and approximated by the affine that maps the three
/// local box corners `(0,0)`, `(w,0)`, `(0,h)` exactly (PRD 08 §3.6.1,
/// DEGRADED 3D policy).
pub fn camera_affine(
    scene: &VectorScene3d,
    local_w: f32,
    local_h: f32,
    slide_width_emu: i64,
) -> AffineTransform {
    if local_w <= 0.0 || local_h <= 0.0 {
        return AffineTransform::identity();
    }
    let angles = camera_angles(&scene.camera_type);
    let center = (local_w / 2.0, local_h / 2.0);

    let fov = if scene.fov_deg > 0.0 {
        scene.fov_deg
    } else {
        angles.default_fov_deg
    };
    let d_cam = if is_perspective(&scene.camera_type) {
        let half = (fov.clamp(20.0, 120.0) / 2.0).to_radians();
        let d = (slide_width_emu.max(1) as f32) / (2.0 * half.tan());
        Some(d.max(1.0))
    } else {
        None
    };

    let q0 = project_point(0.0, 0.0, center, angles, d_cam);
    let q1 = project_point(local_w, 0.0, center, angles, d_cam);
    let q2 = project_point(0.0, local_h, center, angles, d_cam);

    AffineTransform {
        sx: (q1.0 - q0.0) / local_w,
        ky: (q1.1 - q0.1) / local_w,
        kx: (q2.0 - q0.0) / local_h,
        sy: (q2.1 - q0.1) / local_h,
        tx: q0.0,
        ty: q0.1,
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
            light_rig_type: "threePt".to_string(),
            light_direction_deg: 225.0,
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
        let affine = camera_affine(&scene("isometricTopUp", 0.0), 200.0, 100.0, 9144000);
        // Vertical extent is foreshortened by cos(35.264°) ≈ 0.8165.
        assert!(affine.sy < 0.9 && affine.sy > 0.7);
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
}
