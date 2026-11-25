//! Reference-faithful DrawingML 3-D camera projection (camera *face* geometry).
//!
//! Port of `packages/core/src/shape/scene3d-camera.ts` @ `1f5c6e1d`:
//! * `:355-446` `computeScene3dQuad` (perspective divide, parallel shear, zoom),
//! * `:502-527` `computeDepthOffset` (screen displacement of the object's −Z),
//! * `:151-206` the observed `ST_PresetCameraType` table,
//! * `:283-333` `cameraDistance` incl. the 2 % view-Z grace gap.
//!
//! Values are `f64` so callers can compare projected corners against analytic
//! pinhole bounds (the reference is JavaScript). The raster paint path warps
//! through this quad (`rasterizer::shape_effect::projection`); the SVG backend
//! and the flat fallback use [`crate::scene3d::camera_affine`], the affine fit
//! of the same quad (documented SVG degradation, see
//! `tests/parity/coverage/shapes-b.md`).

use crate::contracts::VectorScene3d;

/// Physical scene framing of the modern perspective presets, in inches
/// (ref `scene3d-camera.ts:151` `MODERN_SCENE_INCHES`, an Office observation).
#[allow(clippy::approx_constant)] // Observed 6.28-inch framing; not `TAU`.
const MODERN_SCENE_INCHES: f64 = 6.28;
/// Lens retained only for the unrecognized `perspective*` fallback
/// (ref `scene3d-camera.ts:181`).
const LEGACY_PERSP_FOV: f64 = 26.0;

/// 3×3 matrix in row-major order.
pub type Mat3 = [f64; 9];

/// `<a:rot>` sphere coordinates in degrees (ECMA-376 §20.1.5.11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rot3d {
    pub lat: f64,
    pub lon: f64,
    pub rev: f64,
}

/// Camera inputs the projection math needs (ref `scene3d-camera.ts:35-56`).
#[derive(Clone, Debug, PartialEq)]
pub struct CameraInput {
    /// `ST_PresetCameraType` name.
    pub prst: String,
    /// Field-of-view override in degrees; `None` = preset default.
    pub fov: Option<f64>,
    /// Zoom as a unit ratio (1.0 = 100 %).
    pub zoom: f64,
    /// Camera rotation override; `None` = preset base orientation.
    pub rot: Option<Rot3d>,
    /// Projection units per document inch (renderer context, not OOXML).
    pub units_per_inch: f64,
    /// Extent of the rendered solid along object −Z, in projection units.
    pub scene_depth: f64,
}

impl Default for CameraInput {
    fn default() -> Self {
        Self {
            prst: "orthographicFront".to_string(),
            fov: None,
            zoom: 1.0,
            rot: None,
            units_per_inch: 96.0,
            scene_depth: 0.0,
        }
    }
}

impl CameraInput {
    /// Build the camera from the resolution-independent scene contract.
    ///
    /// `unitsPerInch` is renderer context, not OOXML: the IR stores EMU
    /// lengths, so the compiler writes 914 400 (ref `scene3d-camera.ts:24-34`).
    pub fn from_scene(scene: &VectorScene3d) -> Self {
        Self {
            prst: scene.camera_type.clone(),
            // The IR stores 0.0 for "no authored fov" (`a:camera@fov` absent).
            fov: (scene.fov_deg > 0.0).then_some(scene.fov_deg as f64),
            zoom: if scene.zoom > 0.0 { scene.zoom as f64 } else { 1.0 },
            rot: scene.rot.map(|rot| Rot3d {
                lat: rot.lat_deg as f64,
                lon: rot.lon_deg as f64,
                rev: rot.rev_deg as f64,
            }),
            units_per_inch: if scene.units_per_inch > 0.0 {
                scene.units_per_inch as f64
            } else {
                96.0
            },
            scene_depth: 0.0,
        }
    }

    /// Same camera re-expressed in device pixels: `w`, `h` and `depth` are
    /// multiplied by the caller's EMU→device scale, so the fixed physical
    /// framing stays identical while the lens works in device units.
    pub fn scaled(mut self, scale: f64) -> Self {
        self.units_per_inch *= scale;
        self.scene_depth *= scale;
        self
    }

