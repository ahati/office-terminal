//! Production-pipeline rendering for the external comparison.
//!
//! Slides are compiled to the vector display list and rasterized through
//! `tiny-skia` exactly as the presenter does; only the reference PNGs come from
//! another renderer.

use std::path::Path;

use crossbeam_channel::unbounded;
use kitty_pptx_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec,
    RgbaSlide, SlideCompiler, SlideRasterizer, Source, TinySkiaRasterizer,
};

/// Render every slide of `path` at `width_px`, preserving the deck aspect ratio.
pub fn render_fixture(path: &Path, width_px: u32) -> Result<Vec<RgbaSlide>, String> {
    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path.to_path_buf()))
        .map_err(|error| format!("failed to load {}: {error}", path.display()))?;
    let height_px = (width_px as f64 * presentation.slide_height as f64
        / presentation.slide_width as f64)
        .round()
        .max(1.0) as u32;
    let target = RenderTargetSpec {
        width_px,
        height_px,
    };
    let compiler = DefaultSlideCompiler::new();
    let rasterizer = TinySkiaRasterizer::new();
    let (tx, _rx) = unbounded();

    let mut slides = Vec::with_capacity(presentation.slides.len());
    for (index, slide) in presentation.slides.iter().enumerate() {
        let vector = compiler
            .compile_in_deck(
                slide,
                &presentation.slides,
                presentation.slide_width,
                presentation.slide_height,
                &tx,
            )
            .map_err(|error| format!("{} slide {}: {error}", path.display(), index + 1))?;
        let rendered = rasterizer
            .rasterize(&vector, &target, assets.as_ref())
            .map_err(|error| format!("{} slide {}: {error}", path.display(), index + 1))?;
        slides.push(rendered);
    }
    Ok(slides)
}
