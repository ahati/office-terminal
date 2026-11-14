//! Typed parser errors for native callers.
//!
//! The reference propagates an `OoxmlResourceLimitError` object through every
//! layer, so a caller can tell a quota crossing from a malformed part by
//! identity (`embedded-fonts.test.ts:160-165`, `embedded-fonts-load.test.ts`
//! `4096`/`4097`). The wasm entry points flatten errors to their wire string;
//! this enum is the native form that keeps the violation typed.

use ooxml_common::resource::ResourceViolation;

/// One parser failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PptxError {
    /// A governed resource ceiling was crossed. The payload is the same
    /// `{stage, resource, metric, part, limit, observed, configurable, usage}`
    /// object the wire envelope carries
    /// (ref `core/src/errors/ooxml-error.ts` `OoxmlResourceLimitError`).
    Resource(Box<ResourceViolation>),
    /// Any other archive/part diagnostic, carried verbatim.
    Format(String),
}

impl std::fmt::Display for PptxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(violation) => write!(f, "resource limit exceeded: {violation}"),
            Self::Format(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for PptxError {}

impl From<PptxError> for wasm_bindgen::JsValue {
    /// Wire form: strings only, as the worker protocol expects
    /// (`serializeWorkerError` in the reference).
    fn from(error: PptxError) -> Self {
        wasm_bindgen::JsValue::from_str(&error.to_string())
    }
}
