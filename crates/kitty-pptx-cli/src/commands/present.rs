use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use kitty_pptx_core::*;
use kitty_pptx_display::*;
use kitty_pptx_tui::*;

pub fn run_interactive(
    file_path: Option<PathBuf>,
    loader: &DefaultPresentationLoader,
    compiler: Arc<DefaultSlideCompiler>,
    rasterizer: Arc<TinySkiaRasterizer>,
) -> ExitCode {
    let file_path = match file_path {
        Some(f) => f,
        None => {
            eprintln!("Usage: kitty-pptx <FILE.pptx> or use --help for commands");
            return ExitCode::from(1);
        }
    };

    let (presentation, asset_store) = match loader.load(Source::File(file_path)) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading presentation: {}", e);
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
