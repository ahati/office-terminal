use std::path::PathBuf;
use std::process::ExitCode;
use kitty_pptx_core::{DefaultPresentationLoader, PresentationLoader, Source};

pub fn run_info(file: PathBuf, loader: &DefaultPresentationLoader) -> ExitCode {
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
