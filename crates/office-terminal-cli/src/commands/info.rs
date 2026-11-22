use std::path::PathBuf;
use std::process::ExitCode;
use office_terminal_core::{DeckSource, DefaultPresentationLoader, PresentationLoader, EMU_PER_PT, EMU_PER_PX};
use crate::commands::source_from_path;

pub fn run_info(file: PathBuf, loader: &DefaultPresentationLoader) -> ExitCode {
    let source = source_from_path(file.clone());
    let deck = match loader.load_deck(source) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading deck: {}", e);
            return ExitCode::from(2);
        }
    };

    let file_str = if file.as_os_str() == "-" {
        "<stdin>".to_string()
    } else {
        file.display().to_string()
    };
    println!("File:           {}", file_str);

    match deck {
        DeckSource::Pptx(presentation, asset_store) => {
            let total = presentation.slides.len();
            let hidden = presentation.slides.iter().filter(|s| s.hidden).count();
            if hidden > 0 {
                println!("Slides:         {} slides ({} hidden)", total, hidden);
            } else {
                println!("Slides:         {} slides", total);
            }

            let w_emu = presentation.slide_width;
            let h_emu = presentation.slide_height;
            println!("Dimensions:     {} x {} EMU", w_emu, h_emu);

            let w_px = (w_emu as f64 / f64::from(EMU_PER_PX)).round() as u64;
            let h_px = (h_emu as f64 / f64::from(EMU_PER_PX)).round() as u64;
            let ratio_str = format_aspect_ratio(w_emu, h_emu);
            println!("Aspect Ratio:   {} ({} x {} px at 96 DPI base)", ratio_str, w_px, h_px);

            let theme_str = match (&presentation.major_font, &presentation.minor_font) {
                (Some(maj), Some(min)) => format!("Office Theme ({}, {})", maj, min),
                (Some(maj), None) => format!("Office Theme ({})", maj),
                (None, Some(min)) => format!("Office Theme ({})", min),
                (None, None) => "Office Theme".to_string(),
            };
            println!("Themes:         {}", theme_str);

            let mut slide_media_count = 0;
            for s in &presentation.slides {
                for el in &s.elements {
                    if matches!(el, pptx_model::SlideElement::Picture(_) | pptx_model::SlideElement::Media(_)) {
                        slide_media_count += 1;
                    }
                }
            }
            let stats = asset_store.stats();
            let count = if stats.cached_entries > 0 { stats.cached_entries } else { slide_media_count };
            let mb = stats.cached_bytes as f64 / (1024.0 * 1024.0);
            println!("Media Assets:   {} images ({:.1} MB)", count, mb);
        }
        DeckSource::Pdf(pdf_source) => {
            let total = pdf_source.len();
            println!("Pages:          {} pages", total);
            if total > 0 {
                let (w_emu, h_emu) = pdf_source.page_size_emu(0);
                let pt_w = w_emu as f64 / f64::from(EMU_PER_PT);
                let pt_h = h_emu as f64 / f64::from(EMU_PER_PT);
                println!("Dimensions:     {:.1} x {:.1} pt ({:.0} x {:.0} mm)", pt_w, pt_h, pt_w * 25.4 / 72.0, pt_h * 25.4 / 72.0);
                let ratio_str = format_aspect_ratio(w_emu, h_emu);
                println!("Aspect Ratio:   {}", ratio_str);
            }
            let toc = pdf_source.toc();
            if !toc.is_empty() {
                println!("Bookmarks:      {} TOC entries", toc.len());
            }
        }
    }

    print_terminal_geometry();

    ExitCode::SUCCESS
}

/// Prints the resolved terminal pixel geometry and its source.
///
/// Runs the full resolution ladder once (env override → TIOCGWINSZ pixels →
/// CSI 16 t → CSI 14 t ÷ grid → default) so a remote session can be
/// diagnosed in one command. Outside a terminal the CSI layers find no
/// controlling tty and fall through instantly.
fn print_terminal_geometry() {
    let resolved = office_terminal_display::probe::geometry::prime_cell_metrics();
    let cell = resolved.size;
    match crossterm::terminal::size() {
        Ok((cols, rows)) => {
            let vw = (cols as f64 * cell.width_px).round() as u64;
            let vh = (rows as f64 * cell.height_px).round() as u64;
            println!(
                "Terminal:       cell {:.0}x{:.0} px via {}, grid {}x{} = {}x{} px",
                cell.width_px,
                cell.height_px,
                resolved.source.label(),
                cols,
                rows,
                vw,
                vh
            );
        }
        Err(_) => println!(
            "Terminal:       cell {:.0}x{:.0} px via {} (no tty grid)",
            cell.width_px,
            cell.height_px,
            resolved.source.label()
        ),
    }
}

fn format_aspect_ratio(w: i64, h: i64) -> String {
    if h == 0 {
        return "0:0".to_string();
    }
    let ratio = w as f64 / h as f64;
    if (ratio - 16.0 / 9.0).abs() < 0.02 {
        "16:9".to_string()
    } else if (ratio - 4.0 / 3.0).abs() < 0.02 {
        "4:3".to_string()
    } else if (ratio - 16.0 / 10.0).abs() < 0.02 {
        "16:10".to_string()
    } else {
        fn gcd(a: i64, b: i64) -> i64 {
            if b == 0 { a } else { gcd(b, a % b) }
        }
        let g = gcd(w, h);
        if g > 0 && w / g < 100 && h / g < 100 {
            format!("{}:{}", w / g, h / g)
        } else {
            format!("{:.2}:1", ratio)
        }
    }
}
