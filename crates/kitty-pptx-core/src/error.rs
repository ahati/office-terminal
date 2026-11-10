use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum KittyPptxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("Asset error: {0}")]
    Asset(#[from] AssetError),
    #[error("Render error: {0}")]
    Render(#[from] RenderError),
    #[error("Display error: {0}")]
    Display(#[from] DisplayError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("Unreadable config file: {0}")]
    UnreadableConfig(String),
    #[error("Invalid slide range: {0}")]
    InvalidSlideRange(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Corrupted PPTX archive: {0}")]
    ArchiveError(String),
    #[error("Malformed XML part '{part}': {details}")]
    CorruptPart { part: String, details: String },
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitHit(&'static str),
    #[error("Unsupported encrypted/password-protected file")]
    PasswordProtected,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("Asset not found in archive: {0}")]
    NotFound(String),
    #[error("Unsupported media format: {0}")]
    UnsupportedFormat(String),
    #[error("Image decoding failed: {0}")]
    DecodeFailed(String),
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum RenderError {
    #[error("Slide layout compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Rasterization error: {0}")]
    RasterizationError(String),
    #[error("Vector serialization error: {0}")]
    SerializationError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("Image {0:?} was not staged prior to presentation")]
    NotStaged(u32),
    #[error("Terminal I/O error: {0}")]
    Io(std::io::Error),
    #[error("No graphics-capable terminal found")]
    NoCapableTerminal,
    #[error("Graphics command requires a valid TTY")]
    TtyRequired,
    #[error("Terminal image memory quota exceeded")]
    QuotaExceeded,
}
