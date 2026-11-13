//! Stage 1 extraction of DrawingML shape effects into the vector IR.
//!
//! Maps `pptx-model` `spPr` effect structures (`<a:effectLst>`, `<a:scene3d>`,
//! `<a:sp3d>`) onto the resolution-independent [`ShapeEffects`] contract.
//! All lengths stay in EMU; only colours and percentages are normalised.

use crate::compiler::parse_hex_color;
use crate::contracts::{
    ShapeEffects, VectorGlow, VectorInnerShadow, VectorOuterShadow, VectorReflection,
    VectorRot3d, VectorScene3d, VectorShape3d, VectorSoftEdge,
};

/// Extract every supported effect component from a shape's `spPr`.
pub fn extract_shape_effects(shape: &pptx_model::ShapeElement) -> ShapeEffects {
    ShapeEffects {
        outer_shadow: shape.shadow.as_ref().map(outer_shadow),
        inner_shadow: shape.inner_shadow.as_ref().map(inner_shadow),
        glow: shape.glow.as_ref().map(glow),
        soft_edge: shape.soft_edge.as_ref().map(|e| VectorSoftEdge { radius_emu: e.radius }),
        reflection: shape.reflection.as_ref().map(reflection),
        scene_3d: shape.scene3d.as_ref().map(scene_3d),
        shape_3d: shape.sp3d.as_ref().map(shape_3d),
    }
}

/// Resolve a model colour string plus authored opacity to straight RGBA8.
fn color_with_alpha(hex: &str, alpha: f64) -> [u8; 4] {
    let [r, g, b, base_alpha] = parse_hex_color(hex);
    let scaled = (base_alpha as f64 / 255.0 * alpha).clamp(0.0, 1.0);
    [r, g, b, (scaled * 255.0).round().clamp(0.0, 255.0) as u8]
}

fn outer_shadow(shadow: &ooxml_common::effect::Shadow) -> VectorOuterShadow {
    VectorOuterShadow {
        color: color_with_alpha(&shadow.color, shadow.alpha),
        blur_radius_emu: shadow.blur,
        distance_emu: shadow.dist,
        direction_deg: shadow.dir,
        scale_x: shadow.sx.unwrap_or(1.0) as f32,
        scale_y: shadow.sy.unwrap_or(1.0) as f32,
        skew_x_deg: shadow.kx.unwrap_or(0.0) as f32,
        skew_y_deg: shadow.ky.unwrap_or(0.0) as f32,
        rotate_with_shape: shadow.rot_with_shape.unwrap_or(false),
    }
}

fn inner_shadow(shadow: &ooxml_common::effect::Shadow) -> VectorInnerShadow {
    VectorInnerShadow {
        color: color_with_alpha(&shadow.color, shadow.alpha),
        blur_radius_emu: shadow.blur,
        distance_emu: shadow.dist,
        direction_deg: shadow.dir,
    }
}

fn glow(glow: &ooxml_common::effect::Glow) -> VectorGlow {
    VectorGlow {
        color: color_with_alpha(&glow.color, glow.alpha),
        radius_emu: glow.radius,
    }
}

fn reflection(reflection: &ooxml_common::effect::Reflection) -> VectorReflection {
    VectorReflection {
        blur_radius_emu: reflection.blur,
        start_opacity: reflection.st_a as f32,
        start_pos: reflection.st_pos as f32,
        end_opacity: reflection.end_a as f32,
        end_pos: reflection.end_pos as f32,
        distance_emu: reflection.dist,
        direction_deg: reflection.dir,
        scale_x: reflection.sx as f32,
        scale_y: reflection.sy as f32,
        // CT_ReflectionEffect defaults `algn` to bottom centre; the parser does
        // not carry the attribute yet.
        align: "b".to_string(),
    }
}

fn scene_3d(scene: &pptx_model::Scene3d) -> VectorScene3d {
    let (light_rig_type, light_direction_deg) = match scene.light_rig.as_ref() {
        Some(rig) => (rig.rig.clone(), light_direction_deg(&rig.dir)),
        None => ("threePt".to_string(), light_direction_deg("t")),
    };
    VectorScene3d {
        camera_type: scene.camera.prst.clone(),
        fov_deg: scene.camera.fov.unwrap_or(0.0) as f32,
        zoom: scene.camera.zoom.unwrap_or(1.0) as f32,
        rot: scene.camera.rot.as_ref().map(|rot| VectorRot3d {
            lat_deg: rot.lat as f32,
            lon_deg: rot.lon as f32,
            rev_deg: rot.rev as f32,
        }),
        light_rig_type,
        light_direction_deg,
    }
}

fn shape_3d(sp3d: &pptx_model::Sp3d) -> VectorShape3d {
    let bevel = sp3d.bevel_t.as_ref();
    let bottom = sp3d.bevel_b.as_ref();
    VectorShape3d {
        extrusion_depth_emu: sp3d.extrusion_h,
        extrusion_color: sp3d.extrusion_clr.as_deref().map(|c| color_with_alpha(c, 1.0)),
        contour_width_emu: sp3d.contour_w,
        contour_color: sp3d.contour_clr.as_deref().map(|c| color_with_alpha(c, 1.0)),
        bevel_top_width_emu: bevel.map(|b| b.w).unwrap_or(0),
        bevel_top_height_emu: bevel.map(|b| b.h).unwrap_or(0),
        bevel_top_type: bevel.map(|b| b.prst.clone()).unwrap_or_default(),
        bevel_bottom_width_emu: bottom.map(|b| b.w).unwrap_or(0),
        bevel_bottom_height_emu: bottom.map(|b| b.h).unwrap_or(0),
        bevel_bottom_type: bottom.map(|b| b.prst.clone()).unwrap_or_default(),
        prst_material: sp3d.prst_material.clone(),
    }
}

/// How a shape's 2-D transform composes with a `scene3d` camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene3dShapeTransform {
    /// The element's ordinary 2-D rotation, applied *after* the camera.
    pub outer_rotation: f64,
    /// Horizontal flip, applied *inside* the camera (local face).
    pub local_flip_h: bool,
    /// Vertical flip, applied *inside* the camera (local face).
    pub local_flip_v: bool,
}

/// Split a shape transform into the part that warps the authored face and the
/// part that stays outside the projection.
///
/// DrawingML camera projection consumes the shape-local face *after* `xfrm`
/// flips, while the ordinary 2-D rotation remains outside the projected face:
/// a post-projection flip would mirror the camera footprint itself
/// (ref `renderer.ts:3570-3583` `splitScene3dShapeTransform`).
pub fn split_scene3d_shape_transform(
    rotation: f64,
    flip_h: bool,
    flip_v: bool,
) -> Scene3dShapeTransform {
    Scene3dShapeTransform {
        outer_rotation: rotation,
        local_flip_h: flip_h,
        local_flip_v: flip_v,
    }
}

/// Map an `ST_LightRigDirection` to an azimuth clockwise from East in screen
/// coordinates (x right, y down), where the vector points toward the light.
pub fn light_direction_deg(dir: &str) -> f32 {
    match dir.to_ascii_lowercase().as_str() {
        "t" => 270.0,
        "tl" => 225.0,
        "l" => 180.0,
        "bl" => 135.0,
        "b" => 90.0,
        "br" => 45.0,
        "r" => 0.0,
        "tr" => 315.0,
        _ => 270.0,
    }
}
