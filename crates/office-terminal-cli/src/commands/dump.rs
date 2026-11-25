use crate::commands::dump_ast;
use crate::commands::source_from_path;
use office_terminal_core::metafile::expand_metafiles;
use office_terminal_core::{
    DeckSource, DefaultPresentationLoader, DefaultSlideCompiler, PageSource, PresentationLoader, SlideCompiler,
};
use serde_json::Value;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpJsonEnvelope<'a> {
    schema_version: u32,
    warnings: Vec<String>,
    presentation: &'a pptx_model::Presentation,
    vector_slides: Vec<Value>,
}

pub fn run_dump(
    file: PathBuf,
    notes: bool,
    comments: bool,
    markdown: bool,
    json: bool,
    embed_bitmaps: bool,
    loader: &DefaultPresentationLoader,
) -> ExitCode {
    let source = source_from_path(file.clone());
    let deck = match loader.load_deck(source) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error loading deck: {}", e);
            return ExitCode::from(2);
        }
    };

    match deck {
        DeckSource::Pptx(presentation, asset_store) => dump_pptx(
            file,
            presentation,
            asset_store,
            notes,
            comments,
            markdown,
            json,
            embed_bitmaps,
        ),
        // Both document sources share the corpus-based paged dump; DOCX has
        // no speaker notes or rendered comments, exactly like PDF
        // (issue #40 §7.12).
        DeckSource::Pdf(page_source) | DeckSource::Docx(page_source) => {
            dump_paged(page_source, notes, comments, markdown, json)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dump_pptx(
    file: PathBuf,
    presentation: pptx_model::Presentation,
    asset_store: Arc<dyn office_terminal_core::AssetStore>,
    notes: bool,
    comments: bool,
    markdown: bool,
    json: bool,
    embed_bitmaps: bool,
) -> ExitCode {
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
        let compiler = DefaultSlideCompiler::new();
        let (compile_tx, compile_rx) = crossbeam_channel::unbounded();
        let mut warnings = Vec::new();
        let mut vector_slides = Vec::with_capacity(presentation.slides.len());
        for (index, slide) in presentation.slides.iter().enumerate() {
            let mut scene = match compiler.compile_in_deck(
                slide,
                &presentation.slides,
                presentation.slide_width,
                presentation.slide_height,
                &compile_tx,
            ) {
                Ok(scene) => scene,
                Err(error) => {
                    warnings.push(format!("slide {}: {error}", index + 1));
                    continue;
                }
            };
            for warning in compile_rx.try_iter() {
                warnings.push(format!("slide {}: {warning:?}", index + 1));
            }
            let (warning_tx, warning_rx) = crossbeam_channel::unbounded();
            expand_metafiles(&mut scene, asset_store.as_ref(), &warning_tx);
            for warning in warning_rx.try_iter() {
                warnings.push(format!("slide {}: {warning:?}", index + 1));
            }
            vector_slides.push(serde_json::json!({
                "slideNumber": index + 1,
                "scene": dump_ast::slide_json(&scene, embed_bitmaps),
            }));
        }
        let envelope = DumpJsonEnvelope {
            schema_version: 1,
            warnings,
            presentation: &presentation,
            vector_slides,
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

fn dump_paged(page_source: Arc<dyn PageSource>, notes: bool, comments: bool, markdown: bool, json: bool) -> ExitCode {
    let any_flag = notes || comments || markdown || json;
    let total = page_source.len();
    if !any_flag {
        println!(
            "Document has {} pages. Use --notes, --comments, --markdown, or --json.",
            total
        );
        return ExitCode::SUCCESS;
    }

    if notes {
        for i in 0..total {
            println!("--- Page {} Notes ---", i + 1);
            println!("(No notes)");
        }
    }

    if comments {
        for i in 0..total {
            println!("--- Page {} Comments ---", i + 1);
            println!("(No comments)");
        }
    }

    if markdown {
        let corpus = page_source.search_corpus();
        for i in 0..total {
            println!("# Page {}\n", i + 1);
            for line in corpus.page_text(i) {
                println!("{}", line.text);
            }
            println!();
        }
    }

    if json {
        let corpus = page_source.search_corpus();
        let mut pages = Vec::with_capacity(total);
        for i in 0..total {
            let lines: Vec<String> = corpus.page_text(i).iter().map(|l| l.text.clone()).collect();
            pages.push(serde_json::json!({
                "page": i + 1,
                "text": lines.join("\n"),
            }));
        }
        let env = serde_json::json!({
            "schemaVersion": 1,
            "pageCount": total,
            "pages": pages,
        });
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    }

    ExitCode::SUCCESS
}
