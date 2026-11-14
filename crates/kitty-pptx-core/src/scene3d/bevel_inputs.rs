//! Bevel-shading inputs in device px.
//!
//! Port of `packages/pptx/src/renderer.ts:6296-6358` (`buildBevelInputs`) and
//! `packages/core/src/shape/bevel-lighting.ts:16-89` (`presetMaterial` /
//! `MATERIALS` keys).
//!
//! ECMA-376 §20.1.5.12 `bevelT`/`bevelB` band and height are EMU lengths
//! (§20.1.5.3); they are converted with `scale × devScale` — the EMU→CSS-px
//! scale times the device scale — because the shading reads/writes the
//! device-resolution offscreen. No further factor is applied: the slide-6 "flat
//! cut" investigation suspected a double-applied scale, and these inputs are
//! what pin its absence.
//!
//! The IR now carries the authored `lightRig@rig`/`@dir`/`@rot`
//! (`VectorScene3d::{light_rig_type,light_dir,light_rot}`) for the per-rig
//! tables. This pass still resolves the flat azimuth
//! (`light_direction_deg`) because `lightDirFromRig`'s 23-entry light rig table
//! (`bevel-light-rigs.ts`) belongs to the S7a shading work; that is the
//! recorded divergence in `tests/parity/coverage/shapes-b.md`.

use crate::contracts::{VectorScene3d, VectorShape3d};
use crate::scene3d::light_vector;

fn preset_name(preset: &str) -> String {
    if preset.is_empty() {
        "circle".to_string()
    } else {
        preset.to_string()
    }
}

/// `ST_PresetMaterialType` surface class (ref `PresetMaterial`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetMaterial {
    LegacyMatte,
    LegacyPlastic,
    LegacyMetal,
    LegacyWireframe,
    Matte,
    Plastic,
    Metal,
    WarmMatte,
    TranslucentPowder,
    Powder,
    DkEdge,
    SoftEdge,
    Clear,
    Flat,
    SoftMetal,
}

impl PresetMaterial {
    /// `prstMaterial` name → class; unknown/absent names degrade to `matte`
    /// (ref `presetMaterial`).
    pub fn from_name(name: &str) -> Self {
        match name {
            "legacyMatte" => PresetMaterial::LegacyMatte,
            "legacyPlastic" => PresetMaterial::LegacyPlastic,
            "legacyMetal" => PresetMaterial::LegacyMetal,
            "legacyWireframe" => PresetMaterial::LegacyWireframe,
            "plastic" => PresetMaterial::Plastic,
            "metal" => PresetMaterial::Metal,
            "warmMatte" => PresetMaterial::WarmMatte,
            "translucentPowder" => PresetMaterial::TranslucentPowder,
            "powder" => PresetMaterial::Powder,
            "dkEdge" => PresetMaterial::DkEdge,
            "softEdge" => PresetMaterial::SoftEdge,
            "clear" => PresetMaterial::Clear,
            "flat" => PresetMaterial::Flat,
            "softmetal" => PresetMaterial::SoftMetal,
            _ => PresetMaterial::Matte,
        }
    }

    /// The `ST_PresetMaterialType` spelling (used to key the shading material).
    pub fn as_str(self) -> &'static str {
        match self {
            PresetMaterial::LegacyMatte => "legacyMatte",
            PresetMaterial::LegacyPlastic => "legacyPlastic",
            PresetMaterial::LegacyMetal => "legacyMetal",
            PresetMaterial::LegacyWireframe => "legacyWireframe",
            PresetMaterial::Matte => "matte",
            PresetMaterial::Plastic => "plastic",
            PresetMaterial::Metal => "metal",
            PresetMaterial::WarmMatte => "warmMatte",
            PresetMaterial::TranslucentPowder => "translucentPowder",
            PresetMaterial::Powder => "powder",
            PresetMaterial::DkEdge => "dkEdge",
            PresetMaterial::SoftEdge => "softEdge",
            PresetMaterial::Clear => "clear",
            PresetMaterial::Flat => "flat",
            PresetMaterial::SoftMetal => "softmetal",
        }
    }

    /// `legacyWireframe` has no surface properties (ECMA-376 §20.1.10.50); its
    /// geometry is rasterized by the bevel compositor instead of being shaded.
    pub fn is_wireframe(self) -> bool {
        self == PresetMaterial::LegacyWireframe
    }
}

