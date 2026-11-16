//! DrawingML text layout engine (parity port).
//!
//! Pure, device-independent layout: line breaking, justification, tab stops and
//! PowerPoint line metrics. Ported 1:1 from the reference renderer, see
//! `packages/pptx/src/powerpoint-line-metrics.ts` and
//! `packages/core/src/drawingml-text/`.

pub mod bidi;
pub mod columns;
pub mod east_asian_default;
mod east_asian_coverage;
pub mod bullets;
pub mod emit;
mod face_cache;
pub mod faces;
pub mod face_features;
pub mod find;
pub mod fonts;
pub mod linebreak;
pub mod justify;
pub mod measure;
pub mod power_point;
pub(crate) mod run_decorations;
pub mod shape;
pub mod metrics;
mod natural_box;
mod preflight;
mod decor;
mod paint;
mod paragraphs;
mod slots;
mod stacked;
pub mod tabs;
pub mod vertical;

pub use measure::{StyleId, TabAlign, TabStop, TextMeasure};
