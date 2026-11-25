//! DrawingML text layout engine (parity port).
//!
//! Pure, device-independent layout: line breaking, justification, tab stops and
//! PowerPoint line metrics. Ported 1:1 from the reference renderer, see
//! `packages/pptx/src/powerpoint-line-metrics.ts` and
//! `packages/core/src/drawingml-text/`.

pub mod bidi;
mod body_layout;
pub mod bullets;
pub mod cmap;
pub mod columns;
mod decor;
pub mod document_faces;
mod east_asian_coverage;
pub mod east_asian_default;
pub mod emit;
mod face_cache;
pub mod face_features;
pub mod faces;
pub mod find;
pub mod fonts;
pub mod justify;
pub mod linebreak;
pub mod measure;
pub mod metrics;
mod natural_box;
mod paint;
mod paragraphs;
pub mod power_point;
mod preflight;
pub(crate) mod run_decorations;
pub mod shape;
mod shrink;
mod slots;
mod stacked;
pub mod tabs;
pub mod vertical;

pub use measure::{StyleId, TabAlign, TabStop, TextMeasure};