    /// Same camera with the solid's rear extent bound (ref `projectedSurface`).
    pub fn with_scene_depth(mut self, depth: f64) -> Self {
        if depth > self.scene_depth {
            self.scene_depth = depth;
        }
        self
    }
}

/// The four projected corners (shape-local CSS px) plus the affine/identity
/// classification of the resulting map (ref `Scene3dQuad`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene3dQuad {
    /// TL, TR, BR, BL — the images of `(0,0) (w,0) (w,h) (0,h)`.
    pub corners: [(f64, f64); 4],
    pub is_affine: bool,
    pub is_identity: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Perspective,
    Orthographic,
}

#[derive(Clone, Copy, Debug)]
struct PresetDef {
    kind: Kind,
    base_lat: f64,
    base_lon: f64,
    base_rev: f64,
    fov_deg: f64,
    scene_inches: Option<f64>,
    shear_x: f64,
    shear_y: f64,
}

fn parallel(lat: f64, lon: f64, rev: f64, shear_x: f64, shear_y: f64) -> PresetDef {
    PresetDef {
        kind: Kind::Orthographic,
        base_lat: lat,
        base_lon: lon,
        base_rev: rev,
        fov_deg: 0.0,
        scene_inches: None,
        shear_x,
        shear_y,
    }
}

fn perspective(lat: f64, lon: f64, rev: f64, fov_deg: f64) -> PresetDef {
    PresetDef {
        kind: Kind::Perspective,
        base_lat: lat,
        base_lon: lon,
        base_rev: rev,
        fov_deg,
        scene_inches: Some(MODERN_SCENE_INCHES),
        shear_x: 0.0,
        shear_y: 0.0,
    }
}

const OBLIQUE: f64 = 0.3 / std::f64::consts::SQRT_2;

