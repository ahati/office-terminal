use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use kitty_pptx_core::*;
use kitty_pptx_display::*;
use crate::cli::{FitMode, PayloadMode};
use crate::commands::source_from_path;

#[allow(clippy::too_many_arguments)]
pub fn run_view(
    file: PathBuf,
    slide: usize,
    all: bool,
    fit: FitMode,
    _payload: PayloadMode,
    loader: &DefaultPresentationLoader,
    compiler: &Arc<DefaultSlideCompiler>,
    rasterizer: &Arc<TinySkiaRasterizer>,
) -> ExitCode {
    let is_tty = (std::io::stdout().is_terminal() || std::env::var("KITTY_PPTX_FORCE_TTY").is_ok())
        && std::env::var("KITTY_PPTX_FORCE_NOT_TTY").is_err();
    if !is_tty {
        eprintln!("Graphics command requires a valid TTY");
        return ExitCode::from(3);
    }

    let source = source_from_path(file);
    let (presentation, asset_store) = match loader.load(source) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading deck: {}", e);
            return ExitCode::from(2);
        }
    };

    let total = presentation.slides.len();
    if total == 0 {
        eprintln!("Presentation has no slides");
        return ExitCode::from(1);
    }

    let slide_indices: Vec<usize> = if all {
        (0..total).collect()
    } else {
        if slide == 0 || slide > total {
            eprintln!("Slide index {} out of range (1..={})", slide, total);
            return ExitCode::from(1);
        }
        vec![slide - 1]
    };

    let detected = CapabilityProbe::probe().unwrap_or(DetectedProtocol::HalfBlock);
    let mut driver = CapabilityProbe::instantiate_driver(detected, PlacementMode::Scrollback);

    let base_w = 1920;
    let base_h = if presentation.slide_width > 0 {
        ((base_w as f64 * presentation.slide_height as f64) / presentation.slide_width as f64).round() as u32
    } else {
        1080
    };
    let target = match fit {
        FitMode::Contain => RenderTargetSpec {
            width_px: base_w,
            height_px: 1080.min(base_h),
        },
        FitMode::Width => RenderTargetSpec {
            width_px: base_w,
            height_px: base_h,
        },
    };

    let (tx, _rx) = crossbeam_channel::unbounded();
    for &idx in &slide_indices {
        if all {
            println!("--- Slide {} / {} ---", idx + 1, total);
        }
        let vector_slide = match compiler.compile(
            &presentation.slides[idx],
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        ) {
            Ok(vs) => vs,
            Err(e) => {
                eprintln!("Compile error on slide {}: {}", idx + 1, e);
                return ExitCode::from(2);
            }
        };

        let rgba = match rasterizer.rasterize(&vector_slide, &target, asset_store.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Rasterize error on slide {}: {}", idx + 1, e);
                return ExitCode::from(2);
            }
        };

        if let Err(e) = driver.stage(&rgba, ImageId(1)) {
            eprintln!("Display staging error: {}", e);
            return ExitCode::from(3);
        }
        if let Err(e) = driver.present(ImageId(1), Placement::ScrollbackBlock) {
            eprintln!("Display presentation error: {}", e);
            return ExitCode::from(3);
        }
    }

    ExitCode::SUCCESS
}
