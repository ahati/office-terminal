use std::sync::Arc;
use std::process::ExitCode;
use clap::Parser;
use kitty_pptx_core::*;
use kitty_pptx_display::*;
use kitty_pptx_tui::*;

use kitty_pptx_cli::cli::{Cli, Commands, ExportFormat, parse_slide_range};
use kitty_pptx_cli::config::Config;

fn main() -> ExitCode {
    let args = Cli::parse();
    let _config = Config::load();

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    match args.command {
        Some(Commands::Export { file, out, format, dpi, slides }) => {
            let (presentation, asset_store) = match loader.load(Source::File(file)) {
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
                let vector_slide = match compiler.compile(slide, presentation.slide_width, presentation.slide_height, &tx) {
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
                        let mut file = std::fs::File::create(&png_path).unwrap();
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
        Some(Commands::Info { file }) => {
            let (presentation, _) = match loader.load(Source::File(file.clone())) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("Error loading deck: {}", e);
                    return ExitCode::from(2);
                }
            };

            println!("File:           {}", file.display());
            println!("Slides:         {} slides", presentation.slides.len());
            println!("Dimensions:     {} x {} EMU", presentation.slide_width, presentation.slide_height);
            println!("Aspect Ratio:   {:.2}:1", presentation.slide_width as f64 / presentation.slide_height as f64);
            println!("Themes:         Office Theme");
            ExitCode::SUCCESS
        }
        Some(Commands::Dump { file, notes, comments: _, markdown: _, json: _ }) => {
            let (presentation, _) = match loader.load(Source::File(file)) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("Error loading deck: {}", e);
                    return ExitCode::from(2);
                }
            };

            if notes {
                for (i, s) in presentation.slides.iter().enumerate() {
                    println!("--- Slide {} Notes ---", i + 1);
                    if let Some(ref n) = s.notes {
                        println!("{}", n);
                    } else {
                        println!("(No notes)");
                    }
                }
            } else {
                println!("Presentation has {} slides. Use --notes, --comments, --markdown, or --json.", presentation.slides.len());
            }
            ExitCode::SUCCESS
        }
        Some(Commands::View { file, slide, all: _ }) => {
            let (presentation, asset_store) = match loader.load(Source::File(file)) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("Error loading deck: {}", e);
                    return ExitCode::from(2);
                }
            };

            let idx = slide.saturating_sub(1);
            if idx >= presentation.slides.len() {
                eprintln!("Slide index {} out of range (1..={})", slide, presentation.slides.len());
                return ExitCode::from(1);
            }

            let (tx, _rx) = crossbeam_channel::unbounded();
            let vector_slide = compiler.compile(&presentation.slides[idx], presentation.slide_width, presentation.slide_height, &tx).unwrap();
            let target = RenderTargetSpec { width_px: 1920, height_px: 1080 };
            let rgba = rasterizer.rasterize(&vector_slide, &target, asset_store.as_ref()).unwrap();

            let mut driver = CapabilityProbe::instantiate_driver(DetectedProtocol::Kitty, PlacementMode::Scrollback);
            driver.stage(&rgba, ImageId(1)).ok();
            driver.present(ImageId(1), Placement::ScrollbackBlock).ok();

            ExitCode::SUCCESS
        }
        None => {
            // Interactive TUI (default)
            let file_path = match args.file {
                Some(f) => f,
                None => {
                    eprintln!("Usage: kitty-pptx <FILE.pptx> or use --help for commands");
                    return ExitCode::from(1);
                }
            };

            let (presentation, asset_store) = match loader.load(Source::File(file_path)) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(2);
                }
            };

            let pres_arc = Arc::new(presentation);
            let pipeline = Arc::new(DefaultRenderPipeline::new(
                pres_arc.clone(),
                asset_store,
                compiler,
                rasterizer,
            ));

            let detected = CapabilityProbe::probe().unwrap_or(DetectedProtocol::Kitty);
            let driver = CapabilityProbe::instantiate_driver(detected, PlacementMode::Interactive);

            let notes: Vec<Option<String>> = pres_arc.slides.iter().map(|s| s.notes.clone()).collect();
            let mut app = PresentationApp::new(
                pipeline,
                driver,
                pres_arc.slides.len(),
                notes,
            );

            if let Err(e) = app.run() {
                eprintln!("TUI Presentation Error: {}", e);
                return ExitCode::from(3);
            }

            ExitCode::SUCCESS
        }
    }
}
