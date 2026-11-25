//! Decoded-image raster policy for one render pass.
//!
//! Ports the reference's `planDecodedImageTargets`
//! (ref core/src/image/adaptive-image-budget.ts:130-330) and the display/decode
//! axes it derives (ref core/src/image/raster-target.ts:1-70):
//!
//! * every `DrawImage` contributes one demand keyed by blip identity, with the
//!   *device* grid its destination covers;
//! * `native-if-fit` keeps the source grid while the aggregate RGBA surfaces fit
//!   `decodedByteBudget`, and otherwise allocates one uniform quality scale
//!   across every demand (integer-floored, so the plan never rounds above the
//!   budget);
//! * `strict` surfaces the typed quota crossing instead of degrading.
//!
//! Our [`AssetStore`] decodes and then resizes, so a hint above the native grid
//! is a no-op: the plan only shrinks, and [`DecodePlan::size_hint`] returns
//! [`SizeHint::Native`] for every surface the plan leaves at source size.

use std::collections::BTreeMap;

use crate::contracts::{AffineTransform, RectF32, RenderTargetSpec, VectorCommand, VectorSlide};
use crate::error::RenderError;
use crate::loader::SizeHint;

/// Per-surface ceiling on retained raster pixels: 32 MP / 128 MiB RGBA
/// (ref core/src/image/pixel-budget.ts:24, `MAX_RASTER_PIXELS`).
pub const MAX_RASTER_PIXELS: u64 = 1 << 25;

/// Default aggregate decoded-byte budget for one document paint
/// (ref pixel-budget.ts:46, `MAX_DECODED_IMAGE_BYTES`).
pub const MAX_DECODED_IMAGE_BYTES: u64 = MAX_RASTER_PIXELS * 4;

/// Hard ceiling a caller may not exceed (ref pixel-budget.ts:54,
/// `HARD_MAX_DECODED_IMAGE_BYTES`).
pub const HARD_MAX_DECODED_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// `imageResources.strategy` (ref adaptive-image-budget.ts:11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetStrategy {
    /// Lower raster resolution to stay inside the budget.
    Adaptive,
    /// Preserve requested targets and surface the typed quota error.
    Strict,
}

/// `imageResources.resolution` (ref adaptive-image-budget.ts:20-24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeResolution {
    /// Keep the source grid while the paint budget allows it.
    NativeIfFit,
    /// Request the current canvas/DPR grid.
    Display,
}

/// `imageResources` option bag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageResourceOptions {
    pub decoded_byte_budget: u64,
    pub strategy: BudgetStrategy,
    pub resolution: DecodeResolution,
}

impl Default for ImageResourceOptions {
    fn default() -> Self {
        Self {
            decoded_byte_budget: MAX_DECODED_IMAGE_BYTES,
            strategy: BudgetStrategy::Adaptive,
            resolution: DecodeResolution::NativeIfFit,
        }
    }
}

impl ImageResourceOptions {
    /// Validate the budget range (ref adaptive-image-budget.ts:56-66).
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.decoded_byte_budget < 4 || self.decoded_byte_budget > HARD_MAX_DECODED_IMAGE_BYTES {
            return Err(RenderError::RasterizationError(format!(
                "decoded byte budget must be 4..={} bytes",
                HARD_MAX_DECODED_IMAGE_BYTES
            )));
        }
        Ok(())
    }
}

/// One retained raster surface a render pass needs
/// (ref adaptive-image-budget.ts:78-92, `DecodedImageTargetDemand`).
#[derive(Clone, Debug, PartialEq)]
pub struct DecodeDemand {
    /// Cache identity, including any pixel effect producing another surface.
    pub key: String,
    /// Device-pixel destination grid of the full source after crop expansion.
    pub target_width_px: f32,
    pub target_height_px: f32,
    /// Declared source grid, when the decoder can report it.
    pub source_width_px: Option<f32>,
    pub source_height_px: Option<f32>,
    /// Simultaneously retained surfaces for this identity.
    pub retained_surfaces: u32,
}

