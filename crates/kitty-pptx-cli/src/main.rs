#![forbid(unsafe_code)]
use clap::Parser;
use kitty_pptx_cli::cli::{Cli, Commands};
use kitty_pptx_cli::commands::*;
use kitty_pptx_cli::commands::export::run_export_with_options;
use kitty_pptx_cli::config::Config;
use kitty_pptx_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer, TinySkiaRasterizer,
};
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let args = Cli::parse();

    if let Some(shell) = args.generate_completions {
        return run_completions(shell);
    }

    let config = Config::load();

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    match args.command {
        Some(Commands::Completions { shell }) => run_completions(shell),
        Some(Commands::Export {
            file,
            out,
            format,
            dpi,
            width,
            slides,
            svg_text_mode,
        }) => {
            let format = format.unwrap_or_else(|| config.resolved_format());
            let dpi = dpi.unwrap_or_else(|| config.resolved_dpi());
            let svg_text_mode = svg_text_mode
                .map(|mode| mode.to_mode())
                .unwrap_or_else(|| config.resolved_svg_text_mode());
            run_export_with_options(
                file,
                out,
                format,
                dpi,
                width,
                slides,
                svg_text_mode,
                &loader,
                &compiler,
                &rasterizer,
                &svg_serializer,
            )
        }
        Some(Commands::Info { file }) => run_info(file, &loader),
        Some(Commands::Dump {
            file,
            notes,
            comments,
            markdown,
            json,
        }) => run_dump(file, notes, comments, markdown, json, &loader),
        Some(Commands::View {
            file,
            slide,
            all,
            fit,
        }) => {
            let fit = fit.unwrap_or_else(|| config.resolved_fit());
            run_view(file, slide, all, fit, &loader, &compiler, &rasterizer)
        }
        None => run_interactive(
            args.file,
            args.slide,
            args.presenter,
            &loader,
            compiler,
            rasterizer,
        ),
    }
}