/// `ST_PresetCameraType` orientation table (ref `scene3d-camera.ts:190-235`).
///
/// +X right, +Y down, +Z toward the viewer. Explicit `rot` replaces these
/// angles while retaining the preset's projection kind and shear.
fn preset_def(prst: &str) -> PresetDef {
    match prst {
        "isometricBottomDown" => parallel(35.4, 314.7, 299.8, 0.0, 0.0),
        "isometricBottomUp" => parallel(35.4, 45.3, 60.2, 0.0, 0.0),
        "isometricLeftDown" => parallel(35.0, 45.0, 0.0, 0.0, 0.0),
        "isometricLeftUp" => parallel(325.0, 45.0, 0.0, 0.0, 0.0),
        "isometricOffAxis1Left" => parallel(18.0, 64.0, 0.0, 0.0, 0.0),
        "isometricOffAxis1Right" => parallel(18.0, 334.0, 0.0, 0.0, 0.0),
        "isometricOffAxis1Top" => parallel(301.3, 306.5, 57.6, 0.0, 0.0),
        "isometricOffAxis2Left" => parallel(18.0, 26.0, 0.0, 0.0, 0.0),
        "isometricOffAxis2Right" => parallel(18.0, 296.0, 0.0, 0.0, 0.0),
        "isometricOffAxis2Top" => parallel(301.3, 53.5, 302.4, 0.0, 0.0),
        "isometricOffAxis3Bottom" => parallel(58.7, 306.5, 302.4, 0.0, 0.0),
        "isometricOffAxis3Left" => parallel(342.0, 64.0, 0.0, 0.0, 0.0),
        "isometricOffAxis3Right" => parallel(342.0, 334.0, 0.0, 0.0, 0.0),
        "isometricOffAxis4Bottom" => parallel(58.7, 53.5, 57.6, 0.0, 0.0),
        "isometricOffAxis4Left" => parallel(342.0, 26.0, 0.0, 0.0, 0.0),
        "isometricOffAxis4Right" => parallel(342.0, 296.0, 0.0, 0.0, 0.0),
        "isometricRightDown" => parallel(325.0, 315.0, 0.0, 0.0, 0.0),
        "isometricRightUp" => parallel(35.0, 315.0, 0.0, 0.0, 0.0),
        "isometricTopDown" => parallel(324.6, 45.3, 299.8, 0.0, 0.0),
        "isometricTopUp" => parallel(324.6, 314.7, 60.2, 0.0, 0.0),
        "obliqueBottom" => parallel(0.0, 0.0, 0.0, 0.0, -0.3),
        "obliqueBottomLeft" => parallel(0.0, 0.0, 0.0, OBLIQUE, -OBLIQUE),
        "obliqueBottomRight" => parallel(0.0, 0.0, 0.0, -OBLIQUE, -OBLIQUE),
        "obliqueLeft" => parallel(0.0, 0.0, 0.0, 0.3, 0.0),
        "obliqueRight" => parallel(0.0, 0.0, 0.0, -0.3, 0.0),
        "obliqueTop" => parallel(0.0, 0.0, 0.0, 0.0, 0.3),
        "obliqueTopLeft" => parallel(0.0, 0.0, 0.0, OBLIQUE, OBLIQUE),
        "obliqueTopRight" => parallel(0.0, 0.0, 0.0, -OBLIQUE, OBLIQUE),
        "orthographicFront" => parallel(0.0, 0.0, 0.0, 0.0, 0.0),
        "perspectiveAbove" => perspective(340.0, 0.0, 0.0, 45.0),
        "perspectiveAboveLeftFacing" => perspective(39.3, 14.3, 341.1, 45.0),
        "perspectiveAboveRightFacing" => perspective(39.3, 345.7, 18.9, 45.0),
        "perspectiveBelow" => perspective(20.0, 0.0, 0.0, 45.0),
        "perspectiveContrastingLeftFacing" => perspective(10.4, 43.9, 356.4, 45.0),
        "perspectiveContrastingRightFacing" => perspective(10.4, 316.1, 3.6, 45.0),
        "perspectiveFront" => perspective(0.0, 0.0, 0.0, 45.0),
        "perspectiveHeroicExtremeLeftFacing" => perspective(8.1, 34.5, 357.1, 80.0),
        "perspectiveHeroicExtremeRightFacing" => perspective(8.1, 325.5, 2.9, 80.0),
        "perspectiveHeroicLeftFacing" => perspective(349.0, 14.3, 2.6, 45.0),
        "perspectiveHeroicRightFacing" => perspective(349.0, 345.7, 357.4, 45.0),
        "perspectiveLeft" => perspective(0.0, 20.0, 0.0, 45.0),
        "perspectiveRelaxed" => perspective(309.6, 0.0, 0.0, 45.0),
        "perspectiveRelaxedModerately" => perspective(324.8, 0.0, 0.0, 45.0),
        "perspectiveRight" => perspective(0.0, 340.0, 0.0, 45.0),
        // Unknown perspective names keep the neutral lens/framing; every other
        // unknown name keeps the old parallel front fallback.
        _ if prst.starts_with("perspective") => PresetDef {
            scene_inches: None,
            fov_deg: LEGACY_PERSP_FOV,
            ..perspective(0.0, 0.0, 0.0, LEGACY_PERSP_FOV)
        },
        _ => parallel(0.0, 0.0, 0.0, 0.0, 0.0),
    }
}

fn mul3(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[i * 3 + k] * b[k * 3 + j];
            }
            r[i * 3 + j] = s;
        }
    }
    r
}

fn rot_x(deg: f64) -> Mat3 {
    let (s, c) = deg.to_radians().sin_cos();
    [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c]
}

fn rot_y(deg: f64) -> Mat3 {
    let (s, c) = deg.to_radians().sin_cos();
    [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c]
}

