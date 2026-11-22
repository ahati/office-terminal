//! Engine diagnostics mapped onto the project's fidelity classes.

use crate::contracts::FidelityClass;

/// One engine diagnostic, deduplicated per `(element, tag)` before emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetafileDiagnostic {
    /// Stable tag (`metafile_hatch`, `metafile_rop`, …) reused as the
    /// `WarningEvent::FidelityDegraded::feature` value.
    pub tag: &'static str,
    pub class: FidelityClass,
    /// Human-readable detail for the warnings drawer.
    pub detail: String,
    /// When set, the diagnostic is surfaced as `WarningEvent::FontSubstituted`
    /// (`(requested, substitute)`) instead of a generic fidelity warning.
    pub font_substitution: Option<(String, String)>,
}

impl MetafileDiagnostic {
    pub fn new(tag: &'static str, class: FidelityClass, detail: impl Into<String>) -> Self {
        Self {
            tag,
            class,
            detail: detail.into(),
            font_substitution: None,
        }
    }

    /// A face the bundled/system font stack does not provide.
    pub fn font_substituted(requested: &str, substitute: &str) -> Self {
        Self {
            tag: "metafile_font_substituted",
            class: FidelityClass::Degraded,
            detail: format!("font '{requested}' substituted with '{substitute}'"),
            font_substitution: Some((requested.to_string(), substitute.to_string())),
        }
    }

    /// A visible approximation (rendered, but not faithfully).
    pub fn degraded(tag: &'static str, detail: impl Into<String>) -> Self {
        Self::new(tag, FidelityClass::Degraded, detail)
    }

    /// A placeholder-class failure.
    pub fn placeholder(tag: &'static str, detail: impl Into<String>) -> Self {
        Self::new(tag, FidelityClass::Placeholder, detail)
    }
}

/// Drops repeated tags, keeping the first occurrence.
pub fn dedup(diagnostics: Vec<MetafileDiagnostic>) -> Vec<MetafileDiagnostic> {
    let mut seen: Vec<&'static str> = Vec::with_capacity(diagnostics.len());
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            if seen.contains(&diagnostic.tag) {
                false
            } else {
                seen.push(diagnostic.tag);
                true
            }
        })
        .collect()
}