/// One planned surface (ref adaptive-image-budget.ts:94-110).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedTarget {
    /// Decoder request axes.
    pub width: u32,
    pub height: u32,
    /// Grid charged to the budget.
    pub retained_width: u32,
    pub retained_height: u32,
    pub retained_pixels: u64,
    pub max_retained_pixels: u64,
}

/// The plan for one render pass.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodePlan {
    pub targets: BTreeMap<String, PlannedTarget>,
    pub ideal_bytes: u64,
    pub planned_bytes: u64,
    pub quality_scale: f32,
    pub degraded: bool,
}

impl DecodePlan {
    /// Decode hint for one blip.
    ///
    /// Raster surfaces the plan leaves at source size keep [`SizeHint::Native`]
    /// (the store cannot upscale them). SVG has no fixed grid, so it always
    /// takes the planned device-pixel target — including the supersampled
    /// target branch 2 allocates for uninspected sources.
    pub fn size_hint(&self, rel_id: &str) -> SizeHint {
        let is_svg = rel_id.to_ascii_lowercase().ends_with(".svg");
        match self.targets.get(rel_id) {
            Some(target) if self.degraded || is_svg => SizeHint::AtMost {
                width: target.width,
                height: target.height,
            },
            _ => SizeHint::Native,
        }
    }
}

fn positive_pixel(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.ceil().max(1.0)
    } else {
        1.0
    }
}

fn surfaces(demand: &DecodeDemand) -> u64 {
    u64::from(demand.retained_surfaces.max(1))
}

/// Grid actually retained for a bounded display decode
/// (ref raster-target.ts:50-73, `decodedBitmapRetainedTarget`): a two-axis
/// request uses the exact axis-wise grid, clamped to avoid upscaling.
fn retained_target(source: Option<(f32, f32)>, target: (f32, f32)) -> Option<(u32, u32)> {
    let source = source?;
    let width = source.0.min(target.0.ceil().max(1.0));
    let height = source.1.min(target.1.ceil().max(1.0));
    if width < source.0 || height < source.1 {
        Some((width as u32, height as u32))
    } else {
        None
    }
}

