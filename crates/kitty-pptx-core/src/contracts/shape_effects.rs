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
    pub end_opacity: f32,
    pub distance_emu: i64,
    pub direction_deg: f64,
}

/// 3D Scene camera and lighting rig parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorScene3d {
    pub camera_type: String,
    pub fov_deg: f32,
    pub light_rig_type: String,
    pub light_direction_deg: f32,
}

/// 3D Shape bevel and extrusion parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorShape3d {
    pub extrusion_depth_emu: i64,
    pub extrusion_color: Option<[u8; 4]>,
    pub contour_width_emu: i64,
    pub contour_color: Option<[u8; 4]>,
    pub bevel_top_width_emu: i64,
    pub bevel_top_height_emu: i64,
    pub bevel_top_type: String,
}
