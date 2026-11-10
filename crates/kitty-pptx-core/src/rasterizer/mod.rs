pub mod effects;
mod image;
mod path;
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

        // 3. Render vector commands
        for cmd in &scene.commands {
            match cmd {
                VectorCommand::DrawPath {
                    path: v_path,
                    fill,
                    stroke,
                    transform,
                    ..
                } => {
                    path::render_path(
                        &mut pixmap,
                        v_path,
                        fill.as_ref(),
                        stroke.as_ref(),
                        transform,
                        s_f32,
                        dx,
                        dy,
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
                        &mut pixmap,
                        assets,
                        rel_id,
                        src_rect,
                        dest_rect,
                        *opacity,
                        transform,
                        effects,
                        s_f32,
                        dx,
                        dy,
                    );
                }
                VectorCommand::DrawText { runs, transform } => {
                    text::render_text_runs(
                        &mut pixmap,
                        runs,
                        transform,
                        s_f32,
                        dx,
                        dy,
                    );
                }
                VectorCommand::PushClip(_clip_path) => {
                    // Clip stack support: preserved for fidelity
                }
                VectorCommand::PopClip => {
                    // Pop clip
                }
            }
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