fn rot_z(deg: f64) -> Mat3 {
    let (s, c) = deg.to_radians().sin_cos();
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

fn apply(m: &Mat3, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    (
        m[0] * x + m[1] * y + m[2] * z,
        m[3] * x + m[4] * y + m[5] * z,
        m[6] * x + m[7] * y + m[8] * z,
    )
}

/// Object rotation `Rz(-rev) · Rx(-lat) · Ry(-lon)` with an explicit `rot`
/// replacing the modelled preset orientation (ref `scene3d-camera.ts:283-292`).
pub fn camera_rotation(prst: &str, rot: Option<Rot3d>) -> Mat3 {
    let def = preset_def(prst);
    let (lat, lon, rev) = match rot {
        Some(rot) => (rot.lat, rot.lon, rot.rev),
        None => (def.base_lat, def.base_lon, def.base_rev),
    };
    mul3(&rot_z(-rev), &mul3(&rot_x(-lat), &rot_y(-lon)))
}

/// Camera-to-scene distance for a perspective lens, including the 2 % view-Z
/// grace gap that keeps a rear face from crossing the near plane
/// (ref `scene3d-camera.ts:311-352`).
fn camera_distance(camera: &CameraInput, def: &PresetDef, rotation: &Mat3, w: f64, h: f64, surface_depth: f64) -> f64 {
    let fov_deg = camera.fov.unwrap_or(def.fov_deg);
    let fov = fov_deg.clamp(1.0, 179.0).to_radians();
    let numerator = match def.scene_inches {
        Some(inches) => inches * camera.units_per_inch,
        None => w.max(h) / 2.0,
    };
    let distance = numerator / (fov / 2.0).tan();
    let depth = surface_depth.max(camera.scene_depth).max(0.0);
    let half_range = rotation[6].abs() * w / 2.0 + rotation[7].abs() * h / 2.0;
    let rear_z = -rotation[8] * depth;
    let nearest = half_range + rear_z.max(0.0);
    let volume = 2.0 * half_range + rear_z.abs();
    distance.max(nearest + 0.02 * volume)
}

/// Project the `w × h` planar face through the camera; corners are shape-local
/// CSS px at `(0,0)-(w,0)-(w,h)-(0,h)` (ref `computeScene3dQuad`).
///
/// `surface_depth` is the back surface's distance along object −Z in the same
/// units as `w`/`h` (MS-OI29500 §2.1.1268: `bevelB` sits at the extrusion base).
pub fn compute_scene3d_quad(camera: &CameraInput, w: f64, h: f64, surface_depth: f64) -> Scene3dQuad {
    if w <= 0.0 || h <= 0.0 {
        return Scene3dQuad {
            corners: [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
            is_affine: true,
            is_identity: true,
        };
    }
    let def = preset_def(&camera.prst);
    let r = camera_rotation(&camera.prst, camera.rot);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let src = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
    let zoom = camera.zoom;

    let distance = if def.kind == Kind::Perspective {
        camera_distance(camera, &def, &r, w, h, surface_depth)
    } else {
        0.0
    };
    let mut projected = [(0.0, 0.0); 4];
    for (slot, (sx, sy)) in projected.iter_mut().zip(src) {
        let (rx, ry, rz) = apply(&r, sx, sy, -surface_depth);
        let (px, py) = match def.kind {
            // Camera at +Z = d looking toward −Z: divide by the remaining depth.
            Kind::Perspective => {
                let f = distance / (distance - rz);
                (rx * f, ry * f)
            }
            // Parallel projection, including the oblique directional depth shear.
            Kind::Orthographic => (rx + rz * def.shear_x, ry + rz * def.shear_y),
        };
        *slot = (w / 2.0 + px * zoom, h / 2.0 + py * zoom);
    }

    // A quad is a parallelogram iff its diagonals bisect each other.
    let eps = 1e-3 * w.max(h);
    let sum_diag_x = projected[0].0 + projected[2].0 - (projected[1].0 + projected[3].0);
    let sum_diag_y = projected[0].1 + projected[2].1 - (projected[1].1 + projected[3].1);
    let is_affine = sum_diag_x.abs() < eps && sum_diag_y.abs() < eps;
    let reference = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    let is_identity = projected
        .iter()
        .zip(reference)
        .all(|(p, r)| (p.0 - r.0).abs() <= eps && (p.1 - r.1).abs() <= eps);
    Scene3dQuad {
        corners: projected,
        is_affine,
        is_identity,
    }
}

/// Whether the camera `prst` is one of the modelled modern presets (they carry
/// a physical 6.28-inch scene framing, ref `scene3d-camera.ts:151`).
fn is_modern_perspective(camera: &CameraInput) -> bool {
    preset_def(&camera.prst).scene_inches.is_some()
}

/// True when a scene3d camera visibly changes the shape, i.e. the renderer
/// should take the projection path (ref `isScene3dNonIdentity`,
/// `scene3d-camera.ts:472-522`).
///
/// `w`/`h` are the leaf face dimensions in the camera's units; `depth` is the
/// solid's rear extent along object −Z (extrusion height). A face-on camera
/// whose rear plane contracts inside the front face is an identity for the
/// planar compositor and keeps the cheaper flat path.
pub fn is_scene3d_non_identity(camera: &CameraInput, w: Option<f64>, h: Option<f64>, depth: f64) -> bool {
    if let (Some(w), Some(h)) = (w, h) {
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let camera = camera.clone().with_scene_depth(depth);
        if !compute_scene3d_quad(&camera, w, h, 0.0).is_identity {
            return true;
        }
        if depth <= 0.0 {
            return false;
        }
        let back = compute_scene3d_quad(&camera, w, h, depth);
        if !is_modern_perspective(&camera) {
            return !back.is_identity;
        }
        let eps = 1e-3 * w.max(h);
        return back
            .corners
            .iter()
            .any(|(x, y)| *x < -eps || *y < -eps || *x > w + eps || *y > h + eps);
    }
    // Geometry-free callers use a conservative mathematical identity test
    // instead of borrowing a shape's numerical pixel tolerance.
    let def = preset_def(&camera.prst);
    let r = camera_rotation(&camera.prst, camera.rot);
    let mut terms: Vec<f64> = match def.kind {
        Kind::Perspective => vec![r[0] - 1.0, r[1], r[3], r[4] - 1.0, r[6], r[7]],
        Kind::Orthographic => vec![
            r[0] + def.shear_x * r[6] - 1.0,
            r[1] + def.shear_x * r[7],
            r[3] + def.shear_y * r[6],
            r[4] + def.shear_y * r[7] - 1.0,
        ],
    };
    if depth > 0.0 {
        if def.kind == Kind::Perspective {
            return true;
        }
        terms.push(r[2] + def.shear_x * r[8]);
        terms.push(r[5] + def.shear_y * r[8]);
    }
    (camera.zoom - 1.0).abs() > 1e-12 || terms.iter().any(|value| value.abs() > 1e-12)
}

/// Internal surface frame for the bitmap bevel compositor: the object
/// rotation and whether the camera looks at the back face, including the
/// parallel oblique shear (ref `scene3dSurfaceFrame`, `scene3d-camera.ts:524-533`).
pub fn scene3d_surface_frame(camera: &CameraInput) -> (Mat3, bool) {
    let def = preset_def(&camera.prst);
    let rotation = camera_rotation(&camera.prst, camera.rot);
    let facing = rotation[8] - def.shear_x * rotation[2] - def.shear_y * rotation[5];
    (rotation, facing < 0.0)
}

/// Whether the camera looks at the object's *back* face (negative projected
/// winding of the front plane), including the parallel oblique shear
/// (ref `scene3d-camera.ts:487-491` `scene3dSurfaceFrame`).
pub fn is_back_facing(camera: &CameraInput) -> bool {
    scene3d_surface_frame(camera).1
}

/// Screen-space displacement (shape-local px, +X right / +Y down) of pushing a
/// point at the shape centre `depth_px` along the object's −Z axis
/// (ref `computeDepthOffset`, the centre approximation of the wall sweep).
pub fn compute_depth_offset(camera: &CameraInput, w: f64, h: f64, depth_px: f64) -> (f64, f64) {
    if w <= 0.0 || h <= 0.0 || depth_px == 0.0 {
        return (0.0, 0.0);
    }
    let def = preset_def(&camera.prst);
    let r = camera_rotation(&camera.prst, camera.rot);
    let distance = if def.kind == Kind::Perspective {
        camera_distance(camera, &def, &r, w, h, depth_px)
    } else {
        0.0
    };
    let project = |object_z: f64| -> (f64, f64) {
        let (rx, ry, rz) = apply(&r, 0.0, 0.0, object_z);
        if def.kind == Kind::Perspective {
            let f = distance / (distance - rz);
            (rx * f * camera.zoom, ry * f * camera.zoom)
        } else {
            (
                (rx + rz * def.shear_x) * camera.zoom,
                (ry + rz * def.shear_y) * camera.zoom,
            )
        }
    };
    let (x0, y0) = project(0.0);
    let (x1, y1) = project(-depth_px);
    (x1 - x0, y1 - y0)
}
