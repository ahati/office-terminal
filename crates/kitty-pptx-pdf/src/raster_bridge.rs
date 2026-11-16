use std::io::Cursor;
use std::sync::Arc;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::Pixmap;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use hayro_interpret::InterpreterSettings;
use hayro_syntax::page::Page;
use image::ImageEncoder;
use kitty_pptx_core::{
    AffineTransform, BitmapSampling, RgbaSlide, SlideLink, VectorBackground, VectorCommand,
    VectorSlide,
};

use crate::error::PdfError;
use crate::limits::calculate_canonical_scale;

/// Renders a PDF page to RGBA pixels at the given scale factor.
pub fn render_page_rgba<'a>(
    page: &'a Page<'a>,
    scale: f32,
    cache: &RenderCache<'a>,
) -> Result<RgbaSlide, PdfError> {
    let settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
    };

    let pixmap: Pixmap = render(page, cache, &settings, &render_settings, &pixmap_settings);
    let width = pixmap.width() as u32;
    let height = pixmap.height() as u32;

    if width == 0 || height == 0 {
        return Err(PdfError::RenderFailed {
            page: 0,
            detail: "Rendered pixmap has 0 dimensions".to_string(),
        });
    }

    // Convert premultiplied RGBA8 from pixmap to straight RGBA8
    let data = pixmap.data();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for p in data {
        let a = p.a;
        if a == 255 || a == 0 {
            pixels.push(p.r);
            pixels.push(p.g);
            pixels.push(p.b);
            pixels.push(a);
        } else {
            let alpha = a as f32 / 255.0;
            let r = ((p.r as f32 / alpha).min(255.0).round()) as u8;
            let g = ((p.g as f32 / alpha).min(255.0).round()) as u8;
            let b = ((p.b as f32 / alpha).min(255.0).round()) as u8;
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }
    }

    Ok(RgbaSlide {
        width,
        height,
        pixels: Arc::from(pixels.into_boxed_slice()),
    })
}

/// Compiles a PDF page into a Tier-1 VectorSlide wrapping an embedded raster bitmap (Phase R).
pub fn compile_page_raster<'a>(
    page: &'a Page<'a>,
    cache: &RenderCache<'a>,
    links: Vec<SlideLink>,
) -> Result<VectorSlide, PdfError> {
    let (w_pt, h_pt) = page.render_dimensions();
    let width_emu = (w_pt * 12_700.0).round() as i64;
    let height_emu = (h_pt * 12_700.0).round() as i64;

    let scale = calculate_canonical_scale(w_pt, h_pt);
    let slide = render_page_rgba(page, scale, cache)?;

    // Encode RGBA pixels to PNG
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
    encoder
        .write_image(
            &slide.pixels,
            slide.width,
            slide.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| PdfError::RenderFailed {
            page: 0,
            detail: format!("PNG encoding failed: {}", e),
        })?;

    let mut vector_slide = VectorSlide::new(width_emu, height_emu);
    vector_slide.background = Some(VectorBackground::Solid([255, 255, 255, 255]));
    vector_slide.commands.push(VectorCommand::DrawBitmap {
        png: Arc::from(png_bytes.into_boxed_slice()),
        width: slide.width,
        height: slide.height,
        placement: AffineTransform::scale(width_emu as f32, height_emu as f32),
        opacity: 1.0,
        sampling: BitmapSampling::Linear,
    });
    vector_slide.hyperlinks = links;

    Ok(vector_slide)
}
