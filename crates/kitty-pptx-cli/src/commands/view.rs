use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use kitty_pptx_core::*;
use kitty_pptx_display::*;

pub fn run_view(
    file: PathBuf,
    slide: usize,
    _all: bool,
    loader: &DefaultPresentationLoader,
    compiler: &Arc<DefaultSlideCompiler>,
    rasterizer: &Arc<TinySkiaRasterizer>,
) -> ExitCode {
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
    let vector_slide = match compiler.compile(
        &presentation.slides[idx],
        presentation.slide_width,
        presentation.slide_height,
        &tx,
    ) {
        Ok(vs) => vs,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            return ExitCode::from(2);
        }
    };

    let target = RenderTargetSpec { width_px: 1920, height_px: 1080 };
    let rgba = match rasterizer.rasterize(&vector_slide, &target, asset_store.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Rasterize error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut driver = CapabilityProbe::instantiate_driver(DetectedProtocol::Kitty, PlacementMode::Scrollback);
    driver.stage(&rgba, ImageId(1)).ok();
    driver.present(ImageId(1), Placement::ScrollbackBlock).ok();

    ExitCode::SUCCESS
}
