//! Viewer-lifecycle parity batch — ports of the reference presentation/renderer
//! lifecycle contracts: per-part failure degradation (#1974), document-level
//! resource poison, destroy idempotence and cache invalidation.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/src/`
//! `part-failures.ts` (slide failures keep the deck loadable),
//! `presentation-part-failures.test.ts` (7 cases),
//! `presentation-resource-poison.test.ts` (2 cases),
//! `presentation-destroy.test.ts` (14), `presentation-cfb-guard.test.ts` (3),
//! `presentation-decrypt.test.ts` (4), `presentation-preflight.test.ts` (23),
//! `viewer-part-failures.test.ts` (1), `presentation-font-options.test.ts` (3),
//! `presentation-handle-media-errors.test.ts` (6),
//! `renderer-destroy-invalidation.test.ts` (1), `presentation-bounded-pipeline.test.ts`
//! (1), `presentation-canvas-realm.test.ts` (1), `presentation.mode.test.ts` (2),
//! `presentation.image.test.ts` (2), `presentation.markdown.test.ts` (1),
//! `presentation-handle.test.ts` (2), `presentation-worker-bitmap-ownership.test.ts`
//! (6), `worker-init-hang.test.ts` (7), `render-worker-init-hang.test.ts`.
//!
//! BLOCKED (no production API, see `tests/parity/coverage/viewer.md`):
//! `assertNotCfbContainer` (no CFB sniff: `pptx-parser` reports a CFB payload as
//! an archive error, never `ParseError::PasswordProtected`), `decryptOoxml`
//! (no Agile decryption), `PresentationPreflightBuilder` (no deck-level font
//! union / structural projection), `Slide::parse_error` placeholder painting
//! (the compiler ignores `parse_error`), `WarningEvent::MediaFailed` (never
//! emitted) and the worker init timeout (`WorkerPool` has no hang bound).
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_viewer_lifecycle_tests.rs.txt`; the
//! ledger keeps their rows.

use std::collections::HashSet;
use std::io::{Cursor, Write};
use std::sync::Arc;

use crossbeam_channel::Sender;
use kitty_pptx_core::contracts::{
    InvalidateCause, PipelineEntry, PrefetchPlan, RenderPipeline, RenderTargetSpec, VectorSlide,
    WarningEvent,
};
use kitty_pptx_core::loader::{
    AssetStore, AssetStoreStats, DecodedImage, DefaultPresentationLoader, PresentationLoader,
    SizeHint, Source,
};
use kitty_pptx_core::{
    DefaultRenderPipeline, DefaultSlideCompiler, RenderError, SlideCompiler, TinySkiaRasterizer,
};
use zip::write::SimpleFileOptions;

mod parity_support;
use parity_support::deck;

// ── a two-slide package with one deliberately broken slide part ────────────

/// The reference's #1974 fixture: a deck whose second slide part cannot be
/// parsed while the first stays intact. Built by extending the synthetic
/// package with a second slide part + its rels + the `sldId`/content-type wiring.
/// ref part-failures.ts:1-20
fn deck_with_broken_second_slide(broken: Option<&[u8]>) -> Vec<u8> {
    let parts = deck::package_parts(
        &deck::text_box("hello", deck::DEFAULT_BODY_PR),
        deck::SLIDE_WIDTH,
        deck::SLIDE_HEIGHT,
    );
    let mut files: Vec<(String, String)> = Vec::new();
    for (name, body) in parts {
        let body = match name.as_str() {
            "[Content_Types].xml" => body.replace(
                "</Types>",
                concat!(
                    r#"<Override PartName="/ppt/slides/slide2.xml" "#,
                    r#"ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
                    "</Types>"
                ),
            ),
            "ppt/presentation.xml" => body.replace(
                r#"<p:sldId id="256" r:id="rId2"/>"#,
                r#"<p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/>"#,
            ),
            "ppt/_rels/presentation.xml.rels" => body.replace(
                "</Relationships>",
                &format!(
                    r#"<Relationship Id="rId3" Type="{}/slide" Target="slides/slide2.xml"/></Relationships>"#,
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                ),
            ),
            _ => body,
        };
        files.push((name, body));
    }
    let slide2 = match broken {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => files
            .iter()
            .find(|(name, _)| name == "ppt/slides/slide1.xml")
            .map(|(_, body)| body.clone())
            .expect("slide1 present"),
    };
    files.push(("ppt/slides/slide2.xml".to_string(), slide2));
    files.push((
        "ppt/slides/_rels/slide2.xml.rels".to_string(),
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#.to_string(),
    ));

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in files {
            writer.start_file(name, options).expect("start zip entry");
            writer.write_all(body.as_bytes()).expect("write zip entry");
        }
        writer.finish().expect("finish zip");
    }
    cursor.into_inner()
}

fn load(bytes: Vec<u8>) -> Result<pptx_model::Presentation, kitty_pptx_core::ParseError> {
    DefaultPresentationLoader
        .load(Source::Bytes(bytes))
        .map(|(presentation, _store)| presentation)
}

// ── part failures (ref part-failures.ts:1-20, presentation-part-failures) ──

/// "The deck still loads; that slide shows a placeholder and every other slide
/// renders normally." ref part-failures.ts:5-8
#[test]
fn a_broken_slide_part_keeps_the_other_slides_intact() {
    // Invalid UTF-8 in the second slide part (the reference's #1974 trigger).
    let bytes = deck_with_broken_second_slide(Some(&[0xff, 0xfe, 0x00]));
    let presentation = load(bytes).expect("the deck still loads");
    assert_eq!(presentation.slides.len(), 2, "both slide ids are kept");
    assert_eq!(presentation.slides[0].parse_error, None);
    let failure = presentation.slides[1]
        .parse_error
        .as_deref()
        .unwrap_or_else(|| panic!("slide 2 carries a part failure"));
    assert!(
        failure.starts_with("ppt/slides/slide2.xml:"),
        "the message is part-tagged: {failure}"
    );
    // Healthy slides keep their content.
    assert!(!presentation.slides[0].elements.is_empty());
}

/// "A broken slide's `show` flag cannot be read, so it is never treated as
/// hidden." ref part-failures.ts:9-11
#[test]
fn a_broken_slide_is_never_treated_as_hidden() {
    let bytes = deck_with_broken_second_slide(Some(&[0xff, 0xfe, 0x00]));
    let presentation = load(bytes).expect("deck loads");
    assert!(!presentation.slides[1].hidden);
    assert_eq!(
        presentation.slides[1].part_name.as_deref(),
        Some("ppt/slides/slide2.xml"),
        "the slide→index map still resolves a jump to the broken slide"
    );
}

/// A deck with no broken part reports no failure at all — the healthy half of
/// the same contract. ref presentation-part-failures.test.ts:84-100
#[test]
fn a_healthy_deck_reports_no_part_failure() {
    let presentation = load(deck_with_broken_second_slide(None)).expect("deck loads");
    assert_eq!(presentation.slides.len(), 2);
    assert!(presentation
        .slides
        .iter()
        .all(|slide| slide.parse_error.is_none()));
    assert!(presentation.slides.iter().all(|slide| !slide.hidden));
}

// ── document-level resource poison (ref presentation-resource-poison) ──────

struct NoAssets;

impl AssetStore for NoAssets {
    fn get(&self, _rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        Err(AssetError::NotFound("none".into()))
    }
    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}
use kitty_pptx_core::AssetError;

/// A compiler that fails exactly one slide index, mimicking the reference's
/// sticky fatal resource violation. ref presentation-resource-poison.test.ts:39-76
struct FailingCompiler {
    failing: usize,
}

impl SlideCompiler for FailingCompiler {
    fn compile(
        &self,
        slide: &pptx_model::Slide,
        slide_width: i64,
        slide_height: i64,
        warning_sink: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        if slide.index == self.failing {
            return Err(RenderError::DecodedImageLimit {
                metric: "inflated-bytes",
                limit: 1024,
                observed: 1025,
            });
        }
        DefaultSlideCompiler::new().compile(slide, slide_width, slide_height, warning_sink)
    }
}

fn deck_of(count: usize) -> Arc<pptx_model::Presentation> {
    let slides = (0..count)
        .map(|index| {
            serde_json::from_value::<pptx_model::Slide>(serde_json::json!({
                "index": index,
                "slideNumber": index + 1,
                "partName": format!("ppt/slides/slide{}.xml", index + 1),
                                "elements": [],
                "elementSources": [],
                "comments": []
            }))
            .expect("slide")
        })
        .collect();
    Arc::new(pptx_model::Presentation {
        slide_width: 9_144_000,
        slide_height: 6_858_000,
        slides,
        default_text_color: Some("000000".into()),
        major_font: Some("Calibri".into()),
        minor_font: Some("Calibri".into()),
        hlink_color: None,
        fol_hlink_color: None,
    })
}

fn failing_pipeline(count: usize, failing: usize) -> DefaultRenderPipeline {
    DefaultRenderPipeline::new(
        deck_of(count),
        Arc::new(NoAssets),
        Arc::new(FailingCompiler { failing }),
        Arc::new(TinySkiaRasterizer::new()),
    )
}

const TARGET: RenderTargetSpec = RenderTargetSpec {
    width_px: 320,
    height_px: 180,
};

/// The first fatal violation is replayed by every later read of that slide
/// instead of being retried or downgraded to "not loaded".
/// ref presentation-resource-poison.test.ts:39-64
#[test]
fn a_failed_slide_replays_its_error_on_every_later_read() {
    let pipeline = failing_pipeline(3, 1);
    let plan = PrefetchPlan {
        center: 1,
        behind: 0,
        ahead: 0,
        stage: true,
    };
    pipeline.prefetch(&plan, &TARGET);

    // Force the failing compile synchronously so the assertion is not racy.
    let error = pipeline
        .render_sync(1, &TARGET)
        .err()
        .expect("slide 1 fails to compile");
    assert_eq!(
        error,
        RenderError::DecodedImageLimit {
            metric: "inflated-bytes",
            limit: 1024,
            observed: 1025
        }
    );

    // The worker records the failure; repeated reads replay it verbatim.
    pipeline.workers().cancel_all();
    pipeline.workers().schedule(1, 10, TARGET);
    let mut recorded = None;
    for _ in 0..200 {
        if let Some(failure) = pipeline.workers().get_failed(1) {
            recorded = Some(failure);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let recorded = recorded.expect("the worker records the failure");

    assert_eq!(recorded, error, "the same typed error is replayed");
    assert_eq!(pipeline.workers().get_failed(1), Some(recorded.clone()));
    assert_eq!(pipeline.workers().get_failed(1), Some(recorded));
    match pipeline.entry(1) {
        PipelineEntry::Failed { error } => assert_eq!(error, error_of(&pipeline)),
        other => panic!(
            "a poisoned slide reports Failed, got {}",
            entry_name(&other)
        ),
    }
}

fn error_of(pipeline: &DefaultRenderPipeline) -> RenderError {
    pipeline
        .workers()
        .get_failed(1)
        .unwrap_or_else(|| panic!("slide 1 must stay failed"))
}

fn entry_name(entry: &PipelineEntry) -> &'static str {
    match entry {
        PipelineEntry::Ready { .. } => "Ready",
        PipelineEntry::Rendering => "Rendering",
        PipelineEntry::NotLoaded => "NotLoaded",
        PipelineEntry::Failed { .. } => "Failed",
    }
}

/// The poison survives unrelated cache work but is dropped only by an explicit
/// reset. ref presentation-resource-poison.test.ts:66-100
#[test]
fn clearing_the_poison_allows_a_fresh_attempt() {
    let pipeline = failing_pipeline(2, 1);
    pipeline
        .render_sync(0, &TARGET)
        .map(|_| ())
        .expect("slide 0 renders");
    assert!(pipeline.render_sync(1, &TARGET).is_err());

    pipeline.workers().clear_failed();
    assert!(pipeline.workers().get_failed(1).is_none());
    assert!(pipeline.workers().get_failed(0).is_none());
    // Still failing when re-attempted — the poison was cleared, not the failure.
    assert!(pipeline.render_sync(1, &TARGET).is_err());
    // ... and the healthy slide is unaffected throughout.
    assert!(pipeline.cache().contains_raster(0, &TARGET));
}

// ── destroy / invalidation (ref presentation-destroy.test.ts:115-127) ─────

/// "is safe to call destroy() twice". ref presentation-destroy.test.ts:123-127
#[test]
fn destroy_is_idempotent() {
    let pipeline = failing_pipeline(4, usize::MAX);
    pipeline.prefetch(
        &PrefetchPlan {
            center: 0,
            behind: 0,
            ahead: 3,
            stage: true,
        },
        &TARGET,
    );
    pipeline.workers().shutdown();
    let after_first = (
        pipeline.workers().in_flight_count(),
        pipeline.workers().queue_len(),
    );
    assert_eq!(
        after_first,
        (0, 0),
        "destroy drains in-flight and queued work"
    );
    pipeline.workers().shutdown();
    assert_eq!(
        (
            pipeline.workers().in_flight_count(),
            pipeline.workers().queue_len()
        ),
        (0, 0),
        "a second destroy is a no-op"
    );
    // The pipeline stays usable for reads after shutdown (no panic, no poison).
    assert!(matches!(
        pipeline.entry(0),
        PipelineEntry::NotLoaded | PipelineEntry::Ready { .. }
    ));
}

/// A deck reload cancels in-flight prefetches and drops both cache tiers, so a
/// render that resolves afterwards is never served as fresh.
/// ref presentation-destroy.test.ts:115-121, renderer-destroy-invalidation.test.ts:19-66
#[test]
fn deck_reload_cancels_in_flight_work_and_drops_the_caches() {
    let pipeline = failing_pipeline(5, usize::MAX);
    pipeline
        .render_sync(0, &TARGET)
        .map(|_| ())
        .expect("slide 0 renders");
    assert!(pipeline.cache().has_vector(0));
    assert!(pipeline.cache().contains_raster(0, &TARGET));

    pipeline.prefetch(
        &PrefetchPlan {
            center: 0,
            behind: 1,
            ahead: 2,
            stage: true,
        },
        &TARGET,
    );
    let generation = pipeline.current_generation();
    pipeline.invalidate(InvalidateCause::DeckReload);

    assert_eq!(pipeline.current_generation(), generation + 1);
    assert_eq!(
        pipeline.workers().in_flight_count(),
        0,
        "in-flight cancelled"
    );
    assert_eq!(pipeline.workers().queue_len(), 0, "queue purged");
    assert!(!pipeline.cache().has_vector(0));
    assert!(!pipeline.cache().contains_raster(0, &TARGET));
    assert!(!pipeline.workers().is_in_flight(0));

    // A later prefetch of a window that excludes the old slides is honoured.
    let keep: HashSet<usize> = [3usize, 4].into_iter().collect();
    pipeline.workers().cancel_except(&keep);
    assert_eq!(pipeline.workers().in_flight_count(), 0);
}

/// `InvalidateCause::Resize` keeps every cached artefact (only the cell grid
/// changed). ref presentation-destroy.test.ts:129-158 (no cache churn on resize)
#[test]
fn resize_invalidation_keeps_both_cache_tiers() {
    let pipeline = failing_pipeline(2, usize::MAX);
    pipeline
        .render_sync(0, &TARGET)
        .map(|_| ())
        .expect("render");
    pipeline.invalidate(InvalidateCause::Resize);
    assert!(pipeline.cache().has_vector(0));
    assert!(pipeline.cache().contains_raster(0, &TARGET));
    assert!(matches!(pipeline.entry(0), PipelineEntry::Ready { .. }));
}

/// Reading a slide beyond the deck is a typed error, not a panic — the
/// reference's slide-index validation. ref slide-pull-client.ts:104-112
#[test]
fn out_of_range_slide_reads_fail_typed() {
    let pipeline = failing_pipeline(2, usize::MAX);
    match pipeline.render_sync(2, &TARGET) {
        Err(RenderError::CompilationFailed(message)) => {
            assert!(message.contains("out of bounds"), "{message}");
        }
        Ok(_) => panic!("expected a typed bounds error, got a rendered slide"),
        Err(other) => panic!("expected a typed bounds error, got {other:?}"),
    }
}