/// One bevel lip to shade, already in device px (ref `BevelInput`).
#[derive(Clone, Debug, PartialEq)]
pub struct BevelInput {
    /// Bevel `w` converted to device px.
    pub width_px: f64,
    /// Bevel `h` (profile height) converted to device px.
    pub height_px: f64,
    /// `ST_BevelPresetType` profile name; empty authors resolve to `circle`
    /// (ref `buildBevelInputs`: `prst || 'circle'`).
    pub preset: String,
    pub material: PresetMaterial,
    /// Unit surface-to-light vector for the rig/direction.
    pub light: [f64; 3],
    /// `true` for `bevelB`, which MS-OI29500 §2.1.1268 places at the base.
    pub bottom: bool,
    /// Extrusion depth in device px (bottom lip only).
    pub depth_px: f64,
    /// Wireframe rim width in device px (`devScale`).
    pub wireframe_line_px: f64,
}

/// Build the bevel-shading inputs for an `sp3d`, in device px.
///
/// Returns an empty list when there is no bevel to shade: `bevelT`/`bevelB`
/// must have `w > 0 && h > 0` (ref `buildBevelInputs`), so a zero-sized bevel
/// contributes neither a band nor a wireframe-only pass.
pub fn build_bevel_inputs(
    shape_3d: Option<&VectorShape3d>,
    scene_3d: Option<&VectorScene3d>,
    scale: f64,
    dev_scale: f64,
) -> Vec<BevelInput> {
    let Some(shape_3d) = shape_3d else {
        return Vec::new();
    };
    let light = {
        let azimuth = scene_3d.map(|s| s.light_direction_deg).unwrap_or(270.0);
        let v = light_vector(azimuth);
        [v[0] as f64, v[1] as f64, v[2] as f64]
    };
    let material = PresetMaterial::from_name(&shape_3d.prst_material);
    let emu_to_dev = scale * dev_scale;
    let mut out: Vec<BevelInput> = Vec::new();

    let band = |w: i64, h: i64| (w as f64 * emu_to_dev, h as f64 * emu_to_dev);
    if let Some(top) = shape_3d
        .bevel_top
        .as_ref()
        .filter(|bevel| bevel.width_emu > 0 && bevel.height_emu > 0)
    {
        let (width_px, height_px) = band(top.width_emu, top.height_emu);
        out.push(BevelInput {
            width_px,
            height_px,
            preset: preset_name(&top.preset),
            material,
            light,
            bottom: false,
            depth_px: 0.0,
            wireframe_line_px: dev_scale,
        });
    }
    if let Some(bottom) = shape_3d
        .bevel_bottom
        .as_ref()
        .filter(|bevel| bevel.width_emu > 0 && bevel.height_emu > 0)
    {
        let (width_px, height_px) = band(bottom.width_emu, bottom.height_emu);
        out.push(BevelInput {
            width_px,
            height_px,
            preset: preset_name(&bottom.preset),
            material,
            light,
            bottom: true,
            depth_px: shape_3d.extrusion_depth_emu as f64 * emu_to_dev,
            wireframe_line_px: dev_scale,
        });
    }
    // Wireframe has geometry even without a bevel; it must not retain a solid
    // face just because there is no bevel band to send to the compositor.
    if material.is_wireframe() {
        for bottom in [false, true] {
            if !out.iter().any(|bevel| bevel.bottom == bottom) {
                out.push(BevelInput {
                    width_px: 0.0,
                    height_px: 0.0,
                    preset: "circle".to_string(),
                    material,
                    light,
                    bottom,
                    depth_px: if bottom {
                        shape_3d.extrusion_depth_emu as f64 * emu_to_dev
                    } else {
                        0.0
                    },
                    wireframe_line_px: dev_scale,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::VectorBevel;

    fn shape() -> VectorShape3d {
        VectorShape3d {
            extrusion_depth_emu: 0,
            extrusion_color: None,
            contour_width_emu: 0,
            contour_color: None,
            prst_material: "matte".to_string(),
            bevel_top: Some(VectorBevel {
                width_emu: 304_800,
                height_emu: 152_400,
                preset: "hardEdge".to_string(),
            }),
            bevel_bottom: None,
        }
    }

    #[test]
    fn band_is_the_emu_width_times_scale_times_dev_scale() {
        let css_scale = 1920.0 / 12_192_000.0;
        let inputs = build_bevel_inputs(Some(&shape()), None, css_scale, 4.0);
        assert_eq!(inputs.len(), 1);
        assert!((inputs[0].width_px - 304_800.0 * css_scale * 4.0).abs() <= 1.0);
        assert!((inputs[0].height_px - 152_400.0 * css_scale * 4.0).abs() <= 1.0);
    }

    #[test]
    fn no_bevel_needs_no_input() {
        let mut flat = shape();
        flat.bevel_top = None;
        assert!(build_bevel_inputs(Some(&flat), None, 1.0, 1.0).is_empty());
        assert!(build_bevel_inputs(None, None, 1.0, 1.0).is_empty());
    }
}
