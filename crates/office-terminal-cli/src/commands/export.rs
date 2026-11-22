use crate::cli::{parse_slide_range, ExportFormat};
use crate::commands::source_from_path;
use office_terminal_core::*;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

/// OOXML slide geometry is expressed in English Metric Units.
const EMU_PER_INCH: f64 = 914_400.0;

/// PNG raster size for a slide rendered at `dpi`.
pub fn export_target_size(slide_width_emu: i64, slide_height_emu: i64, dpi: u32) -> (u32, u32) {
    let width_emu = slide_width_emu.max(0) as f64;
    let scale = dpi as f64 / EMU_PER_INCH;
    let target_w = (width_emu * scale).round().max(1.0) as u32;
    export_target_size_for_width(slide_width_emu, slide_height_emu, target_w)
}

/// PNG raster size when the caller pins the pixel width (`--width`).
pub fn export_target_size_for_width(
    slide_width_emu: i64,
    slide_height_emu: i64,
    width_px: u32,
) -> (u32, u32) {
    let width_emu = slide_width_emu.max(0) as f64;
    let height_emu = slide_height_emu.max(0) as f64;
    let target_w = width_px.max(1);
    let target_h = if width_emu > 0.0 {
        (height_emu * (target_w as f64 / width_emu))
            .round()
            .max(1.0) as u32
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
    run_export_with_options(
        file,
        out,
        format,
        dpi,
        None,
        slides,
        SvgTextMode::VectorGlyphs,
        loader,
        compiler,
        rasterizer,
        svg_serializer,
    )
}

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
    run_export_with_options(
        file,
        out,
        format,
        dpi,
        width,
        slides,
        SvgTextMode::VectorGlyphs,
        loader,
        compiler,
        rasterizer,
        svg_serializer,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_export_with_options(
    file: PathBuf,
    out: PathBuf,
    format: ExportFormat,
    dpi: u32,
    width: Option<u32>,
    slides: Option<String>,
    svg_text_mode: SvgTextMode,
    loader: &DefaultPresentationLoader,
    compiler: &Arc<DefaultSlideCompiler>,
    rasterizer: &Arc<TinySkiaRasterizer>,
    svg_serializer: &Arc<DefaultSlideSvgSerializer>,
) -> ExitCode {
    let deck = match loader.load_deck(source_from_path(file)) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return ExitCode::from(2);
        }
    };

    let page_source: Arc<dyn PageSource> = match deck {
        DeckSource::Pptx(presentation, asset_store) => {
            Arc::new(PptxPageSource::new(Arc::new(presentation), asset_store, compiler.clone()))
        }
        DeckSource::Pdf(pdf_source) => pdf_source,
    };

    let total = page_source.len();
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

    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("Output directory error: {}: {}", out.display(), e);
        return ExitCode::from(2);
    }

    // Pixels come from the same unified render path the interactive presenter
    // uses (compile, metafile expansion, native raster sources, caching, and
    // warning collection behind `pipeline.render`). SVG still needs the
    // display list, so its arm compiles once through `compile_page` — which
    // already expands metafiles.
    let tiny_rasterizer: Arc<TinySkiaRasterizer> = Arc::clone(rasterizer);
    let pipeline =
        DefaultRenderPipeline::from_page_source(Arc::clone(&page_source), tiny_rasterizer);
    let mut failed = false;

    for idx in slide_indices {
        let (w_emu, h_emu) = page_source.page_size_emu(idx);

        // PNG Export
        if format == ExportFormat::Png || format == ExportFormat::All {
            let (target_w, target_h) = match width {
                Some(width_px) => export_target_size_for_width(
                    w_emu,
                    h_emu,
                    width_px,
                ),
                None => {
                    export_target_size(w_emu, h_emu, dpi)
                }
            };
            let target = RenderTargetSpec {
                width_px: target_w,
                height_px: target_h,
            };

            match pipeline.render(idx, &target) {
                Ok(rgba) => {
                    let png_path = out.join(format!("slide_{:03}.png", idx + 1));
                    let result = std::fs::File::create(&png_path).and_then(|mut file| {
                        image::write_buffer_with_format(
                            &mut file,
                            &rgba.pixels,
                            rgba.width,
                            rgba.height,
                            image::ColorType::Rgba8,
                            image::ImageFormat::Png,
                        )
                        .map_err(std::io::Error::other)
                    });
                    match result {
                        Ok(()) => println!("Exported {}", png_path.display()),
                        Err(e) => {
                            eprintln!(
                                "Render write failed on slide {}: {}: {}",
                                idx + 1,
                                png_path.display(),
                                e
                            );
                            failed = true;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Render failed on slide {}: {}", idx + 1, e);
                    failed = true;
                }
            }

            // Warnings collected by the render pipeline for this slide. In
            // All mode the SVG arm below re-compiles the same slide and prints
            // them once, so echoing them here as well would duplicate every
            // line on stderr.
            let warnings = pipeline.drain_warnings();
            if format == ExportFormat::Png {
                for (slide, warning) in warnings {
                    eprintln!("Warning (slide {}): {:?}", slide + 1, warning);
                }
            }
        }

        // SVG Export
        if format == ExportFormat::Svg || format == ExportFormat::All {
            let (tx, compile_rx) = crossbeam_channel::unbounded();
            let vector_slide = match page_source.compile_page(idx, &tx) {
                Ok(vs) => vs,
                Err(e) => {
                    eprintln!("Render compilation failed on slide {}: {}", idx + 1, e);
                    failed = true;
                    continue;
                }
            };
            for warning in compile_rx.try_iter() {
                eprintln!("Warning (slide {}): {:?}", idx + 1, warning);
            }

            // SVG serialization embeds referenced media itself, so it needs
            // the deck's asset store (raster-native sources have none).
            let assets = page_source.asset_store();
            let default_store = EmptyAssetStore;
            let store: &dyn AssetStore = assets.as_deref().unwrap_or(&default_store);

            match svg_serializer.serialize_svg(&vector_slide, store, svg_text_mode) {
                Ok(svg_content) => {
                    let svg_path = out.join(format!("slide_{:03}.svg", idx + 1));
                    match std::fs::write(&svg_path, svg_content) {
                        Ok(()) => println!("Exported {}", svg_path.display()),
                        Err(e) => {
                            eprintln!(
                                "Render write failed on slide {}: {}: {}",
                                idx + 1,
                                svg_path.display(),
                                e
                            );
                            failed = true;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("SVG serialization failed on slide {}: {}", idx + 1, e);
                    failed = true;
                }
            }
        }
    }

    if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
