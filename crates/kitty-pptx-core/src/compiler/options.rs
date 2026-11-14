//! Per-slide compile options.
//!
//! The reference threads one `SlideRenderOptions` object through every stage;
//! this is the subset the display-list compiler reads. Every field is defaulted
//! so `compile` keeps its fixed four-argument contract and callers opt in only
//! where they need a policy.

use std::sync::Arc;

use crate::contracts::VectorCommand;

/// One chart-extension render pass. The host supplies the painter; the compiler
/// forwards the parsed chart and the shape rotation and splices the returned
/// commands into the paint order (ref renderer.ts:4418-4450
/// `chartEx.render(ctx, chart, rect, ptToPx, rotationDeg)`). `None` means the
/// extension declined, so the built-in chart painter runs.
pub type ChartExRendererFn =
    dyn Fn(&ooxml_common::chart::ChartModel, f32) -> Option<Vec<VectorCommand>> + Send + Sync;

/// One part that failed to parse (ref `PptxPartFailure`,
/// `packages/pptx/src/part-failures.ts:18-30`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartFailure {
    /// Failure family; `"slide"` for a slide part.
    pub kind: &'static str,
    /// 0-based presentation index.
    pub slide_index: usize,
    /// 1-based slide number, as shown in the built-in placeholder title.
    pub slide_number: usize,
    /// The parser's part-tagged diagnostic, e.g.
    /// `ppt/slides/slide3.xml: unexpected end of stream`.
    pub message: String,
}

/// Replacement text for the built-in "Slide N could not be displayed" card
/// (ref `ResolvedErrorPlaceholder`, `core/src/internal/part-failures.ts:33-37`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPlaceholder {
    pub title: String,
    /// `None` paints no detail line; `Some("")` behaves the same (the
    /// reference normalises the empty string to `null`,
    /// `resolvedFromWire`, `part-failures.ts:124`).
    pub detail: Option<String>,
}

/// In-realm resolver for a part failure. Returning `None` keeps the built-in
/// text, exactly like the reference's `() => undefined`.
pub type ErrorPlaceholderFn = dyn Fn(&PartFailure) -> Option<ErrorPlaceholder> + Send + Sync;

/// Compile-time policies for one slide.
#[derive(Default)]
pub struct CompileOptions {
    /// `errorPlaceholder` (ref renderer.ts:8142-8161). Consulted only for a
    /// slide carrying `parseError`; a healthy slide never calls it.
    pub error_placeholder: Option<Box<ErrorPlaceholderFn>>,
    /// Optional ChartEx painter (see [`ChartExRendererFn`]). `None` keeps the
    /// built-in chart pipeline.
    pub chartex: Option<Arc<ChartExRendererFn>>,
}

impl CompileOptions {
    /// Static text form (the reference's `{title, detail}` object).
    pub fn with_placeholder_text(title: impl Into<String>, detail: Option<String>) -> Self {
        let placeholder = ErrorPlaceholder {
            title: title.into(),
            detail,
        };
        Self {
            error_placeholder: Some(Box::new(move |_| Some(placeholder.clone()))),
            ..Self::default()
        }
    }

    /// Function form (the reference's `(failure) => text`).
    pub fn with_placeholder_fn(
        resolve: impl Fn(&PartFailure) -> Option<ErrorPlaceholder> + Send + Sync + 'static,
    ) -> Self {
        Self {
            error_placeholder: Some(Box::new(resolve)),
            ..Self::default()
        }
    }

    /// Resolve the placeholder for `failure`, normalising `Some("")` to `None`.
    pub fn resolve_placeholder(&self, failure: &PartFailure) -> Option<ErrorPlaceholder> {
        let mut resolved = self.error_placeholder.as_ref()?.as_ref()(failure)?;
        if resolved.detail.as_deref() == Some("") {
            resolved.detail = None;
        }
        Some(resolved)
    }
}

impl std::fmt::Debug for CompileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompileOptions")
            .field("error_placeholder", &self.error_placeholder.is_some())
            .field("chartex", &self.chartex.is_some())
            .finish()
    }
}