/// Plan every decoded surface for one render pass
/// (ref adaptive-image-budget.ts:130-330, `planDecodedImageTargets`).
pub fn plan_decoded_image_targets(
    demands: &[DecodeDemand],
    options: &ImageResourceOptions,
) -> Result<DecodePlan, RenderError> {
    options.validate()?;

    // One entry per cache identity; repeated keys keep the largest grid and
    // surface count (ref adaptive-image-budget.ts:141-163).
    let mut merged: BTreeMap<String, DecodeDemand> = BTreeMap::new();
    for demand in demands {
        let entry = merged.entry(demand.key.clone()).or_insert_with(|| DecodeDemand {
            key: demand.key.clone(),
            target_width_px: 0.0,
            target_height_px: 0.0,
            source_width_px: None,
            source_height_px: None,
            retained_surfaces: 1,
        });
        entry.target_width_px = entry.target_width_px.max(positive_pixel(demand.target_width_px));
        entry.target_height_px = entry.target_height_px.max(positive_pixel(demand.target_height_px));
        if let (Some(w), Some(h)) = (demand.source_width_px, demand.source_height_px) {
            entry.source_width_px = Some(entry.source_width_px.unwrap_or(0.0).max(positive_pixel(w)));
            entry.source_height_px = Some(entry.source_height_px.unwrap_or(0.0).max(positive_pixel(h)));
        }
        entry.retained_surfaces = entry.retained_surfaces.max(demand.retained_surfaces.max(1));
    }

    let source_of = |demand: &DecodeDemand| -> Option<(f32, f32)> {
        match (demand.source_width_px, demand.source_height_px) {
            (Some(w), Some(h)) => Some((positive_pixel(w), positive_pixel(h))),
            _ => None,
        }
    };
    let native_size = |demand: &DecodeDemand| -> (f32, f32) {
        source_of(demand).unwrap_or((demand.target_width_px, demand.target_height_px))
    };
    let scaled_request = |demand: &DecodeDemand, scale: f32| -> (f32, f32) {
        (
            (demand.target_width_px * scale).floor().max(1.0),
            (demand.target_height_px * scale).floor().max(1.0),
        )
    };
    let retained_for = |demand: &DecodeDemand, scale: f32| -> (u32, u32) {
        let request = scaled_request(demand, scale);
        match source_of(demand) {
            Some(source) => {
                retained_target(Some(source), request).unwrap_or((source.0.max(1.0) as u32, source.1.max(1.0) as u32))
            }
            None => (request.0 as u32, request.1 as u32),
        }
    };
    let byte_cost = |size: &dyn Fn(&DecodeDemand) -> (u32, u32)| -> u64 {
        merged
            .values()
            .map(|demand| {
                let (w, h) = size(demand);
                u64::from(w) * u64::from(h) * surfaces(demand) * 4
            })
            .sum()
    };

    let native_bytes = byte_cost(&|demand| {
        let (w, h) = native_size(demand);
        (w as u32, h as u32)
    });
    let native_surfaces_fit = merged.values().all(|demand| {
        let (w, h) = native_size(demand);
        u64::from(w as u32) * u64::from(h as u32) <= MAX_RASTER_PIXELS
    });
    let has_uninspected_source = merged.values().any(|demand| source_of(demand).is_none());
    let ideal_bytes = byte_cost(&|demand| retained_for(demand, 1.0));

    // 1. Everything the paint asks for already fits: keep the source grids.
    if options.resolution == DecodeResolution::NativeIfFit
        && !has_uninspected_source
        && native_surfaces_fit
        && native_bytes <= options.decoded_byte_budget
    {
        let mut targets = BTreeMap::new();
        for (key, demand) in &merged {
            let (w, h) = native_size(demand);
            let (w, h) = (w as u32, h as u32);
            targets.insert(
                key.clone(),
                PlannedTarget {
                    width: w,
                    height: h,
                    retained_width: w,
                    retained_height: h,
                    retained_pixels: u64::from(w) * u64::from(h),
                    max_retained_pixels: u64::from(w) * u64::from(h),
                },
            );
        }
        return Ok(DecodePlan {
            targets,
            ideal_bytes,
            planned_bytes: native_bytes,
            quality_scale: 1.0,
            degraded: false,
        });
    }

    // 2. Uninspected sources inside an adaptive budget: allocate the unused
    // aggregate budget as one uniform native-admission share, capped at a 2x
    // supersampled grid (ref adaptive-image-budget.ts:216-260).
    if options.resolution == DecodeResolution::NativeIfFit
        && options.strategy == BudgetStrategy::Adaptive
        && has_uninspected_source
        && ideal_bytes <= options.decoded_byte_budget
    {
        let mut admission = (options.decoded_byte_budget as f64 / ideal_bytes.max(1) as f64).sqrt();
        for demand in merged.values() {
            let (w, h) = retained_for(demand, 1.0);
            admission = admission.min((MAX_RASTER_PIXELS as f64 / (f64::from(w) * f64::from(h)).max(1.0)).sqrt());
        }
        let mut targets = BTreeMap::new();
        let mut remaining = options.decoded_byte_budget / 4;
        let mut planned_bytes = 0u64;
        for (key, demand) in &merged {
            let (w, h) = retained_for(demand, 1.0);
            let display_pixels = u64::from(w) * u64::from(h);
            let proportional = display_pixels.max((display_pixels as f64 * admission * admission).floor() as u64);
            let max_retained = proportional.min(remaining / surfaces(demand)).max(1);
            let scale = (max_retained as f64 / display_pixels.max(1) as f64).sqrt().min(2.0);
            let request = scaled_request(demand, scale as f32);
            let retained = retained_for(demand, scale as f32);
            targets.insert(
                key.clone(),
                PlannedTarget {
                    width: request.0 as u32,
                    height: request.1 as u32,
                    retained_width: retained.0,
                    retained_height: retained.1,
                    retained_pixels: u64::from(retained.0) * u64::from(retained.1),
                    max_retained_pixels: max_retained,
                },
            );
            remaining = remaining.saturating_sub(max_retained * surfaces(demand));
            planned_bytes = planned_bytes.saturating_add(max_retained * surfaces(demand) * 4);
        }
        return Ok(DecodePlan {
            targets,
            ideal_bytes,
            planned_bytes,
            quality_scale: 1.0,
            degraded: false,
        });
    }

    // 3. Strict mode surfaces the aggregate crossing instead of degrading.
    if options.strategy == BudgetStrategy::Strict && ideal_bytes > options.decoded_byte_budget {
        return Err(RenderError::DecodedImageLimit {
            metric: "active-decoded-bytes",
            limit: options.decoded_byte_budget,
            observed: ideal_bytes,
        });
    }
    if options.strategy == BudgetStrategy::Adaptive {
        let minimum = byte_cost(&|demand| retained_for(demand, 0.0));
        if minimum > options.decoded_byte_budget {
            return Err(RenderError::DecodedImageLimit {
                metric: "active-decoded-bytes",
                limit: options.decoded_byte_budget,
                observed: minimum,
            });
        }
    }

    // 4. One uniform adaptive scale, binary-searched so integer projection
    // never rounds above the budget (ref adaptive-image-budget.ts:290-310).
    let mut adaptive = 1.0f32;
    if options.strategy == BudgetStrategy::Adaptive && ideal_bytes > options.decoded_byte_budget {
        let (mut lower, mut upper) = (0.0f32, 1.0f32);
        for _ in 0..40 {
            let candidate = (lower + upper) / 2.0;
            let bytes = byte_cost(&|demand| retained_for(demand, candidate));
            if bytes <= options.decoded_byte_budget {
                lower = candidate;
            } else {
                upper = candidate;
            }
        }
        adaptive = lower;
    }

    let mut targets = BTreeMap::new();
    for (key, demand) in &merged {
        let request = scaled_request(demand, adaptive);
        let (w, h) = retained_for(demand, adaptive);
        targets.insert(
            key.clone(),
            PlannedTarget {
                width: request.0 as u32,
                height: request.1 as u32,
                retained_width: w,
                retained_height: h,
                retained_pixels: u64::from(w) * u64::from(h),
                max_retained_pixels: u64::from(w) * u64::from(h),
            },
        );
    }
    let planned_bytes = byte_cost(&|demand| retained_for(demand, adaptive));
    Ok(DecodePlan {
        targets,
        ideal_bytes,
        planned_bytes,
        quality_scale: adaptive,
        degraded: adaptive < 1.0,
    })
}

