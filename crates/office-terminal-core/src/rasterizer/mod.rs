pub mod decode_plan;
pub mod effects;

/// Image-fill paint path (`a:blipFill` / `a:pattFill`) surface used by the
/// parity tests and by the SVG twin.
pub use image_fill::{
    fill_path_with_image, fill_path_with_pattern, prepare_image_fill, tile_anchor, tile_cell_pixels,
    tile_source_extent_emu, ImageFillBrush, MAX_TILE_CELL_DIMENSION, MAX_TILE_CELL_PIXELS,
};
mod background_image;
mod bevel;
mod clip;
pub mod filters;
mod image;
mod image_fill;
mod mask;
mod path;
pub mod reflection_blur;
mod scratch;
mod shape_effect;
mod text;
mod texture_fill;

use crate::contracts::{
    FillRule, PathSegment, RenderTargetSpec, RgbaSlide, ShapeEffects, VectorBackground, VectorCommand, VectorPaint,
    VectorPath, VectorSlide,
};
use crate::error::RenderError;
use crate::loader::AssetStore;
use std::sync::Arc;

use clip::{scratch_layer, ClipStack, LayerPlacement};

pub trait SlideRasterizer: Send + Sync {
    fn rasterize(
        &self,
        scene: &VectorSlide,
        target: &RenderTargetSpec,
        assets: &dyn AssetStore,
    ) -> Result<RgbaSlide, RenderError>;
}

pub struct TinySkiaRasterizer;

impl TinySkiaRasterizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TinySkiaRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SlideRasterizer for TinySkiaRasterizer {
    fn rasterize(
        &self,
        scene: &VectorSlide,
        target: &RenderTargetSpec,
        assets: &dyn AssetStore,
    ) -> Result<RgbaSlide, RenderError> {
        let width = target.width_px;
        let height = target.height_px;

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| RenderError::RasterizationError("failed to allocate pixmap".to_string()))?;

        // Decoded-raster policy for this pass (ref core/src/image/
        // adaptive-image-budget.ts:130): it only shrinks surfaces when the
        // aggregate budget cannot hold the source grids, so the default path
        // keeps native decoding.
        let decode_plan = decode_plan::scene_decode_plan(scene, target).ok();

        // 1. Uniform scale factor
        let s = (width as f64 / scene.width_emu.max(1) as f64).min(height as f64 / scene.height_emu.max(1) as f64);
        let s_f32 = s as f32;

        let dx = ((width as f64 - (scene.width_emu as f64 * s)) / 2.0) as f32;
        let dy = ((height as f64 - (scene.height_emu as f64 * s)) / 2.0) as f32;

        // 2. Clear background
        if let Some(ref bg) = scene.background {
            match bg {
                VectorBackground::Solid([r, g, b, a]) => {
                    let color = tiny_skia::Color::from_rgba8(*r, *g, *b, *a);
                    pixmap.fill(color);
                }
                VectorBackground::Gradient(grad) => {
                    let bg_path = VectorPath::new(
                        vec![
                            PathSegment::MoveTo { x: 0.0, y: 0.0 },
                            PathSegment::LineTo {
                                x: scene.width_emu as f32,
                                y: 0.0,
                            },
                            PathSegment::LineTo {
                                x: scene.width_emu as f32,
                                y: scene.height_emu as f32,
                            },
                            PathSegment::LineTo {
                                x: 0.0,
                                y: scene.height_emu as f32,
                            },
                            PathSegment::Close,
                        ],
                        FillRule::NonZero,
                    );
                    path::render_path(
                        &mut pixmap,
                        assets,
                        &bg_path,
                        Some(&VectorPaint::LinearGradient(grad.clone())),
                        None,
                        &crate::contracts::AffineTransform::default(),
                        s_f32,
                        dx,
                        dy,
                    );
                }
                // §20.1.8.14 blipFill background: white base, slide clip,
                // then srcRect crop + fillRect insets or a:tile repetition
                // (ref renderer.ts:2218-2362).
                image_bg @ VectorBackground::Image { .. } => {
                    background_image::paint(
                        &mut pixmap,
                        assets,
                        image_bg,
                        scene.width_emu as f32,
                        scene.height_emu as f32,
                        s_f32,
                        dx,
                        dy,
                    );
                }
            }
        } else {
            pixmap.fill(tiny_skia::Color::WHITE);
        }

        // 3. Render vector commands through the clip stack. The reference
        //    wraps picture silhouettes in ctx.save()/clip()/restore()
        //    (ref renderer.ts:6985-7036); here PushClip/PopClip delimit the
        //    same nested region, and every command drawn while the stack is
        //    non-empty is composited through the clip intersection.
        let mut clips = ClipStack::default();
        let mut scratch: Option<tiny_skia::Pixmap> = None;
        let mut effects = scratch::EffectScratch::default();

