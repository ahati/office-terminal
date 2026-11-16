use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum PdfError {
    #[error("Malformed PDF document at page {page}: {detail}")]
    Malformed { page: usize, detail: String },

    #[error("Encrypted or password-protected PDF files are not supported")]
    EncryptedUnsupported,

    #[error("Resource limit exceeded on page {page}: {limit}")]
    BudgetExceeded { page: usize, limit: &'static str },

    #[error("PDF rendering error on page {page}: {detail}")]
    RenderFailed { page: usize, detail: String },

    #[error("Empty PDF document contains 0 pages")]
    EmptyDocument,

    #[error("Page index {index} out of bounds (total pages: {total})")]
    IndexOutOfBounds { index: usize, total: usize },

    #[error("PDF I/O error: {0}")]
    Io(String),
}
