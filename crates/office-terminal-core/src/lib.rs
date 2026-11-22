//! kitty-pptx core: vector display list, compiler, rasterizer and SVG exporter.
#![forbid(unsafe_code)]
pub mod contracts;
pub mod error;
pub mod loader;
pub mod compiler;
pub mod metafile;
pub mod rasterizer;
pub mod svg;
pub mod navigation;
pub mod pipeline;
pub mod scene3d;
pub mod selection;
pub mod text;

#[allow(ambiguous_glob_reexports)] // `shape_effects` exists in both contracts and compiler
pub use contracts::*;
pub use error::*;
pub use loader::*;
pub use compiler::*;
pub use rasterizer::*;
pub use svg::*;
pub use navigation::*;
pub use selection::*;
pub use pipeline::*;
