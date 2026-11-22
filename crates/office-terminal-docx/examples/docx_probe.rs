//! Scratch end-to-end probe against real sample files (not committed).

use office_terminal_core::{PageSource, TinySkiaRasterizer, SlideRasterizer};
use office_terminal_docx::DocxPageSource;
use std::sync::Arc;

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe <file.docx>");
    let bytes: Arc<[u8]> = Arc::from(std::fs::read(&path).expect("read").into_boxed_slice());
    let started = std::time::Instant::now();
    let source = match DocxPageSource::open(Arc::clone(&bytes)) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("OPEN FAILED after {:?}: {error}", started.elapsed());
            std::process::exit(2);
        }
    };
    println!(
        "opened in {:?}: {} pages, first page {:?} EMU, toc: {} entries",
        started.elapsed(),
        source.len(),
        source.page_size_emu(0),
        source.toc().len()
    );

    let (tx, rx) = crossbeam_channel::unbounded::<office_terminal_core::contracts::WarningEvent>();
    let rasterizer = TinySkiaRasterizer::new();
    for index in 0..source.len().min(3) {
        let slide = match source.compile_page(index, &tx) {
            Ok(slide) => slide,
            Err(error) => {
                eprintln!("COMPILE FAILED on page {}: {error}", index + 1);
                std::process::exit(3);
            }
        };
        println!(
            "page {}: {} commands, {} hyperlinks, background {:?}",
            index + 1,
            slide.commands.len(),
            slide.hyperlinks.len(),
            slide.background.is_some()
        );
        let (w_emu, h_emu) = source.page_size_emu(index);
        let scale = 96.0 / 72.0;
        let target = office_terminal_core::contracts::RenderTargetSpec {
            width_px: (w_emu as f64 / 12700.0 * scale).round() as u32,
            height_px: (h_emu as f64 / 12700.0 * scale).round() as u32,
        };
        let store = source.asset_store().expect("store");
        match rasterizer.rasterize(&slide, &target, store.as_ref()) {
            Ok(bitmap) => {
                let out = format!("/tmp/docx-probe-{}.png", index + 1);
                let file = std::fs::File::create(&out).expect("create");
                let mut writer = std::io::BufWriter::new(file);
                image::write_buffer_with_format(
                    &mut writer,
                    &bitmap.pixels,
                    bitmap.width,
                    bitmap.height,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .expect("encode");
                println!("rasterized page {} -> {} ({}x{})", index + 1, out, bitmap.width, bitmap.height);
            }
            Err(error) => eprintln!("RASTER FAILED on page {}: {error}", index + 1),
        }
    }
    let warnings: Vec<_> = rx.try_iter().collect();
    for warning in &warnings {
        println!("warning: {warning:?}");
    }
    println!("total warnings: {}", warnings.len());
    let corpus = source.search_corpus();
    for page in 0..corpus.page_count().min(2) {
        println!("--- corpus page {} ---", page + 1);
        for line in corpus.page_text(page).iter().take(12) {
            println!("  {:?}", line.text);
        }
    }
}
