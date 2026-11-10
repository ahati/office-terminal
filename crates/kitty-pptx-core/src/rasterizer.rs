use std::sync::Arc;
use crate::contracts::*;
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

impl SlideRasterizer for TinySkiaRasterizer {
    fn rasterize(
        &self,
        scene: &VectorSlide,
        target: &RenderTargetSpec,
        _assets: &dyn AssetStore,
    ) -> Result<RgbaSlide, RenderError> {
        let width = target.width_px;
        let height = target.height_px;

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| RenderError::RasterizationError("failed to allocate pixmap".to_string()))?;

        // 1. Uniform scale factor
        let s = (width as f64 / scene.width_emu as f64)
            .min(height as f64 / scene.height_emu as f64);
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
                _ => {
                    pixmap.fill(tiny_skia::Color::WHITE);
                }
            }
        } else {
            pixmap.fill(tiny_skia::Color::WHITE);
        }

        // 3. Render vector commands
        for cmd in &scene.commands {
            match cmd {
                VectorCommand::DrawPath { path, fill, stroke, transform } => {
                    let mut pb = tiny_skia::PathBuilder::new();
                    for seg in &path.segments {
                        match seg {
                            PathSegment::MoveTo { x, y } => {
                                pb.move_to(dx + (transform.tx + x) * s_f32, dy + (transform.ty + y) * s_f32);
                            }
                            PathSegment::LineTo { x, y } => {
                                pb.line_to(dx + (transform.tx + x) * s_f32, dy + (transform.ty + y) * s_f32);
                            }
                            PathSegment::QuadTo { x1, y1, x, y } => {
                                pb.quad_to(
                                    dx + (transform.tx + x1) * s_f32, dy + (transform.ty + y1) * s_f32,
                                    dx + (transform.tx + x) * s_f32, dy + (transform.ty + y) * s_f32,
                                );
                            }
                            PathSegment::CubicTo { x1, y1, x2, y2, x, y } => {
                                pb.cubic_to(
                                    dx + (transform.tx + x1) * s_f32, dy + (transform.ty + y1) * s_f32,
                                    dx + (transform.tx + x2) * s_f32, dy + (transform.ty + y2) * s_f32,
                                    dx + (transform.tx + x) * s_f32, dy + (transform.ty + y) * s_f32,
                                );
                            }
                            PathSegment::Close => {
                                pb.close();
                            }
                        }
                    }

                    if let Some(ts_path) = pb.finish() {
                        if let Some(ref f) = fill {
                            let mut paint = tiny_skia::Paint::default();
                            match f {
                                VectorPaint::Solid([r, g, b, a]) => {
                                    paint.set_color_rgba8(*r, *g, *b, *a);
                                }
                                _ => {
                                    paint.set_color_rgba8(200, 200, 200, 255);
                                }
                            }
                            pixmap.fill_path(&ts_path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
                        }

                        if let Some(ref s) = stroke {
                            let mut paint = tiny_skia::Paint::default();
                            let [r, g, b, a] = s.color;
                            paint.set_color_rgba8(r, g, b, a);
                            let width_px = (s.width_emu as f32 * s_f32).max(1.0);
                            let ts_stroke = tiny_skia::Stroke {
                                width: width_px,
                                ..Default::default()
                            };
                            pixmap.stroke_path(&ts_path, &paint, &ts_stroke, tiny_skia::Transform::identity(), None);
                        }
                    }
                }
                _ => {}
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
