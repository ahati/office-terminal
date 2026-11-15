use crate::cli::FitMode;
use crate::commands::source_from_path;
use kitty_pptx_core::metafile::expand_metafiles;
use kitty_pptx_core::*;
use kitty_pptx_display::*;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
pub fn run_view(
    file: PathBuf,
    slide: usize,
    all: bool,
    fit: FitMode,
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

    let mut driver = match CapabilityProbe::probe_driver(PlacementMode::Scrollback) {
        Ok(driver) => driver,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::from(3);
        }
    };

    let target = fit_render_target(presentation.slide_width, presentation.slide_height, fit);

    let (tx, compile_rx) = crossbeam_channel::unbounded();
    for &idx in &slide_indices {
        if all {
            println!("--- Slide {} / {} ---", idx + 1, total);
        }
        let mut vector_slide = match compiler.compile_in_deck(
            &presentation.slides[idx],
            &presentation.slides,
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

        // Compile-stage diagnostics were previously dropped here; drain them
        // like the render pipeline and export path.
        for warning in compile_rx.try_iter() {
            eprintln!("Warning (slide {}): {:?}", idx + 1, warning);
        }

        // The scrollback view must expand metafile markers before rasterizing;
        // otherwise it shows the defensive placeholder while `export` plays the
        // same picture (PRD 13 §10.1).
        let (warning_tx, warning_rx) = crossbeam_channel::unbounded();
        expand_metafiles(&mut vector_slide, asset_store.as_ref(), &warning_tx);
        for warning in warning_rx.try_iter() {
            eprintln!("Warning (slide {}): {:?}", idx + 1, warning);
        }

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
fn fit_render_target(slide_width: i64, slide_height: i64, fit: FitMode) -> RenderTargetSpec {
    const MAX_WIDTH: u32 = 1920;
    const MAX_HEIGHT: u32 = 1080;

    if slide_width <= 0 || slide_height <= 0 {
        return RenderTargetSpec {
            width_px: MAX_WIDTH,
            height_px: MAX_HEIGHT,
        };
    }

    let aspect = slide_width as f64 / slide_height as f64;
    let base_height = (MAX_WIDTH as f64 / aspect).round().max(1.0) as u32;
    match fit {
        FitMode::Contain => {
            if base_height <= MAX_HEIGHT {
                RenderTargetSpec {
                    width_px: MAX_WIDTH,
                    height_px: base_height,
                }
            } else {
                RenderTargetSpec {
                    width_px: (MAX_HEIGHT as f64 * aspect).round().max(1.0) as u32,
                    height_px: MAX_HEIGHT,
                }
            }
        }
        FitMode::Width => RenderTargetSpec {
            width_px: MAX_WIDTH,
            height_px: base_height,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_preserves_slide_aspect_ratio() {
        let sixteen_nine = fit_render_target(12_192_000, 6_858_000, FitMode::Contain);
        assert_eq!(
            (sixteen_nine.width_px, sixteen_nine.height_px),
            (1920, 1080)
        );

        let four_three = fit_render_target(9_144_000, 6_858_000, FitMode::Contain);
        assert_eq!((four_three.width_px, four_three.height_px), (1440, 1080));

        let three_two = fit_render_target(12_192_000, 8_128_000, FitMode::Contain);
        assert_eq!((three_two.width_px, three_two.height_px), (1620, 1080));
    }

    #[test]
    fn width_fit_keeps_full_slide_height() {
        let four_three = fit_render_target(9_144_000, 6_858_000, FitMode::Width);
        assert_eq!((four_three.width_px, four_three.height_px), (1920, 1440));
    }
}
