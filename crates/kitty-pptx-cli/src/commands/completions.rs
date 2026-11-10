use std::io;
use std::process::ExitCode;
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use crate::cli::Cli;

/// Generates shell completion script for the requested shell and writes to stdout.
pub fn run_completions(shell: Shell) -> ExitCode {
    generate_completions_to(shell, &mut io::stdout());
    ExitCode::SUCCESS
}

/// Helper function to generate shell completion scripts to an arbitrary writer (used in tests).
pub fn generate_completions_to<W: io::Write>(shell: Shell, out: &mut W) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "kitty-pptx", out);
}
