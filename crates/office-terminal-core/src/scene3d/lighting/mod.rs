//! Multi-light rigs, material definitions, and Blinn-Phong/Fresnel surface shading (S7a).
//!
//! Port of `packages/core/src/shape/bevel-light-rigs.ts` and `bevel-lighting.ts`.

mod material_tables;
mod rig_legacy;
mod rig_modern;

pub use material_tables::{lookup_material, MaterialDef};
pub use rig_legacy::lookup_legacy_rig;
pub use rig_modern::lookup_modern_rig;

/// Authored single light definition from a preset light rig.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDef {
    pub rgb: [f32; 3],
    pub direction: [f32; 3],
    pub scale: f32,
    pub offset: f32,
    pub specular: bool,
    pub diffuse: bool,
}

/// Authored preset light rig containing ambient intensity and up to 4 directional lights.
#[derive(Clone, Debug, PartialEq)]
pub struct RigDefinition {
    pub ambient: f32,
    pub lights: &'static [LightDef],
}

/// Resolve a preset light rig by name, defaulting to `threePt`.
pub fn lookup_rig_definition(name: &str) -> &'static RigDefinition {
    if let Some(rig) = lookup_modern_rig(name) {
        rig
    } else if let Some(rig) = lookup_legacy_rig(name) {
        rig
    } else {
        &rig_modern::RIG_THREE_PT
    }
}

/// Prepared light rotated into view/scene coordinates with precomputed halfway vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedLight {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub hx: f32,
    pub hy: f32,
    pub hz: f32,
    pub rgb: [f32; 3],
    pub scale: f32,
    pub offset: f32,
    pub specular: bool,
    pub diffuse: bool,
}

/// Rotated light rig ready for surface evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRig {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub lights: Vec<PreparedLight>,
    pub ambient: f32,
}

/// Direction angle for standard `ST_LightRigDirection` presets in degrees.
pub fn rig_direction_angle(dir: &str) -> f32 {
    match dir {
        "t" => 0.0,
        "tr" => 45.0,
        "r" => 90.0,
        "br" => 135.0,
        "b" => 180.0,
        "bl" => 225.0,
        "l" => 270.0,
        "tl" => 315.0,
        _ => 0.0,
    }
}

/// Rotate an authored rig into world/view space based on direction preset and optional rotation.
pub fn rig_vector(rig_name: &str, dir_name: &str, rot: Option<&crate::contracts::VectorRot3d>) -> PreparedRig {
    let rig = lookup_rig_definition(rig_name);
    let deg_to_rad = std::f32::consts::PI / 180.0;
    let (lat_rad, lon_rad, rev_rad) = if let Some(r) = rot {
        (r.lat_deg * deg_to_rad, r.lon_deg * deg_to_rad, r.rev_deg * deg_to_rad)
    } else {
        (0.0, 0.0, rig_direction_angle(dir_name) * deg_to_rad)
    };

    let (c, l) = (lat_rad.cos(), lat_rad.sin());
    let (u, d) = (lon_rad.cos(), lon_rad.sin());
    let (f, p) = (rev_rad.cos(), rev_rad.sin());

    let mut lights = Vec::with_capacity(rig.lights.len());
    for light in rig.lights {
        let [lx, ly, lz] = light.direction;
        let len = (lx * lx + ly * ly + lz * lz).sqrt();
        let inv_len = if len > 1e-6 { 1.0 / len } else { 1.0 };
        let a = ly * inv_len;
        let o = lx * inv_len;
        let s = -lz * inv_len;

        let m_rot = f * a - p * o;
        let h_rot = p * a + f * o;
        let g_rot = c * h_rot - l * s;
        let underscore = l * h_rot + c * s;
        let v = u * m_rot + d * underscore;
        let y = -d * m_rot + u * underscore;

        let b = (v * v + g_rot * g_rot + (y + 1.0) * (y + 1.0)).sqrt();
        let inv_b = if b > 1e-6 { 1.0 / b } else { 1.0 };

        lights.push(PreparedLight {
            x: v,
            y: g_rot,
            z: y,
            hx: v * inv_b,
            hy: g_rot * inv_b,
            hz: (y + 1.0) * inv_b,
            rgb: light.rgb,
            scale: light.scale,
            offset: light.offset,
            specular: light.specular,
            diffuse: light.diffuse,
        });
    }

    let (first_x, first_y, first_z) = lights.first().map(|lt| (lt.x, lt.y, lt.z)).unwrap_or((0.0, 0.0, 1.0));

    PreparedRig {
        x: first_x,
        y: first_y,
        z: first_z,
        lights,
        ambient: rig.ambient,
    }
}

