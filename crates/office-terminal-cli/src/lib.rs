//! Command-line entry points and configuration for office-terminal / ot.
#![forbid(unsafe_code)]
pub mod cli;
pub mod config;
pub mod commands;
pub mod runner;

pub use commands::source_from_path;
pub use runner::run;