/// The device-pixel destination grid of one `DrawImage` after its shape
/// transform and the slide's uniform scale.
fn display_target(dest: &RectF32, transform: &AffineTransform, scale: f32, offset: (f32, f32)) -> (f32, f32) {
    let corners = [
        (dest.x, dest.y),
        (dest.x + dest.w, dest.y),
        (dest.x, dest.y + dest.h),
        (dest.x + dest.w, dest.y + dest.h),
    ];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in corners {
        let (tx, ty) = transform.transform_point(x, y);
        let px = offset.0 + tx * scale;
        let py = offset.1 + ty * scale;
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    ((max_x - min_x).abs(), (max_y - min_y).abs())
}

/// Demands for every `DrawImage` of a compiled slide, in the uniform
/// slide→canvas mapping [`crate::rasterizer::TinySkiaRasterizer`] uses.
pub fn scene_decode_demands(scene: &VectorSlide, target: &RenderTargetSpec) -> Vec<DecodeDemand> {
    let width = target.width_px;
    let height = target.height_px;
    let s = (width as f64 / scene.width_emu.max(1) as f64).min(height as f64 / scene.height_emu.max(1) as f64) as f32;
    let dx = ((width as f64 - (scene.width_emu as f64 * s as f64)) / 2.0) as f32;
    let dy = ((height as f64 - (scene.height_emu as f64 * s as f64)) / 2.0) as f32;

    let mut demands = Vec::new();
    for command in &scene.commands {
        let VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            transform,
            ..
        } = command
        else {
            continue;
        };
        // A TIFF codec result cannot be decoder-resized, so it stays on its
        // authored source grid and never contributes a display cap
        // (ref renderer-adaptive-image-budget.test.ts:176-198).
        if is_tiff_source(rel_id) {
            continue;
        }
        let (w, h) = display_target(dest_rect, transform, s, (dx, dy));
        // The demand is the *full* source grid: the visible window is only
        // `src_rect.w` of the bitmap, so the retained source must cover the
        // destination divided by that fraction
        // (ref renderer-picture-raster-target.test.ts:237-274).
        let visible_w = if src_rect.w > 0.0 && src_rect.w.is_finite() {
            src_rect.w
        } else {
            1.0
        };
        let visible_h = if src_rect.h > 0.0 && src_rect.h.is_finite() {
            src_rect.h
        } else {
            1.0
        };
        demands.push(DecodeDemand {
            key: rel_id.clone(),
            target_width_px: w / visible_w,
            target_height_px: h / visible_h,
            source_width_px: None,
            source_height_px: None,
            retained_surfaces: 1,
        });
    }
    demands
}

