use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "ot",
    bin_name = "ot",
    version,
    about = "Office Terminal (ot) / kitty-pptx-pdf-reader: High-fidelity terminal presentation & document reader"
)]
pub struct Cli {
    #[arg(help = "Path to presentation (.pptx) or document (.pdf), or '-' for stdin")]
    pub file: Option<PathBuf>,

    #[arg(short, long, help = "Start on specific slide number (1-based)")]
    pub slide: Option<usize>,

    #[arg(short, long, help = "Start in split-pane presenter mode")]
    pub presenter: bool,

    #[arg(
        long = "mode",
        value_enum,
        default_value = "auto",
        help = "Display mode for PDF/presentations: auto|slides|reader"
    )]
    pub mode: DeckModeArg,

    #[arg(
        long = "generate-completions",
        value_name = "SHELL",
        help = "Generate shell completions script directly to stdout (bash, zsh, fish, elvish, powershell)"
    )]
    pub generate_completions: Option<clap_complete::Shell>,

    #[arg(
        long = "metafile",
        value_enum,
        global = true,
        value_name = "MODE",
        help = "Metafile playback override (also KITTY_PPTX_METAFILE): playback|placeholder"
    )]
    pub metafile: Option<MetafileModeArg>,

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
        #[arg(
            long,
            value_enum,
            help = "Fit mode; defaults to display.fit in config.toml"
        )]
        fit: Option<FitMode>,
    },
    #[command(about = "Headless batch slide export to PNG or vector SVG")]
    Export {
        file: PathBuf,
        #[arg(short, long, help = "Output directory")]
        out: PathBuf,
        #[arg(
            short,
            long,
            value_enum,
            help = "Output format; defaults to export.default_format in config.toml"
        )]
        format: Option<ExportFormat>,
        #[arg(
            long,
            help = "Render DPI for PNG exports (ignored when --width is given); defaults to export.default_dpi"
        )]
        dpi: Option<u32>,
        #[arg(
            long,
            value_name = "PX",
            help = "Explicit PNG width in pixels; height follows the slide aspect. Takes precedence over --dpi"
        )]
        width: Option<u32>,
        #[arg(short, long, help = "Slide range (e.g. '1-3,5,10-')")]
        slides: Option<String>,
        #[arg(
            long,
            value_enum,
            value_name = "MODE",
            help = "SVG text mode (config: export.svg_text_mode)"
        )]
        svg_text_mode: Option<SvgTextModeArg>,
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
        #[arg(
            long,
            help = "Include base64 PNG payloads for embedded metafile bitmaps in --json output"
        )]
        embed_bitmaps: bool,
    },
    #[command(about = "Deck structure and diagnostic summary")]
    Info { file: PathBuf },
    #[command(about = "Generate shell completion scripts directly to stdout")]
    Completions {
        #[arg(
            value_name = "SHELL",
            help = "Target shell (bash, zsh, fish, elvish, powershell)"
        )]
        shell: clap_complete::Shell,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum MetafileModeArg {
    /// Play EMF/WMF/EMF+ through the engine (default).
    Playback,
    /// Replace every metafile picture with the diagnostic placeholder.
    Placeholder,
}

impl MetafileModeArg {
    pub fn to_mode(self) -> office_terminal_core::metafile::MetafileMode {
        match self {
            Self::Playback => office_terminal_core::metafile::MetafileMode::Playback,
            Self::Placeholder => office_terminal_core::metafile::MetafileMode::Placeholder,
        }
    }

    /// Parse `KITTY_PPTX_METAFILE` case-insensitively; unknown values are
    /// ignored rather than aborting playback (the flag wins when both are set).
    pub fn from_env(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "playback" => Some(Self::Playback),
            "placeholder" => Some(Self::Placeholder),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum SvgTextModeArg {
    /// Outline glyphs (smallest file, no selectable text).
    Paths,
    /// Real `<text>` elements with the shaped positions.
    Selectable,
    /// Same as Selectable, kept as the reference's spelling.
    Text,
}

impl SvgTextModeArg {
    pub fn to_mode(self) -> office_terminal_core::SvgTextMode {
        match self {
            Self::Paths => office_terminal_core::SvgTextMode::VectorGlyphs,
            Self::Selectable | Self::Text => office_terminal_core::SvgTextMode::SelectableText,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Default)]
pub enum FitMode {
    #[default]
    Contain,
    Width,
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
                return Err(format!(
                    "slide start {} out of bounds (1..={})",
                    start, total
                ));
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

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeckModeArg {
    #[default]
    Auto,
    Slides,
    Reader,
}
