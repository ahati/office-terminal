use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use kitty_pptx_core::*;
use crate::cli::{ExportFormat, parse_slide_range};
use crate::commands::source_from_path;

#[allow(clippy::too_many_arguments)]
pub fn run_export(
    file: PathBuf,
    out: PathBuf,
    format: ExportFormat,
    dpi: u32,
    slides: Option<String>,
    loader: &DefaultPresentationLoader,
    compiler: &Arc<DefaultSlideCompiler>,
    rasterizer: &Arc<TinySkiaRasterizer>,
    svg_serializer: &Arc<DefaultSlideSvgSerializer>,
) -> ExitCode {
    let (presentation, asset_store) = match loader.load(source_from_path(file)) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return ExitCode::from(2);
        }
    };

    let total = presentation.slides.len();
    let slide_indices = if let Some(ref range_str) = slides {
        match parse_slide_range(range_str, total) {
            Ok(idxs) => idxs,
            Err(e) => {
                eprintln!("Configuration error: {}", e);
                return ExitCode::from(1);
            }
        }
    } else {
        (0..total).collect()
    };

    std::fs::create_dir_all(&out).ok();
    let (tx, _rx) = crossbeam_channel::unbounded();

    for idx in slide_indices {
        let slide = &presentation.slides[idx];
        let vector_slide = match compiler.compile_in_deck(slide, &presentation.slides, presentation.slide_width, presentation.slide_height, &tx) {
            Ok(vs) => vs,
            Err(e) => {
                eprintln!("Render compilation failed on slide {}: {}", idx + 1, e);
                continue;
            }
        };

        // PNG Export
        if format == ExportFormat::Png || format == ExportFormat::All {
            let target_w = (presentation.slide_width as f64 / 914400.0 * dpi as f64) as u32;
            let target_h = (presentation.slide_height as f64 / 914400.0 * dpi as f64) as u32;
            let target = RenderTargetSpec { width_px: target_w, height_px: target_h };

            if let Ok(rgba) = rasterizer.rasterize(&vector_slide, &target, asset_store.as_ref()) {
                let png_path = out.join(format!("slide_{:03}.png", idx + 1));
                if let Ok(mut file) = std::fs::File::create(&png_path) {
                    let _ = image::write_buffer_with_format(
                        &mut file,
                        &rgba.pixels,
                        rgba.width,
                        rgba.height,
                        image::ColorType::Rgba8,
                        image::ImageFormat::Png,
                    );
                    println!("Exported {}", png_path.display());
                }
            }
        }

        // SVG Export
        if format == ExportFormat::Svg || format == ExportFormat::All {
            if let Ok(svg_content) = svg_serializer.serialize_svg(&vector_slide, asset_store.as_ref(), SvgTextMode::VectorGlyphs) {
                let svg_path = out.join(format!("slide_{:03}.svg", idx + 1));
                std::fs::write(&svg_path, svg_content).ok();
                println!("Exported {}", svg_path.display());
            }
        }
    }

    ExitCode::SUCCESS
}
