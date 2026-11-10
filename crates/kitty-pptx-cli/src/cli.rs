use std::path::PathBuf;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "kitty-pptx", version, about = "High-fidelity terminal PowerPoint presenter & exporter")]
pub struct Cli {
    #[arg(help = "Path to .pptx deck, or '-' for stdin")]
    pub file: Option<PathBuf>,

    #[arg(short, long, help = "Start on specific slide number (1-based)")]
    pub slide: Option<usize>,

    #[arg(short, long, help = "Start in split-pane presenter mode")]
    pub presenter: bool,

    #[arg(long, help = "Force specific graphics protocol")]
    pub protocol: Option<String>,

    #[arg(long = "generate-completions", value_name = "SHELL", help = "Generate shell completions script directly to stdout (bash, zsh, fish, elvish, powershell)")]
    pub generate_completions: Option<clap_complete::Shell>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "One-shot inline terminal slide preview")]
    View {
        file: PathBuf,
        #[arg(short = 'n', long = "slide", default_value = "1")]
        slide: usize,
        #[arg(short, long)]
        all: bool,
        #[arg(long, value_enum, default_value = "contain")]
        fit: FitMode,
        #[arg(long, value_enum, default_value = "png")]
        payload: PayloadMode,
    },
    #[command(about = "Headless batch slide export to PNG or vector SVG")]
    Export {
        file: PathBuf,
        #[arg(short, long, help = "Output directory")]
        out: PathBuf,
        #[arg(short, long, value_enum, default_value = "png")]
        format: ExportFormat,
        #[arg(long, default_value = "150")]
        dpi: u32,
        #[arg(short, long, help = "Slide range (e.g. '1-3,5,10-')")]
        slides: Option<String>,
    },
    #[command(about = "Extract text, notes, comments, or AST")]
    Dump {
        file: PathBuf,
        #[arg(long)]
        notes: bool,
        #[arg(long)]
        comments: bool,
        #[arg(long)]
        markdown: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Deck structure and diagnostic summary")]
    Info {
        file: PathBuf,
    },
    #[command(about = "Generate shell completion scripts directly to stdout")]
    Completions {
        #[arg(value_name = "SHELL", help = "Target shell (bash, zsh, fish, elvish, powershell)")]
        shell: clap_complete::Shell,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Default)]
pub enum FitMode {
    #[default]
    Contain,
    Width,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Default)]
pub enum PayloadMode {
    #[default]
    Png,
    Raw,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum ExportFormat {
    Png,
    Svg,
    All,
}

pub fn parse_slide_range(input: &str, total: usize) -> Result<Vec<usize>, String> {
    if total == 0 {
        return Err("presentation has no slides".to_string());
    }
    let mut indices = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((start_s, end_s)) = trimmed.split_once('-') {
            let start = if start_s.is_empty() {
                1
            } else {
                start_s
                    .parse::<usize>()
                    .map_err(|_| format!("invalid start number: {}", start_s))?
            };
            let end = if end_s.is_empty() {
                total
            } else {
                end_s
                    .parse::<usize>()
                    .map_err(|_| format!("invalid end number: {}", end_s))?
            };
            if start == 0 || start > total {
                return Err(format!("slide start {} out of bounds (1..={})", start, total));
            }
            if end == 0 || end > total {
                return Err(format!("slide end {} out of bounds (1..={})", end, total));
            }
            if start > end {
                return Err(format!("invalid slide range: {} > {}", start, end));
            }
            for i in start..=end {
                indices.push(i - 1);
            }
        } else {
            let num = trimmed
                .parse::<usize>()
                .map_err(|_| format!("invalid slide number: {}", trimmed))?;
            if num == 0 || num > total {
                return Err(format!("slide index {} out of bounds (1..={})", num, total));
            }
            indices.push(num - 1);
        }
    }
    if indices.is_empty() {
        return Err("slide range resulted in empty selection".to_string());
    }
    indices.sort_unstable();
    indices.dedup();
    Ok(indices)
}
