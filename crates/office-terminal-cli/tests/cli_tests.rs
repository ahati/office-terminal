use clap::Parser;
use office_terminal_cli::cli::{parse_slide_range, Cli, Commands, ExportFormat};
use office_terminal_cli::config::{parse_svg_text_mode, Config};
use office_terminal_core::SvgTextMode;

#[test]
fn test_cli_parse_export_subcommand() {
    let args = Cli::try_parse_from([
        "ot",
        "export",
        "test.pptx",
        "--out",
        "/tmp/export_dir",
        "--format",
        "svg",
        "--dpi",
        "300",
        "--slides",
        "1-3,5",
    ])
    .expect("parsing CLI failed");

    match args.command {
        Some(Commands::Export {
            file,
            out,
            format,
            dpi,
            width,
            slides,
            svg_text_mode,
        }) => {
            assert_eq!(file.to_str().unwrap(), "test.pptx");
            assert_eq!(out.to_str().unwrap(), "/tmp/export_dir");
            assert_eq!(format, Some(ExportFormat::Svg));
            assert_eq!(dpi, Some(300));
            assert_eq!(width, None);
            assert_eq!(slides.as_deref(), Some("1-3,5"));
            assert_eq!(svg_text_mode, None);
        }
        _ => panic!("Expected Export subcommand"),
    }
}

#[test]
fn test_slide_range_parsing() {
    let indices = parse_slide_range("1-3,5", 10).expect("failed parsing range");
    assert_eq!(indices, vec![0, 1, 2, 4]);

    let open_ended = parse_slide_range("8-", 10).expect("failed open range");
    assert_eq!(open_ended, vec![7, 8, 9]);
}

#[test]
fn test_export_flags_are_optional_for_config_fallbacks() {
    let args = Cli::try_parse_from(["ot", "export", "test.pptx", "--out", "/tmp/x"]).expect("parsing CLI failed");
    match args.command {
        Some(Commands::Export {
            format,
            dpi,
            svg_text_mode,
            ..
        }) => {
            assert_eq!(format, None);
            assert_eq!(dpi, None);
            assert_eq!(svg_text_mode, None);
        }
        _ => panic!("Expected Export subcommand"),
    }
}

#[test]
fn test_config_resolvers_use_documented_defaults() {
    let config = Config::default();
    assert_eq!(config.resolved_format(), ExportFormat::Png);
    assert_eq!(config.resolved_dpi(), 150);
    assert_eq!(config.resolved_svg_text_mode(), SvgTextMode::VectorGlyphs);
    assert_eq!(config.resolved_fit(), office_terminal_cli::cli::FitMode::Contain);

    assert_eq!(parse_svg_text_mode("selectable"), SvgTextMode::SelectableText);
    assert_eq!(parse_svg_text_mode("text"), SvgTextMode::SelectableText);
    assert_eq!(parse_svg_text_mode("paths"), SvgTextMode::VectorGlyphs);
}

#[test]
fn test_cli_mode_flags() {
    let args_default = Cli::try_parse_from(["ot", "test.pdf"]).expect("parse failed");
    assert_eq!(args_default.mode, office_terminal_cli::cli::DeckModeArg::Auto);

    let args_reader = Cli::try_parse_from(["ot", "test.pdf", "--mode", "reader"]).expect("parse failed");
    assert_eq!(args_reader.mode, office_terminal_cli::cli::DeckModeArg::Reader);

    let args_slides = Cli::try_parse_from(["ot", "test.pdf", "--mode", "slides"]).expect("parse failed");
    assert_eq!(args_slides.mode, office_terminal_cli::cli::DeckModeArg::Slides);
}
