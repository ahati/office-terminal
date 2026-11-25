//! Office Terminal core: vector display list, compiler, rasterizer and SVG exporter.
#![forbid(unsafe_code)]
// `shape_effects` exists in both `contracts` and `compiler`; the glob re-exports
// intentionally expose one name and are part of the crate's public surface.
#![allow(ambiguous_glob_reexports)]
pub mod compiler;
pub mod contracts;
pub mod error;
pub mod loader;
pub mod metafile;
pub mod navigation;
pub mod pipeline;
pub mod rasterizer;
pub mod scene3d;
pub mod selection;
pub mod svg;
pub mod text;

pub use compiler::*;
pub use contracts::*;
pub use error::*;
pub use loader::*;
pub use navigation::*;
pub use pipeline::*;
pub use rasterizer::*;
pub use selection::*;
pub use svg::*;
