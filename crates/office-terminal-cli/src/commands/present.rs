use crate::cli::DeckModeArg;
use crate::commands::source_from_path;
use office_terminal_core::*;
use office_terminal_display::*;
use office_terminal_tui::*;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

/// Background raster worker count for the interactive pipeline.
///
/// The sorter grid resolves a raster per visible card, so a wider worker pool
/// directly shortens first-window fill. Sorter rasters are ~1280 px wide
/// (~3.7–5.1 MB each), keeping peak in-flight memory modest even at the cap.
fn background_workers() -> usize {
    std::thread::available_parallelism()
        .map_or(4, |cpus| cpus.get())
        .clamp(2, 6)
}

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
            eprintln!("Usage: ot <FILE.pptx|FILE.pdf> or use --help for commands");
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

    // Resolve the terminal's pixel geometry while the input stream is still
    // ours: the CSI fallback probes (for terminals whose TIOCGWINSZ reports
    // zero pixels) round-trip bytes once, here, before the TUI owns stdin.
    // Resizes later re-resolve via the ioctl, keeping the sticky CSI cell.
    office_terminal_display::probe::geometry::prime_cell_metrics();

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
            // Interactive renders are worker-async; the loop stages rasters
            // on worker completion events, and the first frame is rendered
            // synchronously below (PresentationApp::run). Sync fallback stays
            // off (the DefaultRenderPipeline default), so a cache-missing page
            // turn keeps the previous slide on screen instead of rasterizing
            // synchronously on the UI thread and freezing input.
            let pipeline = Arc::new(
                DefaultRenderPipeline::with_worker_count(
                    pres_arc.clone(),
                    asset_store,
                    compiler,
                    rasterizer,
                    background_workers(),
                )
                .with_transmit_encoder(Arc::new(office_terminal_display::KittyTransmitEncoder)),
            );
            // Background workers pre-encode each raster's transmission
            // payload; the UI loop consumes one completion event per raster.
            let render_events = pipeline.completion_receiver();
            let mut a = PresentationApp::new(pipeline, driver, total, notes)
                .with_presenter_mode(presenter)
                .with_slides(pres_arc.slides.clone())
                .with_slide_dimensions(pres_arc.slide_width, pres_arc.slide_height)
                .with_deck_title(doc_title);
            if let Some(rx) = render_events {
                a = a.with_render_events(rx);
            }
            if mode == DeckModeArg::Reader {
                let page_sizes = vec![
                    (
                        pres_arc.slide_width as f32 / EMU_PER_PT,
                        pres_arc.slide_height as f32 / EMU_PER_PT,
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
                let pt_w = (w_emu as f64 / f64::from(EMU_PER_PT)) as f32;
                let pt_h = (h_emu as f64 / f64::from(EMU_PER_PT)) as f32;
                page_sizes.push((pt_w, pt_h));
            }

            let enable_reader = match mode {
                DeckModeArg::Reader => true,
                DeckModeArg::Slides => false,
                DeckModeArg::Auto => office_terminal_pdf::classify_mode(&page_sizes)
                    .map(|d| d.is_reader())
                    .unwrap_or(false),
            };

            // Same async contract as the PPTX pipeline above: interactive
            // renders are worker-async, the loop stages rasters on
            // completion events, and the first frame is rendered
            // synchronously below (PresentationApp::run). Sync fallback
            // stays off so page turns never rasterize on the UI thread.
            let pipeline = Arc::new(
                DefaultRenderPipeline::from_page_source_with_workers(
                    Arc::clone(&pdf_source),
                    rasterizer,
                    background_workers(),
                )
                .with_transmit_encoder(Arc::new(office_terminal_display::KittyTransmitEncoder)),
            );
            // Same off-thread staging contract as the PPTX pipeline above.
            let render_events = pipeline.completion_receiver();
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
            if let Some(rx) = render_events {
                a = a.with_render_events(rx);
            }
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
