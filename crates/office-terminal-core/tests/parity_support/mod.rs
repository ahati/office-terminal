//! Shared harness for reference-parity tests.
//!
//! Mirrors `/tmp/oooxml/packages/pptx/src/test-support/` and the Playwright VRT
//! helpers: a synthetic deck builder, a pixelmatch port, per-slide score
//! storage and the VRT driver.
#![allow(dead_code)]

pub mod deck;
pub mod pixelmatch;
pub mod scores;
pub mod vrt;
