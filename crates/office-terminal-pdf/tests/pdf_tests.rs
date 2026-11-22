use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use office_terminal_core::PageSource;
use office_terminal_core::{EmuRect, RenderTargetSpec, RgbaSlide, TextLine, VectorSlide};
use office_terminal_pdf::detect::{classify_mode, is_encrypted, is_pdf, parse_version, ReaderModeDecision};
use office_terminal_pdf::engine::{HayroEngine, PdfEngine};
use office_terminal_pdf::error::PdfError;
use office_terminal_pdf::source::PdfPageSource;

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
fn test_encryption_detection_scan_edge_cases() {
    // Buffers smaller than the needle can never match.
    assert!(!is_encrypted(b""));
    assert!(!is_encrypted(b"%PDF-1.4"));

    // Pattern at the very end of a large buffer: found by the tail scan.
    let mut tail_hit = vec![b'x'; 8192];
    tail_hit.extend_from_slice(b"trailer << /Encrypt 5 0 R >>");
    assert!(is_encrypted(&tail_hit));

    // Pattern before the tail window: found by the full-buffer scan.
    let mut head_hit = b"%PDF-1.4 /Encrypt".to_vec();
    head_hit.extend_from_slice(&[b'x'; 8192]);
    assert!(is_encrypted(&head_hit));

    // Partial '/' occurrences that never complete the pattern.
    assert!(!is_encrypted(b"%PDF-1.4 trailer << /Encrxyz >> end"));
    assert!(!is_encrypted(b"/Encryp"));
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

#[test]
fn test_render_raster_native_target() {
    let bytes = Arc::from(MINIMAL_PDF);
    let source = PdfPageSource::open(bytes).expect("open page source");

    // The minimal PDF page is 200x200 pt: a 120 px target renders at
    // exactly 120x120 pixels with no codec round-trip.
    let target = RenderTargetSpec {
        width_px: 120,
        height_px: 120,
    };
    let slide = source
        .render_raster(0, &target)
        .expect("native raster path is implemented")
        .expect("native render succeeds");
    assert_eq!(slide.width, 120);
    assert_eq!(slide.height, 120);
    assert_eq!(slide.pixels.len(), 120 * 120 * 4);

    // Opaque white background survives un-premultiplication exactly.
    assert_eq!(&slide.pixels[..4], &[255, 255, 255, 255]);

    // Out-of-bounds pages report a native error so callers can fall back.
    assert!(matches!(source.render_raster(1, &target), Some(Err(_))));
}

/// Mock engine proving the search corpus is built lazily: it counts how
/// often `extract_text` is invoked.
struct LazyCorpusEngine {
    page_count: usize,
    extract_calls: AtomicUsize,
}

impl PdfEngine for LazyCorpusEngine {
    fn page_count(&self) -> usize {
        self.page_count
    }

    fn page_size_pt(&self, _page: usize) -> (f32, f32) {
        (612.0, 792.0)
    }

    fn page_rotation(&self, _page: usize) -> u16 {
        0
    }

    fn render_rgba(&self, _page: usize, _scale: f32) -> Result<RgbaSlide, PdfError> {
        Ok(RgbaSlide {
            width: 1,
            height: 1,
            pixels: Arc::from(vec![0, 0, 0, 0].into_boxed_slice()),
        })
    }

    fn render_raster(
        &self,
        _page: usize,
        _target: &RenderTargetSpec,
    ) -> Result<RgbaSlide, PdfError> {
        Ok(RgbaSlide {
            width: 1,
            height: 1,
            pixels: Arc::from(vec![0, 0, 0, 0].into_boxed_slice()),
        })
    }

    fn compile_raster(&self, _page: usize) -> Result<VectorSlide, PdfError> {
        Ok(VectorSlide::new(612 * 12_700, 792 * 12_700))
    }

    fn compile_vector(&self, _page: usize) -> Result<VectorSlide, PdfError> {
        Ok(VectorSlide::new(612 * 12_700, 792 * 12_700))
    }

    fn outline(&self) -> Vec<office_terminal_core::TocEntry> {
        Vec::new()
    }

    fn links(&self, _page: usize) -> Vec<office_terminal_core::SlideLink> {
        Vec::new()
    }

    fn extract_text(&self, _page: usize) -> Result<Vec<TextLine>, PdfError> {
        self.extract_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![TextLine {
            text: "line".to_string(),
            rect: EmuRect::new(0.0, 0.0, 1.0, 1.0),
        }])
    }
}

