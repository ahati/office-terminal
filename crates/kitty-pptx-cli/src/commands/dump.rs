use std::path::PathBuf;
use std::process::ExitCode;
use kitty_pptx_core::{DefaultPresentationLoader, PresentationLoader};
use crate::commands::source_from_path;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpJsonEnvelope<'a> {
    schema_version: u32,
    warnings: Vec<String>,
    presentation: &'a pptx_model::Presentation,
}

pub fn run_dump(
    file: PathBuf,
    notes: bool,
    comments: bool,
    markdown: bool,
    json: bool,
    loader: &DefaultPresentationLoader,
) -> ExitCode {
    let source = source_from_path(file.clone());
    let (presentation, asset_store) = match loader.load(source) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading deck: {}", e);
            return ExitCode::from(2);
        }
    };

    let any_flag = notes || comments || markdown || json;
    if !any_flag {
        println!(
            "Presentation has {} slides. Use --notes, --comments, --markdown, or --json.",
            presentation.slides.len()
        );
        return ExitCode::SUCCESS;
    }

    if notes {
        for (i, s) in presentation.slides.iter().enumerate() {
            println!("--- Slide {} Notes ---", i + 1);
            if let Some(ref n) = s.notes {
                if n.trim().is_empty() {
                    println!("(No notes)");
                } else {
                    println!("{}", n);
                }
            } else {
                println!("(No notes)");
            }
        }
    }

    if comments {
        for (i, s) in presentation.slides.iter().enumerate() {
            println!("--- Slide {} Comments ---", i + 1);
            if s.comments.is_empty() {
                println!("(No comments)");
            } else {
                for c in &s.comments {
                    let author = c.author.as_deref().unwrap_or("Unknown");
                    let date = c.date.as_deref().unwrap_or("");
                    if date.is_empty() {
                        println!("[{}] {}", author, c.text);
                    } else {
                        println!("[{} ({})] {}", author, date, c.text);
                    }
                    for r in &c.replies {
                        let r_author = r.author.as_deref().unwrap_or("Unknown");
                        let r_date = r.date.as_deref().unwrap_or("");
                        if r_date.is_empty() {
                            println!("  > [{}] {}", r_author, r.text);
                        } else {
                            println!("  > [{} ({})] {}", r_author, r_date, r.text);
                        }
                    }
                }
            }
        }
    }

    if markdown {
        let bytes_opt = asset_store
            .archive_bytes()
            .map(|b| b.to_vec())
            .or_else(|| asset_store.get_raw_bytes("").ok())
            .or_else(|| {
                if file.as_os_str() != "-" {
                    std::fs::read(&file).ok()
                } else {
                    None
                }
            });

        match bytes_opt {
            Some(bytes) => match pptx_parser::to_markdown_native(&bytes) {
                Ok(md) => {
                    print!("{}", md);
                }
                Err(e) => {
                    eprintln!("Markdown extraction error: {}", e);
                    return ExitCode::from(2);
                }
            },
            None => {
                eprintln!("Failed to retrieve deck archive bytes for markdown extraction");
                return ExitCode::from(2);
            }
        }
    }

    if json {
        let envelope = DumpJsonEnvelope {
            schema_version: 1,
            warnings: Vec::new(),
            presentation: &presentation,
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(json_str) => {
                println!("{}", json_str);
            }
            Err(e) => {
                eprintln!("JSON serialization error: {}", e);
                return ExitCode::from(2);
            }
        }
    }

    ExitCode::SUCCESS
}
