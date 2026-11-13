//! DrawingML text layout engine (parity port).
//!
//! Pure, device-independent layout: line breaking, justification, tab stops and
//! PowerPoint line metrics. Ported 1:1 from the reference renderer, see
//! `packages/pptx/src/powerpoint-line-metrics.ts` and
//! `packages/core/src/drawingml-text/`.

pub mod emit;
pub mod fonts;
pub mod linebreak;
pub mod justify;
pub mod measure;
pub mod shape;
pub mod metrics;
mod paint;
pub mod tabs;
pub mod vertical;

pub use measure::{StyleId, TabAlign, TabStop, TextMeasure};
