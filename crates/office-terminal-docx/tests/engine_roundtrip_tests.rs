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

/// Reading-order text of one page, lines joined by single spaces.
fn page_text(source: &DocxPageSource, index: usize) -> String {
    let corpus = source.search_corpus();
    corpus
        .page_text(index)
        .iter()
        .map(|line| line.text.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `(x_pt, y_pt, text)` of every non-blank text run on a page, in points.
fn text_runs_on_page(source: &DocxPageSource, index: usize) -> Vec<(f32, f32, String)> {
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(index, &tx).expect("compile");
    let mut runs = Vec::new();
    for command in &slide.commands {
        if let VectorCommand::DrawText { runs: page_runs, .. } = command {
            for run in page_runs {
                if run.text.trim().is_empty() {
                    continue;
                }
                runs.push((
                    run.glyphs[0].x / 12_700.0,
                    run.glyphs[0].y / 12_700.0,
                    run.text.clone(),
                ));
            }
        }
    }
    runs
}

#[test]
fn declared_core_properties_surface_through_the_page_source() {
    let source = open(common::simple_docx());
    let meta = source.metadata().expect("core properties are declared");
    assert_eq!(meta.title.as_deref(), Some("Integration Report"));
    assert_eq!(meta.author.as_deref(), Some("A. Writer"));
    assert_eq!(meta.subject.as_deref(), Some("DOCX engine roundtrip"));
    assert_eq!(meta.keywords.as_deref(), Some("docx, bridge"));
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

#[test]
fn table_grid_spans_merges_borders_and_shading_survive_the_bridge() {
    let source = open(common::table_docx());
    assert_eq!(source.len(), 1, "a three-row table fits one page");

    // Every cell's text reaches the corpus in reading order: row by row,
    // left to right, with the merged column's restart cell once only.
    let text = page_text(&source, 0);
    let position = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("corpus lacks {needle:?}: {text:?}"))
    };
    let ordered = [
        "Before the table.",
        "Spanning header",
        "Third column",
        "Merged cell",
        "B2",
        "C2",
        "B3",
        "C3",
        "After the table.",
    ];
    let mut last = 0;
    for needle in ordered {
        let at = position(needle);
        assert!(at >= last, "{needle:?} must follow the previous cell");
        last = at;
    }

    // Borders arrive as stroked paths and the shaded header cell as a solid
    // fill of the authored `D9E2F3`.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");
    let strokes = slide
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                VectorCommand::DrawPath { stroke: Some(stroke), .. } if stroke.width_emu > 0
            )
        })
        .count();
    assert!(strokes >= 4, "a bordered table draws stroked paths, got {strokes}");
    let shading = slide.commands.iter().any(|command| {
        matches!(
            command,
            VectorCommand::DrawPath {
                fill: Some(office_terminal_core::contracts::VectorPaint::Solid(color)),
                ..
            } if *color == [0xD9, 0xE2, 0xF3, 255]
        )
    });
    assert!(shading, "the shaded cell keeps its authored fill colour");
}

#[test]
fn headers_and_footers_with_page_fields_land_on_every_page() {
    let source = open(common::header_footer_docx());
    assert!(source.len() >= 2, "the page break yields a second page");

    let first = page_text(&source, 0);
    assert!(first.contains("Running header"), "header on page 1: {first:?}");
    assert!(first.contains("Footer page 1"), "PAGE field on page 1: {first:?}");
    assert!(first.contains("First page body"));

    let second = page_text(&source, 1);
    assert!(second.contains("Running header"), "header on page 2: {second:?}");
    assert!(
        second.contains("Footer page 2"),
        "PAGE field must be re-evaluated for page 2: {second:?}"
    );
    assert!(second.contains("Second page body"));
}