#[test]
fn test_search_corpus_builds_lazily() {
    let engine = Arc::new(LazyCorpusEngine {
        page_count: 3,
        extract_calls: AtomicUsize::new(0),
    });
    let source =
        PdfPageSource::from_engine(Arc::clone(&engine) as Arc<dyn PdfEngine>).expect("source");

    // Opening the document and asking for the corpus size must not
    // interpret any page content streams.
    assert_eq!(engine.extract_calls.load(Ordering::SeqCst), 0);
    let corpus = source.search_corpus();
    assert_eq!(corpus.page_count(), 3);
    assert_eq!(engine.extract_calls.load(Ordering::SeqCst), 0);

    // The first text query extracts all pages exactly once.
    assert_eq!(corpus.page_text(1).len(), 1);
    assert_eq!(engine.extract_calls.load(Ordering::SeqCst), 3);

    // Subsequent queries reuse the cache without re-extraction.
    assert_eq!(corpus.page_text(0).len(), 1);
    assert_eq!(engine.extract_calls.load(Ordering::SeqCst), 3);
}

#[test]
fn test_external_pdf_toc() {
    if let Ok(path) = std::env::var("KITTY_PPTX_TEST_PDF") {
        if let Ok(data) = std::fs::read(&path) {
            let source = PdfPageSource::open(Arc::from(data.into_boxed_slice())).expect("open pdf");
            let toc = source.toc();
            for entry in &toc {
                assert!(
                    !entry.title.contains('\0') && !entry.title.contains('\u{FFFD}'),
                    "TOC title must not contain NUL or replacement chars: {:?}",
                    entry.title
                );
            }
        }
    }
}

#[test]
fn test_pdf_text_string_decoding() {
    use office_terminal_pdf::text_encoding::decode_pdf_text_string;

    // UTF-16BE with BOM
    let utf16_be = [0xFE, 0xFF, 0x00, b'P', 0x00, b'r', 0x00, b'e', 0x00, b'f', 0x00, b'a', 0x00, b'c', 0x00, b'e'];
    assert_eq!(decode_pdf_text_string(&utf16_be), "Preface");

    // UTF-16LE with BOM
    let utf16_le = [0xFF, 0xFE, b'H', 0x00, b'e', 0x00, b'l', 0x00, b'l', 0x00, b'o', 0x00];
    assert_eq!(decode_pdf_text_string(&utf16_le), "Hello");

    // UTF-8 with BOM
    let utf8_bom = [0xEF, 0xBB, 0xBF, b'T', b'e', b's', b't'];
    assert_eq!(decode_pdf_text_string(&utf8_bom), "Test");

    // Raw ASCII
    let ascii = b"Introduction to Computer Vision";
    assert_eq!(decode_pdf_text_string(ascii), "Introduction to Computer Vision");

    // PDFDocEncoding with special chars: 0x84 (em dash), 0x92 (trademark)
    let pdfdoc = [b'C', b'o', b'm', b'p', b'a', b'n', b'y', 0x92, b' ', 0x84, b' ', b'2', b'0', b'2', b'6'];
    assert_eq!(decode_pdf_text_string(&pdfdoc), "Company™ — 2026");

    // Embedded control chars and NUL bytes stripped
    let dirty = [b'A', 0x00, b'B', 0x07, b'C', b'\t', b'D'];
    assert_eq!(decode_pdf_text_string(&dirty), "ABC D");
}


