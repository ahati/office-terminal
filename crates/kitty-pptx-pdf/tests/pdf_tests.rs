use std::sync::Arc;
use kitty_pptx_core::PageSource;
use kitty_pptx_pdf::detect::{classify_mode, is_encrypted, is_pdf, parse_version, ReaderModeDecision};
use kitty_pptx_pdf::engine::{HayroEngine, PdfEngine};
use kitty_pptx_pdf::error::PdfError;
use kitty_pptx_pdf::source::PdfPageSource;

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
fn test_is_pdf_detection() {
    assert!(is_pdf(MINIMAL_PDF));
    assert!(is_pdf(b"   %PDF-1.7\nsome data"));
    assert!(!is_pdf(b"PK\x03\x04something"));
    assert!(!is_pdf(b"not a pdf at all"));
}

#[test]
fn test_parse_version() {
    assert_eq!(parse_version(MINIMAL_PDF), Some("PDF 1.4".to_string()));
    assert_eq!(
        parse_version(b"%PDF-2.0\n1 0 obj..."),
        Some("PDF 2.0".to_string())
    );
    assert_eq!(parse_version(b"corrupt"), None);
}

#[test]
fn test_encryption_detection() {
    assert!(!is_encrypted(MINIMAL_PDF));
    assert!(is_encrypted(b"%PDF-1.4\n... /Encrypt 5 0 R ..."));
}

#[test]
fn test_classify_mode_rules() {
    // D1: Empty -> error
    assert_eq!(classify_mode(&[]), Err(PdfError::EmptyDocument));

    // D2: 16:9 uniform landscape (1920x1080) -> Slides
    let slides_16_9 = vec![(1920.0, 1080.0); 10];
    let res = classify_mode(&slides_16_9).unwrap();
    match res {
        ReaderModeDecision::Slides {
            landscape_ratio,
            median_aspect,
            inlier_ratio,
        } => {
            assert_eq!(landscape_ratio, 1.0);
            assert!((median_aspect - 1.777).abs() < 0.01);
            assert_eq!(inlier_ratio, 1.0);
        }
        _ => panic!("Expected Slides mode"),
    }

    // D2: 10% outliers allowed (9 slides landscape, 1 portrait) -> Slides
    let mut slides_with_outlier = vec![(1024.0, 768.0); 9];
    slides_with_outlier.push((768.0, 1024.0)); // 1 portrait slide
    assert!(matches!(
        classify_mode(&slides_with_outlier).unwrap(),
        ReaderModeDecision::Slides { .. }
    ));

    // D3: Portrait document (A4 portrait) -> Document
    let a4_doc = vec![(595.28, 841.89); 20];
    let res_a4 = classify_mode(&a4_doc).unwrap();
    match res_a4 {
        ReaderModeDecision::Document {
            portrait_ratio,
            paper_size,
        } => {
            assert_eq!(portrait_ratio, 1.0);
            assert_eq!(paper_size, Some("A4"));
        }
        _ => panic!("Expected Document mode"),
    }

    // D3: Portrait majority with 1 rotated landscape table -> Document
    let mut doc_with_landscape = vec![(612.0, 792.0); 9]; // US Letter
    doc_with_landscape.push((792.0, 612.0)); // 1 landscape figure
    assert!(matches!(
        classify_mode(&doc_with_landscape).unwrap(),
        ReaderModeDecision::Document { paper_size: Some("Letter"), .. }
    ));

    // D3: Near-square majority -> Document
    let square_pages = vec![(500.0, 500.0); 5];
    assert!(matches!(
        classify_mode(&square_pages).unwrap(),
        ReaderModeDecision::Document { .. }
    ));
}

#[test]
fn test_minimal_pdf_engine_and_source() {
    let bytes = Arc::from(MINIMAL_PDF);
    let engine = HayroEngine::open(Arc::clone(&bytes)).expect("open minimal PDF");
    assert_eq!(engine.page_count(), 1);
    assert_eq!(engine.page_size_pt(0), (200.0, 200.0));
    assert_eq!(engine.page_rotation(0), 0);

    let source = PdfPageSource::open(bytes).expect("open page source");
    assert_eq!(source.len(), 1);
    let (w_emu, h_emu) = source.page_size_emu(0);
    assert_eq!(w_emu, (200.0 * 12_700.0) as i64);
    assert_eq!(h_emu, (200.0 * 12_700.0) as i64);

    let (tx, _rx) = crossbeam_channel::unbounded();
    let vector_slide = source.compile_page(0, &tx).expect("compile page");
    assert_eq!(vector_slide.width_emu, w_emu);
    assert_eq!(vector_slide.height_emu, h_emu);
    assert_eq!(vector_slide.commands.len(), 1); // 1 DrawBitmap command
}