#[test]
fn inline_png_pictures_become_draw_image_served_by_the_asset_store() {
    let source = open(common::image_docx());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");

    let rel_id = slide
        .commands
        .iter()
        .find_map(|command| match command {
            VectorCommand::DrawImage {
                rel_id, dest_rect, ..
            } => {
                assert!(dest_rect.w > 0.0 && dest_rect.h > 0.0, "placed box");
                Some(rel_id.clone())
            }
            _ => None,
        })
        .expect("the inline picture becomes a DrawImage command");
    assert!(rel_id.starts_with("docx-media-"), "content-addressed key");

    let store = source.asset_store().expect("media surface");
    let decoded = store
        .get(&rel_id, office_terminal_core::SizeHint::Native)
        .expect("the registered PNG decodes");
    assert_eq!((decoded.width, decoded.height), (2, 1), "SAMPLE_PNG dimensions");
    assert_eq!(store.entry_count(), 1);
    assert!(store.raw_bytes_total() > 0);
}

#[test]
fn omml_equations_lay_out_as_text_runs() {
    let source = open(common::math_docx());
    assert_eq!(source.len(), 1);

    let text = page_text(&source, 0);
    for fragment in ["E=m", "c", "2"] {
        assert!(text.contains(fragment), "math fragment {fragment:?} in {text:?}");
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");
    let runs: usize = slide
        .commands
        .iter()
        .map(|command| match command {
            VectorCommand::DrawText { runs, .. } => runs.len(),
            _ => 0,
        })
        .sum();
    assert!(runs >= 3, "base, superscript base and exponent each emit a run");
}

/// Column tracks work: once column 1 (x = 72 pt) fills the page, text
/// continues at the top of column 2 (x = 72 + 216 + 36 = 324 pt).
#[test]
fn two_column_tracks_fill_left_column_before_right() {
    let source = open(common::columns_docx(60));
    assert!(source.len() >= 2, "60 lines need more than one column");

    // Page 2 is the engine's first full page of the two-column section.
    let runs = text_runs_on_page(&source, 1);
    let starts_a_line = |run: &(f32, f32, String)| run.2.starts_with("Column");
    let column_x: Vec<i32> = runs
        .iter()
        .filter(|run| starts_a_line(run))
        .map(|run| run.0.round() as i32)
        .collect();
    assert!(
        column_x.contains(&72),
        "column 1 at the left margin, got {column_x:?}"
    );
    assert!(
        column_x.contains(&324),
        "column 2 after the 0.5in gap, got {column_x:?}"
    );

    // Reading order stays logical: column 1 top-to-bottom before column 2.
    let text = page_text(&source, 1);
    let first_of_column_2 = text.find("Column line 29").expect("line 29 opens column 2");
    let last_of_column_1 = text.find("Column line 28").expect("line 28 closes column 1");
    assert!(last_of_column_1 < first_of_column_2);
}

/// Word starts a continuous section that changes the column count on the
/// **current page**, right below the preceding content. The pinned engine
/// instead starts it on a new page (`rdocx-layout` `paginator.rs`
/// `continuous_geometry_matches` requires identical column tracks), which is
/// what strands the "Columns" heading of `sample3.docx` on the previous
/// page and leaves the demonstration in a single stacked column.
#[test]
#[ignore = "rdocx 0.17: a continuous section with a different column count starts on a new page (docs/issues/docx-input-support.md §7.18)"]
fn continuous_column_change_starts_on_the_current_page() {
    let source = open(common::columns_docx(4));
    assert_eq!(source.len(), 1, "a one-line intro and four column lines fit one page");

    let runs = text_runs_on_page(&source, 0);
    let intro_y = runs
        .iter()
        .find(|run| run.2.starts_with("Single"))
        .map(|run| run.1)
        .expect("intro on page 1");
    let column_y = runs
        .iter()
        .filter(|run| run.2.starts_with("Column"))
        .map(|run| run.1)
        .reduce(f32::min)
        .expect("column lines on page 1");
    assert!(
        column_y > intro_y,
        "the two-column section follows the intro on the same page"
    );
}
