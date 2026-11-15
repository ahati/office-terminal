//! 15 3-D preset material definitions (S7a).
//!
//! Port of `packages/core/src/shape/bevel-lighting.ts` `MATERIALS`.

/// Material parameters governing diffuse, specular, Fresnel, and emissive shading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialDef {
    pub specular_shape: f32,
    pub specular_white: f32,
    pub diffuse_shape: f32,
    pub diffuse_white: f32,
    pub power: f32,
    pub blinn: bool,
    pub diffuse_fresnel: f32,
    pub alpha_fresnel: f32,
    pub diffuse_alpha: f32,
    pub ambient_alpha: f32,
    pub emissive: f32,
}

#[allow(clippy::too_many_arguments)]
const fn mat(
    specular_shape: f32,
    specular_white: f32,
    diffuse_shape: f32,
    diffuse_white: f32,
    power: f32,
    blinn: bool,
    diffuse_fresnel: f32,
    alpha_fresnel: f32,
    diffuse_alpha: f32,
    ambient_alpha: f32,
    emissive: f32,
) -> MaterialDef {
    MaterialDef {
        specular_shape,
        specular_white,
        diffuse_shape,
        diffuse_white,
        power,
        blinn,
        diffuse_fresnel,
        alpha_fresnel,
        diffuse_alpha,
        ambient_alpha,
        emissive,
    }
}

pub static MAT_LEGACY_MATTE: MaterialDef = mat(0.0, 0.0, 1.0, 0.0, 0.0, true, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_LEGACY_PLASTIC: MaterialDef = mat(0.0, 1.0, 1.0, 0.0, 32.0, true, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_LEGACY_METAL: MaterialDef = mat(1.0, 0.0, 2.0 / 3.0, 0.0, 32.0, true, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_LEGACY_WIREFRAME: MaterialDef = mat(0.0, 0.0, 0.0, 0.0, 0.0, false, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_MATTE: MaterialDef = mat(0.0, 0.0, 1.0, 0.0, 0.0, false, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_PLASTIC: MaterialDef = mat(0.0, 0.6, 1.0, 0.0, 12.0, false, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_METAL: MaterialDef = mat(-0.32, 1.32, 1.0, 0.0, 12.0, false, 4.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_WARM_MATTE: MaterialDef = mat(0.0, 0.3, 1.0, 0.0, 8.0, false, 0.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_TRANSLUCENT_POWDER: MaterialDef = mat(0.0, 0.3, 1.0, 0.0, 10.0, true, 2.0, -1.0, 0.7, 0.7, 0.0);
pub static MAT_POWDER: MaterialDef = mat(0.0, 0.3, 0.5, 0.5, 10.0, true, 2.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_DK_EDGE: MaterialDef = mat(0.0, 1.0, 1.0, 0.0, 35.0, false, -2.0, 0.0, 1.0, 1.0, 0.0);
pub static MAT_SOFT_EDGE: MaterialDef = mat(0.0, 1.0, 1.0, 0.0, 35.0, false, 4.0, -10.0, 1.0, 1.0, 0.0);
pub static MAT_CLEAR: MaterialDef = mat(0.0, 0.6, 1.0, 0.0, 20.0, false, -8.0, 1.0, 0.1, 1.0, 0.0);
pub static MAT_FLAT: MaterialDef = mat(0.0, 0.8, 0.0, 0.0, 50.0, false, -4.0, 0.0, 1.0, 1.0, 1.0);
pub static MAT_SOFTMETAL: MaterialDef = mat(0.5, 0.5, 1.0, 0.0, 8.0, false, 0.0, 0.0, 1.0, 1.0, 0.0);

pub fn lookup_material(name: &str) -> MaterialDef {
    match name {
        "legacyMatte" => MAT_LEGACY_MATTE,
        "legacyPlastic" => MAT_LEGACY_PLASTIC,
        "legacyMetal" => MAT_LEGACY_METAL,
        "legacyWireframe" => MAT_LEGACY_WIREFRAME,
        "matte" => MAT_MATTE,
        "plastic" => MAT_PLASTIC,
        "metal" => MAT_METAL,
        "warmMatte" => MAT_WARM_MATTE,
        "translucentPowder" => MAT_TRANSLUCENT_POWDER,
        "powder" => MAT_POWDER,
        "dkEdge" => MAT_DK_EDGE,
        "softEdge" => MAT_SOFT_EDGE,
        "clear" => MAT_CLEAR,
        "flat" => MAT_FLAT,
        "softmetal" => MAT_SOFTMETAL,
        _ => MAT_MATTE,
    }
}
