use std::path::PathBuf;
use std::process::ExitCode;
use kitty_pptx_core::{DefaultPresentationLoader, PresentationLoader, Source};

pub fn run_dump(
    file: PathBuf,
    notes: bool,
    _comments: bool,
    _markdown: bool,
    _json: bool,
    loader: &DefaultPresentationLoader,
) -> ExitCode {
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
        println!(
            "Presentation has {} slides. Use --notes, --comments, --markdown, or --json.",
            presentation.slides.len()
        );
    }
    ExitCode::SUCCESS
}
