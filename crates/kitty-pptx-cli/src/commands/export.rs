use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use kitty_pptx_core::*;
use crate::cli::{ExportFormat, parse_slide_range};
use crate::commands::source_from_path;

/// OOXML slide geometry is expressed in English Metric Units.
const EMU_PER_INCH: f64 = 914_400.0;

/// PNG raster size for a slide rendered at `dpi`.
///
/// Ports the reference canvas contract (`renderer.ts:8367-8370`):
/// `scale = targetWidth / slideWidth`, `canvasW = Math.round(targetWidth)`,
/// `canvasH = Math.round(slideHeight * scale)`. We derive `targetWidth` from the
/// requested DPI (96 DPI = one CSS pixel per inch) and then derive the height
/// from the rounded width so the slide aspect stays exact instead of drifting.
/// `Math.round` is round-half-up; `f64::round` matches for non-negative sizes.
pub fn export_target_size(slide_width_emu: i64, slide_height_emu: i64, dpi: u32) -> (u32, u32) {
    let width_emu = slide_width_emu.max(0) as f64;
    let scale = dpi as f64 / EMU_PER_INCH;
    let target_w = (width_emu * scale).round().max(1.0) as u32;
    export_target_size_for_width(slide_width_emu, slide_height_emu, target_w)
}

/// PNG raster size when the caller pins the pixel width (`--width`).
///
/// `target_w = width`, `target_h = round(slide_h / slide_w * width)`; this is the
/// reference contract with `targetWidth` supplied by the caller
/// (`renderer.ts:8367-8370`), which is how the 1600x1066 reference canvas for
/// the 8640763x5759450 LibreOffice fixture is produced.
pub fn export_target_size_for_width(
    slide_width_emu: i64,
    slide_height_emu: i64,
    width_px: u32,
) -> (u32, u32) {
    let width_emu = slide_width_emu.max(0) as f64;
    let height_emu = slide_height_emu.max(0) as f64;
    let target_w = width_px.max(1);
    // ref renderer.ts:8370: canvasH = Math.round(slideHeight * scale), with
    // scale taken over the caller-requested width.
    let target_h = if width_emu > 0.0 {
        (height_emu * (target_w as f64 / width_emu)).round().max(1.0) as u32
    } else {
        target_w
    };
    (target_w, target_h)
}

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
    run_export_with_width(
        file,
        out,
        format,
        dpi,
        None,
        slides,
        loader,
        compiler,
        rasterizer,
        svg_serializer,
    )
}

/// Export entry point honouring an explicit pixel width (`--width`).
///
/// `width` takes precedence over `dpi` for PNG targets; the height always
/// follows the slide aspect ratio.
#[allow(clippy::too_many_arguments)]
pub fn run_export_with_width(
    file: PathBuf,
    out: PathBuf,
    format: ExportFormat,
    dpi: u32,
    width: Option<u32>,
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
            let (target_w, target_h) = match width {
                Some(width_px) => export_target_size_for_width(
                    presentation.slide_width,
                    presentation.slide_height,
                    width_px,
                ),
                None => export_target_size(presentation.slide_width, presentation.slide_height, dpi),
            };
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
