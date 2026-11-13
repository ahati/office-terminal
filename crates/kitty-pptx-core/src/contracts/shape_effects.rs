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
    pub light_direction_deg: f32,
}

fn default_zoom() -> f32 {
    1.0
}

/// 3D Shape bevel (`bevelT`/`bevelB`, ECMA-376 §20.1.5.3) and extrusion
/// parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorShape3d {
    pub extrusion_depth_emu: i64,
    pub extrusion_color: Option<[u8; 4]>,
    pub contour_width_emu: i64,
    pub contour_color: Option<[u8; 4]>,
    pub bevel_top_width_emu: i64,
    pub bevel_top_height_emu: i64,
    pub bevel_top_type: String,
    /// `bevelB@w` — MS-OI29500 §2.1.1268 places this lip at the extrusion base.
    #[serde(default)]
    pub bevel_bottom_width_emu: i64,
    /// `bevelB@h`.
    #[serde(default)]
    pub bevel_bottom_height_emu: i64,
    /// `bevelB@prst`; empty = `circle`.
    #[serde(default)]
    pub bevel_bottom_type: String,
    /// `sp3d@prstMaterial` (`ST_PresetMaterialType`); empty = `matte`.
    #[serde(default)]
    pub prst_material: String,
}