/// Prepared lighting model coupling material properties and a prepared light rig.
#[derive(Clone, Debug)]
pub struct PreparedLighting {
    pub material: MaterialDef,
    pub ambient: f32,
    pub lights: Vec<PreparedLight>,
    pub flat_terms: [f32; 6],
}

/// Compute 6 lighting terms: diffuse RGB (`terms[0..3]`) and specular RGB (`terms[3..6]`).
pub fn compute_light_terms(nx: f32, ny: f32, nz: f32, lights: &[PreparedLight], mat: &MaterialDef) -> [f32; 6] {
    let mut diff_r = 0.0f32;
    let mut diff_g = 0.0f32;
    let mut diff_b = 0.0f32;
    let mut spec_r = 0.0f32;
    let mut spec_g = 0.0f32;
    let mut spec_b = 0.0f32;

    for light in lights {
        let dot = nx * light.x + ny * light.y + nz * light.z;
        if light.diffuse {
            let t = (dot * light.scale + light.offset).max(0.0);
            diff_r += light.rgb[0] * t;
            diff_g += light.rgb[1] * t;
            diff_b += light.rgb[2] * t;
        }
        if light.specular && dot > 0.0 && (mat.specular_shape != 0.0 || mat.specular_white != 0.0) {
            let spec_dot = if mat.blinn {
                nx * light.hx + ny * light.hy + nz * light.hz
            } else {
                2.0 * dot * nz - light.z
            };
            let spec_factor = spec_dot.max(0.0).powf(mat.power);
            spec_r += light.rgb[0] * spec_factor;
            spec_g += light.rgb[1] * spec_factor;
            spec_b += light.rgb[2] * spec_factor;
        }
    }

    [diff_r, diff_g, diff_b, spec_r, spec_g, spec_b]
}

/// Instantiate prepared lighting for a material preset and a rotated rig.
pub fn prepare_lighting(material_preset: &str, rig: &PreparedRig) -> PreparedLighting {
    let mat = lookup_material(material_preset);
    let flat_terms = compute_light_terms(0.0, 0.0, 1.0, &rig.lights, &mat);
    PreparedLighting {
        material: mat,
        ambient: rig.ambient,
        lights: rig.lights.clone(),
        flat_terms,
    }
}

/// Surface shading evaluating diffuse, specular, Fresnel, emissive and ambient terms.
#[allow(clippy::too_many_arguments)]
pub fn shade_surface(
    r: f32,
    g: f32,
    b: f32,
    a: f32,
    nx: f32,
    ny: f32,
    nz: f32,
    lighting: &PreparedLighting,
) -> [f32; 4] {
    let mat = &lighting.material;
    let u = 1.0 - nz.abs().min(1.0);
    let diff_fresnel = if mat.diffuse_fresnel == 0.0 {
        0.0
    } else {
        u.powf(mat.diffuse_fresnel.abs())
    };
    let alpha_fresnel = if mat.alpha_fresnel == 0.0 {
        0.0
    } else {
        u.powf(mat.alpha_fresnel.abs())
    };

    let mut diff_alpha = mat.diffuse_alpha;
    if mat.alpha_fresnel > 0.0 {
        diff_alpha += (1.0 - diff_alpha) * alpha_fresnel;
    } else if mat.alpha_fresnel < 0.0 {
        diff_alpha *= 1.0 - alpha_fresnel;
    }

    let is_flat = nx.abs() < 1e-5 && ny.abs() < 1e-5 && (nz - 1.0).abs() < 1e-5;
    let terms = if is_flat {
        lighting.flat_terms
    } else {
        compute_light_terms(nx, ny, nz, &lighting.lights, mat)
    };

    let src = [r / 255.0, g / 255.0, b / 255.0];
    let mut out = [0.0f32; 4];
    for i in 0..3 {
        let mut diff_term = src[i] * mat.diffuse_shape + mat.diffuse_white;
        diff_term = if mat.diffuse_fresnel > 0.0 {
            diff_term + (1.0 - diff_term) * diff_fresnel
        } else {
            diff_term * (1.0 - diff_fresnel)
        };
        let spec_term = src[i] * mat.specular_shape + mat.specular_white;
        let lit = src[i] * mat.emissive
            + (src[i] * mat.diffuse_shape + mat.diffuse_white) * lighting.ambient
            + diff_term * terms[i]
            + spec_term * terms[i + 3];
        out[i] = (lit * 255.0).clamp(0.0, 255.0);
    }
    out[3] = a * diff_alpha.clamp(0.0, 1.0);
    out
}
