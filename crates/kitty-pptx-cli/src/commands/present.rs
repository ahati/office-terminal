use crate::cli::DeckModeArg;
use crate::commands::source_from_path;
use kitty_pptx_core::*;
use kitty_pptx_display::*;
use kitty_pptx_tui::*;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

pub fn run_interactive(
    file_path: Option<PathBuf>,
    start_slide: Option<usize>,
    presenter: bool,
    mode: DeckModeArg,
    loader: &DefaultPresentationLoader,
    compiler: Arc<DefaultSlideCompiler>,
    rasterizer: Arc<TinySkiaRasterizer>,
) -> ExitCode {
    let file_path = match file_path {
        Some(f) => f,
        None => {
            eprintln!("Usage: kitty-pptx <FILE.pptx|FILE.pdf> or use --help for commands");
            return ExitCode::from(1);
        }
    };

    let deck = match loader.load_deck(source_from_path(file_path.clone())) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading presentation: {}", e);
            return ExitCode::from(2);
        }
    };

    let driver = match CapabilityProbe::probe_driver(PlacementMode::Interactive) {
        Ok(driver) => driver,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::from(3);
        }
    };

    let doc_title = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");

    let mut app = match deck {
        DeckSource::Pptx(presentation, asset_store) => {
            let pres_arc = Arc::new(presentation);
            let total = pres_arc.slides.len();
            let notes: Vec<Option<String>> =
                pres_arc.slides.iter().map(|s| s.notes.clone()).collect();
            let pipeline = Arc::new(DefaultRenderPipeline::new(
                pres_arc.clone(),
                asset_store,
                compiler,
                rasterizer,
            ));
            let mut a = PresentationApp::new(pipeline, driver, total, notes)
                .with_presenter_mode(presenter)
                .with_slides(pres_arc.slides.clone())
                .with_slide_dimensions(pres_arc.slide_width, pres_arc.slide_height)
                .with_deck_title(doc_title);
            if mode == DeckModeArg::Reader {
                let page_sizes = vec![
                    (
                        pres_arc.slide_width as f32 / 12700.0,
                        pres_arc.slide_height as f32 / 12700.0,
                    );
                    total
                ];
                a = a.with_reader_mode(true, total, page_sizes);
            }
            a
        }
        DeckSource::Pdf(pdf_source) => {
            let total = pdf_source.len();
            let toc = pdf_source.toc();
            let mut page_sizes = Vec::with_capacity(total);
            for i in 0..total {
                let (w_emu, h_emu) = pdf_source.page_size_emu(i);
                let pt_w = (w_emu as f64 / 12700.0) as f32;
                let pt_h = (h_emu as f64 / 12700.0) as f32;
                page_sizes.push((pt_w, pt_h));
            }

            let enable_reader = match mode {
                DeckModeArg::Reader => true,
                DeckModeArg::Slides => false,
                DeckModeArg::Auto => {
                    kitty_pptx_pdf::classify_mode(&page_sizes)
                        .map(|d| d.is_reader())
                        .unwrap_or(false)
                }
            };

            let pipeline = Arc::new(DefaultRenderPipeline::from_page_source(
                Arc::clone(&pdf_source),
                rasterizer,
            ));
            let notes = vec![None; total];
            let (first_w_emu, first_h_emu) = if total > 0 {
                pdf_source.page_size_emu(0)
            } else {
                (12_192_000, 6_858_000)
            };
            let mut a = PresentationApp::new(pipeline, driver, total, notes)
                .with_presenter_mode(presenter)
                .with_slide_dimensions(first_w_emu, first_h_emu)
                .with_deck_title(doc_title)
                .with_toc(toc);
            if enable_reader {
                a = a.with_reader_mode(true, total, page_sizes);
            }
            a
        }
    };

    if let Some(slide_1based) = start_slide {
        app = app.with_start_slide(slide_1based.saturating_sub(1));
    }

    if let Err(e) = app.run() {
        eprintln!("TUI Presentation Error: {}", e);
        return ExitCode::from(3);
    }

    ExitCode::SUCCESS
}
