use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use office_terminal_core::SvgTextMode;

use crate::cli::{ExportFormat, FitMode};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub export: ExportConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub fit: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            fit: "contain".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportConfig {
    pub default_format: String,
    pub default_dpi: u32,
    pub svg_text_mode: String,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            default_format: "png".to_string(),
            default_dpi: 150,
            svg_text_mode: "paths".to_string(),
        }
    }
}

impl Config {
    /// `export.format`, falling back to PNG for an unknown value.
    pub fn resolved_format(&self) -> ExportFormat {
        match self.export.default_format.trim().to_ascii_lowercase().as_str() {
            "svg" => ExportFormat::Svg,
            "all" => ExportFormat::All,
            _ => ExportFormat::Png,
        }
    }

    /// `export.default_dpi`, clamped to a renderable minimum.
    pub fn resolved_dpi(&self) -> u32 {
        self.export.default_dpi.max(1)
    }

    /// `export.svg_text_mode`: `paths` (default), `selectable`, or `text`.
    pub fn resolved_svg_text_mode(&self) -> SvgTextMode {
        parse_svg_text_mode(&self.export.svg_text_mode)
    }

    /// `display.fit`: `contain` (default) or `width`.
    pub fn resolved_fit(&self) -> FitMode {
        match self.display.fit.trim().to_ascii_lowercase().as_str() {
            "width" | "fillwidth" => FitMode::Width,
            _ => FitMode::Contain,
        }
    }

    pub fn load() -> Self {
        if let Some(base) = dirs_next_config_dir() {
            for dir_name in ["ot", "office-terminal", "kitty-pptx"] {
                let path = base.join(dir_name).join("config.toml");
                if path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(cfg) = toml::from_str(&content) {
                            return cfg;
                        }
                    }
                }
            }
        }
        for file_name in ["ot.toml", "office-terminal.toml", "kitty-pptx.toml"] {
            let path = PathBuf::from(file_name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(cfg) = toml::from_str(&content) {
                        return cfg;
                    }
                }
            }
        }
        Config::default()
    }
}

fn dirs_next_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        Some(PathBuf::from(xdg))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(PathBuf::from(home).join(".config"))
    } else {
        None
    }
}

/// Parse the CLI/config SVG text-mode spelling.
pub fn parse_svg_text_mode(value: &str) -> SvgTextMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "selectable" | "selectabletext" | "text" => SvgTextMode::SelectableText,
        "texttags" => SvgTextMode::TextTags,
        _ => SvgTextMode::VectorGlyphs,
    }
}
