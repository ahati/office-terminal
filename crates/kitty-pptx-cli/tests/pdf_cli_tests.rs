use kitty_pptx_cli::cli::ExportFormat;
use kitty_pptx_cli::commands::export::run_export;
use kitty_pptx_cli::commands::{run_dump, run_info};
use kitty_pptx_core::{DefaultPresentationLoader, DefaultSlideCompiler, DefaultSlideSvgSerializer, TinySkiaRasterizer};
use std::process::ExitCode;
use std::sync::Arc;

const MINIMAL_PDF: &[u8] = b"%PDF-1.4\n\
1 0 obj\n\
<< /Type /Catalog /Pages 2 0 R >>\n\
endobj\n\
2 0 obj\n\
<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
endobj\n\
3 0 obj\n\
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>\n\
endobj\n\
4 0 obj\n\
<< /Length 0 >>\n\
stream\n\
endstream\n\
endobj\n\
xref\n\
0 5\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
0000000202 00000 n \n\
trailer\n\
<< /Size 5 /Root 1 0 R >>\n\
startxref\n\
251\n\
%%EOF\n";

#[test]
fn test_pdf_info_command() {
    kitty_pptx_pdf::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let pdf_path = temp_dir.path().join("sample.pdf");
    std::fs::write(&pdf_path, MINIMAL_PDF).unwrap();

    let loader = DefaultPresentationLoader;
    let code = run_info(pdf_path, &loader);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_pdf_dump_commands() {
    kitty_pptx_pdf::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let pdf_path = temp_dir.path().join("sample.pdf");
    std::fs::write(&pdf_path, MINIMAL_PDF).unwrap();

    let loader = DefaultPresentationLoader;

    // Dump notes
    let code = run_dump(pdf_path.clone(), true, false, false, false, false, &loader);
    assert_eq!(code, ExitCode::SUCCESS);

    // Dump markdown
    let code = run_dump(pdf_path.clone(), false, false, true, false, false, &loader);
    assert_eq!(code, ExitCode::SUCCESS);

    // Dump json
    let code = run_dump(pdf_path, false, false, false, true, false, &loader);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_pdf_export_png_and_svg() {
    kitty_pptx_pdf::register();
    let temp_dir = tempfile::tempdir().unwrap();
    let pdf_path = temp_dir.path().join("sample.pdf");
    std::fs::write(&pdf_path, MINIMAL_PDF).unwrap();

    let out_dir = temp_dir.path().join("out");
    let loader = DefaultPresentationLoader;
    let compiler = Arc::new(DefaultSlideCompiler::new());
    let rasterizer = Arc::new(TinySkiaRasterizer::new());
    let svg_serializer = Arc::new(DefaultSlideSvgSerializer::new());

    // Export PNG
    let code = run_export(
        pdf_path.clone(),
        out_dir.clone(),
        ExportFormat::Png,
        150,
        None,
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out_dir.join("slide_001.png").exists());

    // Export SVG
    let code = run_export(
        pdf_path,
        out_dir.clone(),
        ExportFormat::Svg,
        150,
        None,
        &loader,
        &compiler,
        &rasterizer,
        &svg_serializer,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out_dir.join("slide_001.svg").exists());
}
