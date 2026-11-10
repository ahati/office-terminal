use clap::Parser;
use kitty_pptx_cli::cli::{Cli, Commands, ExportFormat, parse_slide_range};

#[test]
fn test_cli_parse_export_subcommand() {
    let args = Cli::try_parse_from([
        "kitty-pptx",
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
        Some(Commands::Export { file, out, format, dpi, slides }) => {
            assert_eq!(file.to_str().unwrap(), "test.pptx");
            assert_eq!(out.to_str().unwrap(), "/tmp/export_dir");
            assert_eq!(format, ExportFormat::Svg);
            assert_eq!(dpi, 300);
            assert_eq!(slides.as_deref(), Some("1-3,5"));
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
