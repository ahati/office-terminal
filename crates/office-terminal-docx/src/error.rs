//! Typed errors for the DOCX input engine (PRD 15 §8, issue #40 §7.14).

use office_terminal_core::ParseError;
use ooxml_common::resource::ResourceViolation;
#[cfg(feature = "rdocx-backend")]
use ooxml_common::resource::ResourceUsage;

/// Errors produced while opening, paginating or bridging a DOCX document.
#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    /// OPC package / WordprocessingML parse failure.
    #[error("DOCX package error: {0}")]
    Package(String),
    /// Pagination / layout failure after a parseable package.
    #[error("DOCX layout error: {0}")]
    Layout(String),
    /// Encrypted or password-protected package.
    #[error("encrypted/password-protected DOCX is not supported")]
    Encrypted,
    /// A governed resource ceiling crossing (package bytes, page count, …).
    /// Carried typed so a quota is never reported as a corrupt archive.
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(Box<ResourceViolation>),
    /// The third-party layout engine panicked on malformed input; the TUI
    /// must never crash with it (PRD 15 §8.2 panic boundary).
    #[error("DOCX layout engine panicked: {0}")]
    LayoutEnginePanic(String),
    /// A document feature this build deliberately does not support.
    #[error("unsupported document feature: {0}")]
    Unsupported(&'static str),
}

impl From<DocxError> for ParseError {
    fn from(error: DocxError) -> Self {
        match error {
            DocxError::Encrypted => ParseError::PasswordProtected,
            DocxError::ResourceLimit(violation) => ParseError::ResourceLimit(violation),
            DocxError::Package(message) => ParseError::ArchiveError(message),
            DocxError::Layout(message) => {
                ParseError::ArchiveError(format!("document layout failed: {message}"))
            }
            DocxError::LayoutEnginePanic(message) => ParseError::ArchiveError(message),
            DocxError::Unsupported(feature) => ParseError::ArchiveError(format!(
                "unsupported document feature: {feature}"
            )),
        }
    }
}

/// Build a typed page-count violation for the layout stage.
#[cfg(feature = "rdocx-backend")]
pub(crate) fn page_count_violation(observed: usize, limit: usize) -> DocxError {
    DocxError::ResourceLimit(Box::new(ResourceViolation {
        stage: "docx-layout",
        resource: "layout",
        metric: "pages",
        part: None,
        limit: limit as u64,
        observed: observed as u64,
        configurable: true,
        usage: ResourceUsage::default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_maps_to_the_password_error() {
        let mapped = ParseError::from(DocxError::Encrypted);
        assert!(matches!(mapped, ParseError::PasswordProtected));
    }

    #[cfg(feature = "rdocx-backend")]
    #[test]
    fn resource_limits_keep_their_identity_through_the_boundary() {
        let violation = page_count_violation(11_000, 10_000);
        let DocxError::ResourceLimit(inner) = violation else {
            panic!("expected a resource violation");
        };
        let mapped = ParseError::from(DocxError::ResourceLimit(inner));
        assert!(matches!(mapped, ParseError::ResourceLimit(_)));
    }

    #[test]
    fn layout_failures_keep_their_cause_in_the_payload() {
        let mapped = ParseError::from(DocxError::Layout("bad grid".into()));
        let message = mapped.to_string();
        assert!(message.contains("document layout failed"));
        assert!(message.contains("bad grid"));
    }
}