        for cmd in &scene.commands {
            match cmd {
                VectorCommand::PushClip(clip_path) => {
                    clips.push(clip_path, s_f32, dx, dy);
                    continue;
                }
                VectorCommand::PopClip => {
                    clips.pop();
                    continue;
                }
                _ => {}
            }

            if !clips.is_active() {
                draw_command(
                    &mut pixmap,
                    cmd,
                    assets,
                    s_f32,
                    dx,
                    dy,
                    scene.width_emu,
                    decode_plan.as_ref(),
                    &mut effects,
                );
                continue;
            }

            // Empty intersection: nothing inside this clip region can be seen.
            let Some((x0, y0, x1, y1)) = clips.device_bbox(width, height) else {
                continue;
            };
            let layer_w = (x1 - x0) as u32;
            let layer_h = (y1 - y0) as u32;
            if layer_w as usize * layer_h as usize > MAX_CLIP_LAYER_PIXELS {
                // Degrade to unclipped drawing rather than exhausting memory.
                draw_command(
                    &mut pixmap,
                    cmd,
                    assets,
                    s_f32,
                    dx,
                    dy,
                    scene.width_emu,
                    decode_plan.as_ref(),
                    &mut effects,
                );
                continue;
            }
            let Some(layer) = scratch_layer(&mut scratch, layer_w, layer_h) else {
                continue;
            };

            // Render the command into the bbox-sized scratch layer with the
            // same scale, rebased onto the layer origin.
            draw_command(
                layer,
                cmd,
                assets,
                s_f32,
                dx - x0 as f32,
                dy - y0 as f32,
                scene.width_emu,
                decode_plan.as_ref(),
                &mut effects,
            );

            let Some(mask) = clips.build_mask(LayerPlacement {
                origin_x: x0,
                origin_y: y0,
                width: layer_w,
                height: layer_h,
                scale: s_f32,
                offset_x: dx,
                offset_y: dy,
            }) else {
                continue;
            };
            // Mask the layer in its own coordinate space, then composite.
            layer.apply_mask(&mask);
            pixmap.draw_pixmap(
                x0,
                y0,
                layer.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }

        // Epilogue: Convert premultiplied pixmap to straight RGBA8
        let mut pixels = pixmap.take();
        unpremultiply_rgba8(&mut pixels);

        Ok(RgbaSlide {
            width,
            height,
            pixels: Arc::from(pixels.into_boxed_slice()),
        })
    }
}

/// Upper bound on one clipped command's scratch layer. Beyond this the command
/// is drawn unclipped instead of allocating an oversized buffer (mirrors the
/// shape-effect layer policy).
const MAX_CLIP_LAYER_PIXELS: usize = 32_000_000;

/// Dispatch one draw command into `pixmap` with the given scale/origin.
///
/// `scratch` carries the pass-level effect buffers reused across the
/// shape-effect renders of one raster pass (issue #33).
#[allow(clippy::too_many_arguments)]
fn draw_command(
    pixmap: &mut tiny_skia::Pixmap,
    cmd: &VectorCommand,
    assets: &dyn AssetStore,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    slide_width_emu: i64,
    decode_plan: Option<&decode_plan::DecodePlan>,
    scratch: &mut scratch::EffectScratch,
) {
    match cmd {
        VectorCommand::DrawPath {
            path,
            fill,
            stroke,
            transform,
            effects,
        } => {
            shape_effect::render_path_with_effects(
                pixmap,
                assets,
                path,
                fill.as_ref(),
                stroke.as_ref(),
                transform,
                effects,
                scale,
                offset_x,
                offset_y,
                slide_width_emu,
                scratch,
            );
        }
        VectorCommand::DrawLineEnd {
            path,
            fill,
            stroke,
            transform,
        } => {
            // A decoration is painted in its own tip frame with no effects of
            // its own (ref renderer.ts:4513-4535).
            shape_effect::render_path_with_effects(
                pixmap,
                assets,
                path,
                fill.as_ref(),
                stroke.as_ref(),
                transform,
                &ShapeEffects::default(),
                scale,
                offset_x,
                offset_y,
                slide_width_emu,
                scratch,
            );
        }
        VectorCommand::DrawImage {
            rel_id,
            src_rect,
            dest_rect,
            opacity,
            transform,
            effects,
        } => {
            let hint = decode_plan
                .map(|plan| plan.size_hint(rel_id))
                .unwrap_or(crate::loader::SizeHint::Native);
            image::render_image_with_hint(
                pixmap, assets, rel_id, hint, src_rect, dest_rect, *opacity, transform, effects, scale, offset_x,
                offset_y,
            );
        }
        VectorCommand::DrawText { runs, transform } => {
            text::render_text_runs(pixmap, runs, transform, scale, offset_x, offset_y, scratch);
        }
        VectorCommand::DrawBitmap {
            png,
            placement,
            opacity,
            sampling,
            ..
        } => {
            image::render_bitmap(pixmap, png, placement, *opacity, *sampling, scale, offset_x, offset_y);
        }
        VectorCommand::DrawMetafile {
            dest_rect, transform, ..
        } => {
            image::render_missing_metafile(pixmap, dest_rect, transform, scale, offset_x, offset_y);
        }
        VectorCommand::PushClip(_) | VectorCommand::PopClip => {}
    }
}

#[allow(clippy::chunks_exact_to_as_chunks)]
fn unpremultiply_rgba8(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a > 0 && a < 255 {
            let r = (chunk[0] as u32 * 255 + a / 2) / a;
            let g = (chunk[1] as u32 * 255 + a / 2) / a;
            let b = (chunk[2] as u32 * 255 + a / 2) / a;
            chunk[0] = r.min(255) as u8;
            chunk[1] = g.min(255) as u8;
            chunk[2] = b.min(255) as u8;
        }
    }
}
