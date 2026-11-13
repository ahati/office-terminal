pub mod effects;
pub mod filters;
mod bevel;
mod image;
mod mask;
mod path;
mod shape_effect;
mod text;

use std::sync::Arc;
use crate::contracts::{
    FillRule, PathSegment, RenderTargetSpec, RgbaSlide, VectorBackground,
    VectorCommand, VectorPaint, VectorPath, VectorSlide,
};
use crate::error::RenderError;
use crate::loader::AssetStore;

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

        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
            RenderError::RasterizationError("failed to allocate pixmap".to_string())
        })?;

        // 1. Uniform scale factor
        let s = (width as f64 / scene.width_emu.max(1) as f64)
            .min(height as f64 / scene.height_emu.max(1) as f64);
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
                        &bg_path,
                        Some(&VectorPaint::LinearGradient(grad.clone())),
                        None,
                        &crate::contracts::AffineTransform::default(),
                        s_f32,
                        dx,
                        dy,
                    );
                }
                VectorBackground::Image { rel_id } => {
                    let dest = crate::contracts::RectF32::new(
                        0.0,
                        0.0,
                        scene.width_emu as f32,
                        scene.height_emu as f32,
                    );
                    image::render_image(
                        &mut pixmap,
                        assets,
                        rel_id,
                        &crate::contracts::NormalizedRect::default(),
                        &dest,
                        1.0,
                        &crate::contracts::AffineTransform::default(),
                        &[],
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
                draw_command(&mut pixmap, cmd, assets, s_f32, dx, dy, scene.width_emu);
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
                draw_command(&mut pixmap, cmd, assets, s_f32, dx, dy, scene.width_emu);
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
#[allow(clippy::too_many_arguments)]
fn draw_command(
    pixmap: &mut tiny_skia::Pixmap,
    cmd: &VectorCommand,
    assets: &dyn AssetStore,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    slide_width_emu: i64,
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
                path,
                fill.as_ref(),
                stroke.as_ref(),
                transform,
                effects,
                scale,
                offset_x,
                offset_y,
                slide_width_emu,
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
            image::render_image(
                pixmap,
                assets,
                rel_id,
                src_rect,
                dest_rect,
                *opacity,
                transform,
                effects,
                scale,
                offset_x,
                offset_y,
            );
        }
        VectorCommand::DrawText { runs, transform } => {
            text::render_text_runs(pixmap, runs, transform, scale, offset_x, offset_y);
        }
        VectorCommand::PushClip(_) | VectorCommand::PopClip => {}
    }
}

/// One active clip region, in device-pixel bounds.
struct ClipLayer {
    path: VectorPath,
    /// `(min_x, min_y, max_x, max_y)` in canvas pixels (pre-clamp).
    bounds: (f32, f32, f32, f32),
}

/// Placement of a scratch layer plus the canvas mapping used to build masks.
#[derive(Clone, Copy)]
struct LayerPlacement {
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

/// Nested clip regions declared by `PushClip` / `PopClip` pairs.
#[derive(Default)]
struct ClipStack {
    /// `None` entries are no-op pushes (empty silhouettes); they still occupy a
    /// slot so an unbalanced-looking `PopClip` stays matched.
    layers: Vec<Option<ClipLayer>>,
}

impl ClipStack {
    fn push(&mut self, path: &VectorPath, scale: f32, offset_x: f32, offset_y: f32) {
        // An empty silhouette carries no region to clip by (and tiny-skia
        // refuses to fill degenerate paths), so treat it as a no-op.
        if path.segments.is_empty() {
            self.layers.push(None);
            return;
        }
        match clip_bounds_px(path, scale, offset_x, offset_y) {
            Some(bounds) => self.layers.push(Some(ClipLayer {
                path: path.clone(),
                bounds,
            })),
            None => self.layers.push(None),
        }
    }

    fn pop(&mut self) {
        self.layers.pop();
    }

    fn active(&self) -> impl Iterator<Item = &ClipLayer> {
        self.layers.iter().flatten()
    }

    fn is_active(&self) -> bool {
        self.layers.iter().any(Option::is_some)
    }

