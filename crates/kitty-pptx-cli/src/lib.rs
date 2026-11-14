//! Command-line entry points and configuration.
#![forbid(unsafe_code)]
pub mod cli;
pub mod config;
pub mod commands;

pub use commands::source_from_path;
