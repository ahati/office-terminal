use serde::{Deserialize, Serialize};

/// DrawingML shape effect collection attached to vector paths and images.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShapeEffects {
    pub outer_shadow: Option<VectorOuterShadow>,
    pub inner_shadow: Option<VectorInnerShadow>,
    pub glow: Option<VectorGlow>,
    pub soft_edge: Option<VectorSoftEdge>,
    pub reflection: Option<VectorReflection>,
    pub scene_3d: Option<VectorScene3d>,
    pub shape_3d: Option<VectorShape3d>,
}

impl ShapeEffects {
    pub fn is_empty(&self) -> bool {
        self.outer_shadow.is_none()
            && self.inner_shadow.is_none()
            && self.glow.is_none()
            && self.soft_edge.is_none()
            && self.reflection.is_none()
            && self.scene_3d.is_none()
            && self.shape_3d.is_none()
    }
}

/// Outer drop shadow parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorOuterShadow {
    pub color: [u8; 4],
    pub blur_radius_emu: i64,
    pub distance_emu: i64,
    pub direction_deg: f64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub skew_x_deg: f32,
    pub skew_y_deg: f32,
    pub rotate_with_shape: bool,
}

impl Default for VectorOuterShadow {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 128],
            blur_radius_emu: 38100, // 4 pt
            distance_emu: 25400,    // 2 pt
            direction_deg: 45.0,
            scale_x: 1.0,
            scale_y: 1.0,
            skew_x_deg: 0.0,
            skew_y_deg: 0.0,
            rotate_with_shape: false,
        }
    }
}

/// Inner inset shadow parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorInnerShadow {
    pub color: [u8; 4],
    pub blur_radius_emu: i64,
    pub distance_emu: i64,
    pub direction_deg: f64,
}

/// Outer glow halo parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorGlow {
    pub color: [u8; 4],
    pub radius_emu: i64,
}

/// Edge feathering / soft edge parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorSoftEdge {
    pub radius_emu: i64,
}

/// Reflection parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorReflection {
    pub blur_radius_emu: i64,
    pub start_opacity: f32,
    /// `stPos` — where the fade starts, as a fraction (0..1) of the mirrored
    /// band measured from the edge touching the shape (ref effects.ts:612).
    pub start_pos: f32,
    pub end_opacity: f32,
    /// `endPos` — where the fade reaches `end_opacity`, same measure as `start_pos`.
    pub end_pos: f32,
    pub distance_emu: i64,
    /// `dir` in degrees clockwise from East; 90° mirrors below the shape.
    pub direction_deg: f64,
    /// `sx` magnitude: width of the mirrored copy relative to the shape.
    pub scale_x: f32,
    /// `sy` magnitude: height of the mirrored copy relative to the shape.
    pub scale_y: f32,
    /// `algn` (`ST_RectAlignment`) pivot for the mirrored copy; `b` default.
    pub align: String,
}

impl Default for VectorReflection {
    fn default() -> Self {
        Self {
            blur_radius_emu: 0,
            start_opacity: 1.0,
            start_pos: 0.0,
            end_opacity: 0.0,
            end_pos: 1.0,
            distance_emu: 0,
            // The reference mirrors downward and treats 90° as its default
            // (ref packages/core/src/shape/effects.ts:553-559).
            direction_deg: 90.0,
            scale_x: 1.0,
            scale_y: 1.0,
            align: "b".to_string(),
        }
    }
}

/// Camera rotation override — ECMA-376 §20.1.5.11 (`CT_SphereCoords`).
///
/// An authored `<a:rot>` replaces the preset camera's base orientation; all
/// three angles are in degrees (`lat` = X tilt, `lon` = Y turn, `rev` = view-Z
/// spin).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorRot3d {
    pub lat_deg: f32,
    pub lon_deg: f32,
    pub rev_deg: f32,
}

/// 3D Scene camera and lighting rig parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorScene3d {
    pub camera_type: String,
    /// Authored `a:camera@fov` in degrees; `0.0` = preset default.
    pub fov_deg: f32,
    /// `a:camera@zoom` as a unit ratio (1.0 = 100 %).
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Authored `a:camera/a:rot`; `None` = preset base orientation.
    #[serde(default)]
    pub rot: Option<VectorRot3d>,
    pub light_rig_type: String,
    /// `a:lightRig@dir` (`ST_LightRigDirection`); empty/absent = `t`.
    #[serde(default)]
    pub light_dir: String,
    /// Authored `a:lightRig/a:rot`; `None` = the rig's default direction.
    #[serde(default)]
    pub light_rot: Option<VectorRot3d>,
    /// Resolved light azimuth clockwise from East in screen coordinates
    /// (x right, y down). The shading pass consumes this single angle; the
    /// authored `light_dir`/`light_rot` are retained for the per-rig tables.
    pub light_direction_deg: f32,
    /// Projection units per document inch (renderer context, not OOXML).
    /// EMU-based paths use 914 400; CSS-pixel paths use 96 (ref
    /// `scene3d-camera.ts:24-34` `CameraInput.unitsPerInch`).
    #[serde(default = "default_units_per_inch")]
    pub units_per_inch: f32,
}

fn default_zoom() -> f32 {
    1.0
}

fn default_units_per_inch() -> f32 {
    // The vector IR stores lengths in EMU (AGENTS.md §4); the reference works
    // in CSS px and defaults to 96.
    914_400.0
}

/// One `<a:bevel>` lip (ECMA-376 §20.1.5.3 `CT_Bevel`): band width/height in
/// EMU and the `ST_BevelPresetType` profile name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorBevel {
    pub width_emu: i64,
    pub height_emu: i64,
    pub preset: String,
}

/// 3D Shape bevel (`bevelT`/`bevelB`, ECMA-376 §20.1.5.3) and extrusion
/// parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorShape3d {
    pub extrusion_depth_emu: i64,
    pub extrusion_color: Option<[u8; 4]>,
    pub contour_width_emu: i64,
    pub contour_color: Option<[u8; 4]>,
    /// `sp3d@prstMaterial` (`ST_PresetMaterialType`); empty = `matte`.
    #[serde(default)]
    pub prst_material: String,
    /// `bevelT` — the front lip.
    #[serde(default)]
    pub bevel_top: Option<VectorBevel>,
    /// `bevelB` — MS-OI29500 §2.1.1268 places this lip at the extrusion base.
    #[serde(default)]
    pub bevel_bottom: Option<VectorBevel>,
}