    /// Intersection bounding box in whole pixels, clamped to the canvas.
    fn device_bbox(&self, width: u32, height: u32) -> Option<(i32, i32, i32, i32)> {
        let mut iter = self.active();
        let first = iter.next()?;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = first.bounds;
        for layer in iter {
            min_x = min_x.max(layer.bounds.0);
            min_y = min_y.max(layer.bounds.1);
            max_x = max_x.min(layer.bounds.2);
            max_y = max_y.min(layer.bounds.3);
        }
        // One pixel of slack so the silhouette's anti-aliased edge survives.
        let x0 = (min_x - 1.0).floor().clamp(0.0, width as f32);
        let y0 = (min_y - 1.0).floor().clamp(0.0, height as f32);
        let x1 = (max_x + 1.0).ceil().clamp(0.0, width as f32);
        let y1 = (max_y + 1.0).ceil().clamp(0.0, height as f32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0 as i32, y0 as i32, x1 as i32, y1 as i32))
    }

    /// Intersection mask of every active clip, in scratch-layer coordinates.
    fn build_mask(&self, place: LayerPlacement) -> Option<tiny_skia::Mask> {
        let local_x = place.offset_x - place.origin_x as f32;
        let local_y = place.offset_y - place.origin_y as f32;
        let mut mask: Option<tiny_skia::Mask> = None;
        for layer in self.active() {
            let rule = match layer.path.fill_rule {
                FillRule::NonZero => tiny_skia::FillRule::Winding,
                FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
            };
            let Some(ts_path) = device_path(&layer.path, place.scale, local_x, local_y) else {
                // Degenerate silhouette clips everything away.
                return tiny_skia::Mask::new(place.width, place.height);
            };
            match &mut mask {
                Some(existing) => existing.intersect_path(
                    &ts_path,
                    rule,
                    true,
                    tiny_skia::Transform::identity(),
                ),
                None => {
                    let mut fresh = tiny_skia::Mask::new(place.width, place.height)?;
                    fresh.fill_path(
                        &ts_path,
                        rule,
                        true,
                        tiny_skia::Transform::identity(),
                    );
                    mask = Some(fresh);
                }
            }
        }
        mask
    }
}

/// Conservative device-pixel bounds of a slide-EMU clip path.
fn clip_bounds_px(
    path: &VectorPath,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (min_x, min_y, max_x, max_y) = path::local_bounds(path)?;
    let bounds = (
        offset_x + min_x * scale,
        offset_y + min_y * scale,
        offset_x + max_x * scale,
        offset_y + max_y * scale,
    );
    let (x0, y0, x1, y1) = bounds;
    if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
        Some(bounds)
    } else {
        None
    }
}

/// Convert a slide-EMU path into a device-space `tiny_skia::Path`.
fn device_path(
    path: &VectorPath,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<tiny_skia::Path> {
    let map = |x: f32, y: f32| -> Option<(f32, f32)> {
        let px = offset_x + x * scale;
        let py = offset_y + y * scale;
        (px.is_finite() && py.is_finite()).then_some((px, py))
    };
    let mut builder = tiny_skia::PathBuilder::new();
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo { x, y } => {
                let (px, py) = map(x, y)?;
                builder.move_to(px, py);
            }
            PathSegment::LineTo { x, y } => {
                let (px, py) = map(x, y)?;
                builder.line_to(px, py);
            }
            PathSegment::QuadTo { x1, y1, x, y } => {
                let (c1x, c1y) = map(x1, y1)?;
                let (px, py) = map(x, y)?;
                builder.quad_to(c1x, c1y, px, py);
            }
            PathSegment::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1x, c1y) = map(x1, y1)?;
                let (c2x, c2y) = map(x2, y2)?;
                let (px, py) = map(x, y)?;
                builder.cubic_to(c1x, c1y, c2x, c2y, px, py);
            }
            PathSegment::Close => {
                builder.close();
            }
        }
    }
    builder.finish()
}

/// Borrow a cleared scratch layer of exactly `width` x `height`, reusing the
/// allocation across commands when the size did not change.
fn scratch_layer(
    slot: &mut Option<tiny_skia::Pixmap>,
    width: u32,
    height: u32,
) -> Option<&mut tiny_skia::Pixmap> {
    let needs_alloc = match slot {
        Some(pixmap) => pixmap.width() != width || pixmap.height() != height,
        None => true,
    };
    if needs_alloc {
        *slot = Some(tiny_skia::Pixmap::new(width, height)?);
    }
    let pixmap = slot.as_mut()?;
    pixmap.data_mut().fill(0);
    Some(pixmap)
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
