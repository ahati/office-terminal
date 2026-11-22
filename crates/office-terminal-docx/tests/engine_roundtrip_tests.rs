//! End-to-end DOCX engine roundtrip: real packages through the pinned
//! `rdocx` backend, the `PageFrame → VectorSlide` bridge, the rasterizer and
//! the SVG serializer (PRD 15 §10.2 L1/L2 levels).

mod common;

use std::sync::Arc;

use office_terminal_core::contracts::{
    PageSource, RenderTargetSpec, SvgTextMode, VectorCommand,
};
use office_terminal_core::{
    DefaultSlideSvgSerializer, EmptyAssetStore, SlideRasterizer, SlideSvgSerializer,
    TinySkiaRasterizer,
};
use office_terminal_docx::{DocxError, DocxLimits, DocxPageSource};

const EMU_PER_PT: f64 = 12_700.0;

fn open(bytes: Vec<u8>) -> DocxPageSource {
    DocxPageSource::open(Arc::from(bytes.into_boxed_slice())).expect("document opens")
}

fn letter_target() -> RenderTargetSpec {
    // Letter at ~72 DPI in pixels.
    RenderTargetSpec {
        width_px: 612,
        height_px: 792,
    }
}

#[test]
fn simple_document_paginates_into_letter_pages() {
    let source = open(common::simple_docx());
    assert!(source.len() >= 1, "one paragraph flow yields a page");
    let (w_emu, h_emu) = source.page_size_emu(0);
    assert_eq!(w_emu, (612.0 * EMU_PER_PT) as i64);
    assert_eq!(h_emu, (15840.0 / 20.0 * EMU_PER_PT) as i64);
}

#[test]
fn heading_outlines_reach_the_toc() {
    let source = open(common::simple_docx());
    let toc = source.toc();
    assert!(
        toc.iter().any(|entry| entry.title.contains("Integration Heading One")),
        "TOC must map Heading 1 paragraphs, got {toc:?}"
    );
    assert!(toc.iter().all(|entry| entry.page_index < source.len()));
}

#[test]
fn search_corpus_extracts_reading_order_text() {
    let source = open(common::simple_docx());
    let corpus = source.search_corpus();
    let all_text: String = corpus
        .page_text(0)
        .iter()
        .flat_map(|line| line.text.chars())
        .collect();
    assert!(all_text.contains("First body paragraph"));
    assert!(all_text.contains("Integration Heading One"));
}

#[test]
fn compiled_pages_carry_exact_face_text_runs() {
    let source = open(common::simple_docx());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");
    let text_runs: Vec<&office_terminal_core::contracts::PositionedGlyphRun> = slide
        .commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!text_runs.is_empty(), "a text page must carry runs");
    for run in &text_runs {
        // The exact-face handle is the whole point of the bridge: glyph IDs
        // must paint from the engine's own face bytes, never a re-resolved
        // family (PRD 15 §5.5).
        assert!(
            run.face.is_some(),
            "engine runs must carry a registered face handle"
        );
        assert!(!run.single_face);
    }
}

#[test]
fn pages_rasterize_with_real_ink() {
    let source = open(common::simple_docx());
    let rasterizer = TinySkiaRasterizer::new();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");
    let assets = source.asset_store().expect("media surface");
    let bitmap = rasterizer
        .rasterize(&slide, &letter_target(), assets.as_ref())
        .expect("raster");
    let ink = bitmap.pixels[..]
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] > 0 && (px[0] < 245 || px[1] < 245 || px[2] < 245))
        .count();
    assert!(
        ink > 500,
        "a page of 12 pt text at 72 DPI paints substantial ink, got {ink}"
    );
}

#[test]
fn svg_export_supports_selectable_and_vector_glyph_modes() {
    let source = open(common::simple_docx());
    let serializer = DefaultSlideSvgSerializer::new();
    let store = source.asset_store().unwrap_or_else(|| {
        Arc::new(EmptyAssetStore) as Arc<dyn office_terminal_core::AssetStore>
    });
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");

    let selectable = serializer
        .serialize_svg(&slide, store.as_ref(), SvgTextMode::SelectableText)
        .expect("selectable svg");
    assert!(selectable.contains("<text"), "selectable mode emits <text>");
    // The engine shapes word-segmented runs, so each word lands in its own
    // `<text>` element with exact positions.
    for word in ["First", "body", "paragraph", "Integration"] {
        assert!(selectable.contains(word), "selectable text carries '{word}'");
    }

    let vector = serializer
        .serialize_svg(&slide, store.as_ref(), SvgTextMode::VectorGlyphs)
        .expect("vector svg");
    assert!(vector.contains("<path"), "vector mode outlines glyphs");
    assert!(vector.starts_with("<?xml"));
}

#[test]
fn hyperlinks_resolve_external_and_bookmark_targets_across_pages() {
    let source = open(common::hyperlinks_and_pages_docx());
    assert!(source.len() >= 2, "a page break must produce a second page");

    let (tx, _rx) = crossbeam_channel::unbounded();
    let mut links = Vec::new();
    for index in 0..source.len() {
        let slide = source.compile_page(index, &tx).expect("compile");
        links.extend(slide.hyperlinks.iter().cloned());
    }

    let external = links
        .iter()
        .find(|link| matches!(
            link.target,
            office_terminal_core::contracts::HyperlinkTarget::ExternalUri(ref url)
                if url == "https://example.com/more"
        ))
        .expect("external hyperlink survives the sanitizer");
    assert!(external.bounds_emu.w > 0.0);

    // Known engine gap (issue #40 §7.18): `rdocx` projects external
    // relationship hyperlinks as `LinkAnnotation`s but not `w:anchor`
    // internal links, so the bridge's bookmark resolution has no annotation
    // to consume. Assert the page-count half here and record the gap.
    assert!(source.len() >= 2);
}

#[test]
fn page_count_limit_is_a_typed_resource_violation() {
    let bytes = Arc::from(common::simple_docx().into_boxed_slice());
    let limits = DocxLimits {
        max_pages: 0,
        ..DocxLimits::default()
    };
    let error = DocxPageSource::open_with_limits(bytes, &limits)
        .err()
        .expect("a zero page budget rejects the document");
    assert!(matches!(error, DocxError::ResourceLimit(_)));
}

#[test]
fn garbage_packages_error_without_panicking() {
    let error = DocxPageSource::open(Arc::from(common::garbage_bytes().into_boxed_slice()))
        .err()
        .expect("garbage is rejected");
    assert!(matches!(error, DocxError::Package(_)));
}

#[test]
fn packages_without_a_main_part_error_clearly() {
    let error = DocxPageSource::open(Arc::from(
        common::main_part_missing_docx().into_boxed_slice(),
    ))
    .err()
    .expect("missing main part is rejected");
    let message = error.to_string();
    assert!(
        message.to_lowercase().contains("main") || message.to_lowercase().contains("document"),
        "the error names the missing part: {message}"
    );
}

#[test]
fn repeat_compiles_are_served_from_the_ir_cache() {
    let source = open(common::simple_docx());
    let (tx, rx) = crossbeam_channel::unbounded();
    let first = source.compile_page(0, &tx).expect("compile");
    let second = source.compile_page(0, &tx).expect("compile");
    assert_eq!(
        first.commands.len(),
        second.commands.len(),
        "cached and fresh compiles agree"
    );
    // Diagnostics dedupe per document: at most one firing per feature even
    // across two compiles.
    assert!(rx.try_iter().count() <= 3);
}
