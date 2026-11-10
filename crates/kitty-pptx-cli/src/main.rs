use std::process::ExitCode;
use std::sync::Arc;
use clap::Parser;
use kitty_pptx_cli::cli::{Cli, Commands};
use kitty_pptx_cli::commands::*;
use kitty_pptx_cli::config::Config;
use kitty_pptx_core::{DefaultPresentationLoader, DefaultSlideCompiler, TinySkiaRasterizer, DefaultSlideSvgSerializer};

fn main() -> ExitCode {
    let args = Cli::parse();

    if let Some(shell) = args.generate_completions {
        return run_completions(shell);
    }

    let _config = Config::load();

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    match args.command {
        Some(Commands::Completions { shell }) => {
            run_completions(shell)
        }
        Some(Commands::Export { file, out, format, dpi, slides }) => {
            run_export(file, out, format, dpi, slides, &loader, &compiler, &rasterizer, &svg_serializer)
        }
        Some(Commands::Info { file }) => {
            run_info(file, &loader)
        }
        Some(Commands::Dump { file, notes, comments, markdown, json }) => {
            run_dump(file, notes, comments, markdown, json, &loader)
        }
        Some(Commands::View { file, slide, all }) => {
            run_view(file, slide, all, &loader, &compiler, &rasterizer)
        }
        None => {
            run_interactive(args.file, &loader, compiler, rasterizer)
        }
    }
}