/// A TIFF blip: the optional codec decodes at its authored grid only.
fn is_tiff_source(rel_id: &str) -> bool {
    let lower = rel_id.to_ascii_lowercase();
    lower.ends_with(".tif") || lower.ends_with(".tiff")
}

/// Plan a slide's decoded surfaces with the render pass' default policy.
pub fn scene_decode_plan(scene: &VectorSlide, target: &RenderTargetSpec) -> Result<DecodePlan, RenderError> {
    plan_decoded_image_targets(&scene_decode_demands(scene, target), &ImageResourceOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_with_target(rel_id: &str, width: u32, height: u32, degraded: bool) -> DecodePlan {
        let mut targets = BTreeMap::new();
        targets.insert(
            rel_id.to_string(),
            PlannedTarget {
                width,
                height,
                retained_width: width,
                retained_height: height,
                retained_pixels: u64::from(width) * u64::from(height),
                max_retained_pixels: u64::from(width) * u64::from(height),
            },
        );
        DecodePlan {
            targets,
            ideal_bytes: 0,
            planned_bytes: 0,
            quality_scale: 1.0,
            degraded,
        }
    }

    #[test]
    fn svg_uses_the_planned_grid_even_without_budget_degradation() {
        let plan = plan_with_target("ppt/media/image1.svg", 1024, 512, false);
        assert_eq!(
            plan.size_hint("ppt/media/image1.svg"),
            SizeHint::AtMost {
                width: 1024,
                height: 512
            }
        );
    }

    #[test]
    fn raster_surfaces_keep_native_unless_degraded() {
        let plan = plan_with_target("ppt/media/image1.png", 1024, 512, false);
        assert_eq!(plan.size_hint("ppt/media/image1.png"), SizeHint::Native);

        let degraded = plan_with_target("ppt/media/image1.png", 512, 256, true);
        assert_eq!(
            degraded.size_hint("ppt/media/image1.png"),
            SizeHint::AtMost {
                width: 512,
                height: 256
            }
        );
    }
}
