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
    VectorSlide, EMU_PER_PT,
};

use crate::error::PdfError;
use crate::limits::calculate_canonical_scale;

/// Renders a PDF page to RGBA pixels at the given scale factor.
///
/// The hayro pixmap is premultiplied RGBA8; the returned slide holds
/// straight (non-premultiplied) RGBA8 as required by the `RgbaSlide`
/// contract.
pub fn render_page_rgba<'a>(
    page: &'a Page<'a>,
    page_index: usize,
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
            page: page_index,
            detail: "Rendered pixmap has 0 dimensions".to_string(),
        });
    }

    // Convert premultiplied RGBA8 to straight RGBA8. `PremulRgba8` is
    // `#[repr(C)] { r, g, b, a }`, so the raw byte slice yields exactly one
    // `[r, g, b, a]` chunk per pixel. The output is pre-sized and uses only
    // integer math: un-premultiply is round(c_p * 255 / a), computed as
    // `(c_p * 255 + a / 2) / a` (rounds half-up, within parity tolerance).
    let data = pixmap.data_as_u8_slice();
    let mut pixels = Vec::with_capacity(data.len());

    let (chunks, _remainder) = data.as_chunks::<4>();
    for px in chunks {
        let a = px[3];
        if a == 255 || a == 0 {
            pixels.extend_from_slice(&[px[0], px[1], px[2], a]);
        } else {
            let a32 = a as u32;
            let half = a32 / 2;
            let unpremul = |c: u8| ((c as u32 * 255 + half) / a32).min(255) as u8;
            pixels.extend_from_slice(&[unpremul(px[0]), unpremul(px[1]), unpremul(px[2]), a]);
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
    page_index: usize,
    cache: &RenderCache<'a>,
    links: Vec<SlideLink>,
) -> Result<VectorSlide, PdfError> {
    let (w_pt, h_pt) = page.render_dimensions();
    let width_emu = (w_pt * EMU_PER_PT).round() as i64;
    let height_emu = (h_pt * EMU_PER_PT).round() as i64;

    let scale = calculate_canonical_scale(w_pt, h_pt);
    let slide = render_page_rgba(page, page_index, scale, cache)?;

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
            page: page_index,
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
