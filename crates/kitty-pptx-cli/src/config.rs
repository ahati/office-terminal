use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub export: ExportConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub default_protocol: String,
    pub fit: String,
    pub payload: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            default_protocol: "auto".to_string(),
            fit: "contain".to_string(),
            payload: "png".to_string(),
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

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if let Some(mut path) = dirs_next_config_dir() {
            path.push("kitty-pptx");
            path.push("config.toml");
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
