//! DOCX CLI integration tests (PRD 15 §7 command matrix): loader detection,
//! `info`, `dump`, `export` PNG/SVG, and typed rejections for non-document
//! inputs.

// The package fixtures are shared with the engine crate's integration
// tests rather than copied, so a fixture fix lands in both binaries.
#[path = "../../office-terminal-docx/tests/common/mod.rs"]
mod docx_common;
use docx_common as common;

use office_terminal_cli::cli::ExportFormat;
use office_terminal_cli::commands::export::run_export;
use office_terminal_cli::commands::{run_dump, run_info};
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer,
    PresentationLoader, TinySkiaRasterizer,
};
use std::process::ExitCode;
use std::sync::Arc;

/// Loading a DOCX resolves to `DeckSource::Docx`; a spreadsheet package is a
/// typed rejection, and content wins over the file extension.
#[test]
fn loader_detects_docx_by_package_content() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;

    let bytes = common::simple_docx();
    let deck = loader
        .load_deck(office_terminal_core::Source::Bytes(bytes))
        .expect("docx loads");
    assert!(matches!(deck, office_terminal_core::DeckSource::Docx(_)));
}

#[test]
fn loader_rejects_spreadsheets_with_a_typed_error() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;
    let xlsx = common::build_docx(&[
        ("[Content_Types].xml", "placeholder"),
        ("xl/workbook.xml", "<workbook/>"),
    ]);
    let error = loader
        .load_deck(office_terminal_core::Source::Bytes(xlsx))
        .err()
        .expect("spreadsheet rejected");
    assert!(error.to_string().to_lowercase().contains("spreadsheet"));
}

#[test]
fn loader_rejects_garbage_with_a_typed_error() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;
    let error = loader
        .load_deck(office_terminal_core::Source::Bytes(common::garbage_bytes()))
        .err()
        .expect("garbage rejected");
    assert!(!error.to_string().is_empty());
}

#[test]
fn docx_info_reports_pages_dimensions_and_toc() {
    office_terminal_docx::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("sample.docx");
    std::fs::write(&path, common::simple_docx()).unwrap();

    let loader = DefaultPresentationLoader;
    assert_eq!(run_info(path, &loader), ExitCode::SUCCESS);
}

/// `ot info` only ever sees `DeckSource::Docx(Arc<dyn PageSource>)`; the
/// declared core properties must be reachable through that erased handle.
#[test]
fn docx_metadata_is_reachable_through_the_type_erased_deck_source() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;
    let deck = loader
        .load_deck(office_terminal_core::Source::Bytes(common::simple_docx()))
        .expect("docx loads");
    let office_terminal_core::DeckSource::Docx(source) = deck else {
        panic!("expected a DOCX deck");
    };
    let meta = source.metadata().expect("core properties are declared");
    assert_eq!(meta.title.as_deref(), Some("Integration Report"));
    assert_eq!(meta.author.as_deref(), Some("A. Writer"));
}

#[test]
fn docx_dump_markdown_json_and_notes() {
    office_terminal_docx::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("sample.docx");
    std::fs::write(&path, common::simple_docx()).unwrap();
    let loader = DefaultPresentationLoader;

    assert_eq!(
        run_dump(path.clone(), true, false, false, false, false, &loader),
        ExitCode::SUCCESS,
        "--notes prints the per-page empty placeholder"
    );
    assert_eq!(
        run_dump(path.clone(), false, false, true, false, false, &loader),
        ExitCode::SUCCESS,
        "--markdown prints reading-order text"
    );
    assert_eq!(
        run_dump(path, false, false, false, true, false, &loader),
        ExitCode::SUCCESS,
        "--json prints the paged corpus envelope"
    );
}

/// Export parity on the richer fixtures: tables, headers/footers with PAGE
/// fields, inline pictures and OMML must all pass through PNG and SVG
/// without a failed page.
#[test]
fn docx_export_handles_tables_headers_images_and_math() {
    office_terminal_docx::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    for (name, bytes) in [
        ("table", common::table_docx()),
        ("header_footer", common::header_footer_docx()),
        ("image", common::image_docx()),
        ("math", common::math_docx()),
    ] {
        let path = temp_dir.path().join(format!("{name}.docx"));
        std::fs::write(&path, bytes).unwrap();
        let out_dir = temp_dir.path().join(format!("out-{name}"));
        let code = run_export(
            path,
            out_dir.clone(),
            ExportFormat::All,
            96,
            None,
            &loader,
            &compiler,
            &rasterizer,
            &svg_serializer,
        );
        assert_eq!(code, ExitCode::SUCCESS, "{name} exports");
        let png = std::fs::read(out_dir.join("slide_001.png")).expect("first page PNG");
        assert!(png.len() > 100, "{name}: PNG carries content");
        let svg = std::fs::read_to_string(out_dir.join("slide_001.svg")).expect("first page SVG");
        assert!(svg.starts_with("<?xml"), "{name}: SVG is XML");
    }
}

#[test]
fn docx_export_png_and_svg() {
    office_terminal_docx::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("sample.docx");
    std::fs::write(&path, common::simple_docx()).unwrap();
    let out_dir = temp_dir.path().join("out");

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    let code = run_export(
        path.clone(),
        out_dir.clone(),
        ExportFormat::All,
        150,
        None,
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);

    let png = out_dir.join("slide_001.png");
    assert!(png.exists(), "PNG page exported");
    let png_bytes = std::fs::read(&png).unwrap();
    assert!(png_bytes.len() > 5_000, "PNG carries real raster content");

    let svg = out_dir.join("slide_001.svg");
    assert!(svg.exists(), "SVG page exported");
    let svg_text = std::fs::read_to_string(&svg).unwrap();
    assert!(svg_text.starts_with("<?xml"));
    assert!(svg_text.contains("viewBox="));
}

#[test]
fn docx_export_page_range_selects_pages() {
    office_terminal_docx::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("sample.docx");
    std::fs::write(&path, common::hyperlinks_and_pages_docx()).unwrap();
    let out_dir = temp_dir.path().join("out");

    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    let code = office_terminal_cli::commands::export::run_export_with_width(
        path,
        out_dir.clone(),
        ExportFormat::Png,
        96,
        Some(400),
        Some("2".to_string()),
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out_dir.join("slide_002.png").exists(), "page 2 exported");
    assert!(!out_dir.join("slide_001.png").exists(), "page 1 skipped");
}

#[test]
fn loader_rejects_docx_packages_without_a_main_part() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;
    let error = loader
        .load_deck(office_terminal_core::Source::Bytes(common::main_part_missing_docx()))
        .err()
        .expect("missing main part rejected");
    assert!(error.to_string().to_lowercase().contains("main"));
}

#[test]
fn stdin_docx_loads_through_the_same_path() {
    office_terminal_docx::register();
    let loader = DefaultPresentationLoader;
    // `Source::Bytes` exercises the same sniff-and-load path as stdin spooling.
    let deck = loader
        .load_deck(office_terminal_core::Source::Bytes(common::hyperlinks_and_pages_docx()))
        .expect("multi-page docx loads");
    let office_terminal_core::DeckSource::Docx(source) = deck else {
        panic!("expected the DOCX variant");
    };
    assert!(source.len() >= 2);
}
