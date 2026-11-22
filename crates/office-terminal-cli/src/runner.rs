#![forbid(unsafe_code)]
use clap::Parser;
use crate::cli::{Cli, Commands, MetafileModeArg};
use crate::commands::export::run_export_with_options;
use crate::commands::*;
use crate::config::Config;
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer, TinySkiaRasterizer,
};
use std::process::ExitCode;
use std::sync::Arc;

pub fn run() -> ExitCode {
    office_terminal_pdf::register();
    let args = Cli::parse();

    if let Some(shell) = args.generate_completions {
        return run_completions(shell);
    }

    let metafile_mode = args
        .metafile
        .or_else(|| {
            std::env::var("OT_METAFILE")
                .or_else(|_| std::env::var("OFFICE_TERMINAL_METAFILE"))
                .or_else(|_| std::env::var("KITTY_PPTX_METAFILE"))
                .ok()
                .and_then(|value| MetafileModeArg::from_env(&value))
        })
        .unwrap_or(MetafileModeArg::Playback);
    office_terminal_core::metafile::set_playback_mode(metafile_mode.to_mode());

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
            embed_bitmaps,
        }) => run_dump(
            file,
            notes,
            comments,
            markdown,
            json,
            embed_bitmaps,
            &loader,
        ),
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
            args.mode,
            &loader,
            compiler,
            rasterizer,
        ),
    }
}
