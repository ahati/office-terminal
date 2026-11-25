//! Playwright-spec parity batch — the reference's browser acceptance specs,
//! ported as far as the terminal pipeline can express them; the browser-only
//! halves carry an N/A row in `tests/parity/coverage/viewer.md`.
//!
//! Reference (read-only @ `1f5c6e1d`): `/tmp/oooxml/packages/pptx/tests/visual/`
//! `non-ooxml-rejection.spec.ts` (1), `worker-equivalence.spec.ts` (3),
//! `visual.spec.ts` (1), `part-failure.spec.ts` (1),
//! `table-image-fill.spec.ts` (2), `pdf-acceptance.spec.ts` (1),
//! `responsive-overlay.spec.ts` (1), `rotated-tile-coverage.spec.ts` (1),
//! `scroll-zoom-settle.spec.ts` (3), `inline-equation-fallback.spec.ts` (1),
//! `warp-strip-quality.spec.ts` (2), `bevel-ring.spec.ts` (1),
//! `font-source-worker-equivalence.spec.ts` (2),
//! `worker-selection-equivalence.spec.ts` (1).
//!
//! N/A (browser canvas / DOM): the specs assert Chromium's compositor output
//! (overlay overflow, alpha banding, rim brightness) or the DOM overlay tree;
//! the terminal pipeline rasterizes through `tiny-skia` and has neither a
//! scroll area nor a DOM.
//!
//! BLOCKED (no production API, see `tests/parity/coverage/viewer.md`): the
//! table-only image-fill plan, PDF export, the rotated-tile coverage oracle,
//! viewer zoom, the equation-fallback branch, the bevel compositor,
//! font-source demand facts and the worker selection realm. The parse-error
//! placeholder case is enabled: `Slide::parse_error` now paints its card.
//!
//! Blocked cases (no production API yet) are archived, not compiled, in
//! `tests/parity/pending/parity_playwright_tests.rs.txt`; the
//! ledger keeps their rows.

use std::sync::Arc;

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{RenderTargetSpec, RgbaSlide};
use office_terminal_core::loader::{AssetStore, DefaultPresentationLoader, PresentationLoader, Source};
use office_terminal_core::{DefaultSlideCompiler, ParseError, SlideCompiler, SlideRasterizer, TinySkiaRasterizer};
use pptx_model::Slide;

mod parity_support;
use parity_support::deck;

const TARGET: RenderTargetSpec = RenderTargetSpec {
    width_px: 480,
    height_px: 360,
};

/// A one-slide synthetic package (the reference's fixture deck) plus a
/// `p:sp` text box so the frame carries real ink.
fn fixture_deck() -> Vec<u8> {
    deck::deck(&deck::text_box(
        &deck::para(&deck::plain_run("Hello parity"), deck::DEFAULT_PPR),
        deck::DEFAULT_BODY_PR,
    ))
}

fn load(bytes: Vec<u8>) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError> {
    DefaultPresentationLoader.load(Source::Bytes(bytes))
}

fn render_slide(slide: &Slide, deck: &[Slide], assets: &dyn AssetStore) -> RgbaSlide {
    let (tx, _rx) = unbounded();
    let vector = DefaultSlideCompiler::new()
        .compile_in_deck(slide, deck, 9_144_000, 6_858_000, &tx)
        .expect("compile");
    TinySkiaRasterizer::new()
        .rasterize(&vector, &TARGET, assets)
        .expect("rasterize")
}

// ── non-ooxml rejection (ref non-ooxml-rejection.spec.ts:3-16) ─────────────

/// "main and worker mode reject non-OOXML input with OoxmlError not-ooxml": the
/// loader rejects bytes that are not an OOXML package with a typed error instead
/// of producing a placeholder deck. ref non-ooxml-rejection.spec.ts:5-15
#[test]
fn non_ooxml_input_is_rejected_with_a_typed_error() {
    for payload in [
        b"not a zip at all".to_vec(),
        vec![0u8; 64],
        // A PK header with no readable directory.
        vec![b'P', b'K', 0x03, 0x04, 0, 0, 0, 0],
    ] {
        match load(payload.clone()) {
            Err(ParseError::ArchiveError(message)) => {
                assert!(!message.is_empty(), "the rejection carries a diagnostic");
            }
            Err(other) => panic!("expected a typed archive error, got {other}"),
            Ok(_) => panic!("non-OOXML input must not load a deck"),
        }
    }
}

/// The rejection is stable across repeated loads (the reference asserts all eight
/// mode/option combinations reject identically). ref non-ooxml-rejection.spec.ts:12-14
#[test]
fn non_ooxml_rejection_is_repeatable() {
    let first = load(b"junk".to_vec()).err().map(|error| error.to_string());
    let second = load(b"junk".to_vec()).err().map(|error| error.to_string());
    assert_eq!(first, second);
    assert!(first.is_some());
}

// ── placeholder text and part failures (ref part-failure.spec.ts:8-34) ────

/// Main and worker mode paint the placeholder identically for the built-in text
/// and for both option forms; the option changes the text, and the failure is
/// listed and passed to the function on the calling thread.
///
/// The reference hashes PNG pixels and records the worker wire: here both
/// realms rebuild the same model, so the realm-invariant payload is the
/// compiled display list, which is exactly what each realm hands its backend.
/// The `partFailures` listing half is covered by `parity_viewer_lifecycle_tests`
/// (`a_broken_slide_part_keeps_the_other_slides_intact`).
#[test]
fn placeholder_text_matches_in_main_and_worker_mode() {
    use std::sync::Mutex;

    use office_terminal_core::compiler::{CompileOptions, ErrorPlaceholder, PartFailure};
    use office_terminal_core::contracts::{VectorCommand, VectorSlide};

    const MESSAGE: &str = "ppt/slides/slide2.xml: unexpected end of stream";
    let slide: Slide = serde_json::from_value(serde_json::json!({
        "index": 1,
        "slideNumber": 2,
        "partName": "ppt/slides/slide2.xml",
        "background": null,
        "elements": [],
        "elementSources": [],
        "comments": [],
        "parseError": MESSAGE,
    }))
    .expect("broken slide model");

    let compile = |options: &CompileOptions| -> VectorSlide {
        let (tx, _rx) = unbounded();
        DefaultSlideCompiler::new()
            .compile_opts(&slide, 9_144_000, 6_858_000, options, &tx)
            .expect("the card compiles")
    };
    let texts = |card: &VectorSlide| -> Vec<String> {
        card.commands
            .iter()
            .flat_map(|command| match command {
                VectorCommand::DrawText { runs, .. } => runs.iter().map(|run| run.text.clone()).collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect()
    };

    let calls: Arc<Mutex<Vec<PartFailure>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&calls);
    let functional = CompileOptions::with_placeholder_fn(move |failure| {
        log.lock().expect("failure log").push(failure.clone());
        Some(ErrorPlaceholder {
            title: format!("Custom {} title", failure.kind),
            detail: Some("Custom detail line".to_string()),
        })
    });

    // Each variant is rendered twice, once per realm.
    let main_builtin = compile(&CompileOptions::default());
    let worker_builtin = compile(&CompileOptions::default());
    let main_static = compile(&CompileOptions::with_placeholder_text("Custom placeholder title", None));
    let worker_static = compile(&CompileOptions::with_placeholder_text("Custom placeholder title", None));
    let main_function = compile(&functional);
    let worker_function = compile(&functional);

    assert_eq!(main_builtin, worker_builtin, "builtin text is realm-neutral");
    assert_eq!(main_static, worker_static, "static text is realm-neutral");
    assert_eq!(main_function, worker_function, "function text is realm-neutral");

    let payloads = [
        texts(&main_builtin).join("\n"),
        texts(&main_static).join("\n"),
        texts(&main_function).join("\n"),
    ];
    let distinct: std::collections::BTreeSet<&String> = payloads.iter().collect();
    assert_eq!(distinct.len(), 3, "the option changes the painted text");
    assert!(payloads[0].contains("Slide 2 could not be displayed"));
    assert!(payloads[1].contains("Custom placeholder title"));
    assert!(payloads[2].contains("Custom slide title"));

    // Two failed-part renders, two resolver calls, each with the same record.
    let recorded = calls.lock().expect("failure log").clone();
    assert_eq!(recorded.len(), 2, "one call per failed-part render");
    for failure in recorded {
        assert_eq!(failure.kind, "slide");
        assert_eq!((failure.slide_index, failure.slide_number), (1, 2));
        assert!(failure.message.contains("ppt/slides/slide2.xml"), "{}", failure.message);
    }
}

// ── main/worker equivalence (ref worker-equivalence.spec.ts:27-134) ────────

/// "worker mode matches main mode" for one slide. The worker realm rebuilds the
/// deck from the same package bytes it re-parses, so the contract ownable on the
/// Rust side is that two independent loads of one package render the same slide
/// identically. ref worker-equivalence.spec.ts:27-56
///
/// NOTE: the reference worker wire carries the parsed model. `pptx_model`'s JSON
/// is not currently round-trippable (`TextRunData` has required `strikeDouble` /
/// `underlineLineNoFill` fields), so the byte-for-byte wire half is NOT asserted
/// here — see `coverage/viewer.md` (follow-up).
#[test]
fn main_and_worker_mode_produce_identical_pixels() {
    let bytes = fixture_deck();
    let (main, main_assets) = load(bytes.clone()).expect("fixture deck loads");
    let (worker, worker_assets) = load(bytes).expect("fixture deck loads (second realm)");

    let main_frame = render_slide(&main.slides[0], &main.slides, main_assets.as_ref());
    let worker_frame = render_slide(&worker.slides[0], &worker.slides, worker_assets.as_ref());

    assert_eq!(main_frame.width, worker_frame.width);
    assert_eq!(main_frame.height, worker_frame.height);
    assert!(
        main_frame.pixels == worker_frame.pixels,
        "both realms must render the same pixels"
    );
    // Sanity: the frame carries ink, so the comparison is not degenerate.
    assert!(
        main_frame.pixels.chunks(4).any(|pixel| pixel[0..3] != [255, 255, 255]),
        "the fixture slide is not blank"
    );
}

/// The slot's render width is the only per-dispatch input: the same slide at the
/// same width is never re-rendered into different pixels.
/// ref worker-equivalence.spec.ts:27-56, `scroll-viewer.test.ts:775-798`
#[test]
fn rendering_the_same_slide_twice_is_stable() {
    let (presentation, assets) = load(fixture_deck()).expect("fixture deck loads");
    let slides: &[Slide] = &presentation.slides;
    let first = render_slide(&slides[0], slides, assets.as_ref());
    let second = render_slide(&slides[0], slides, assets.as_ref());
    assert!(first.pixels == second.pixels, "no per-run nondeterminism");
}

/// The `partName → index` map agrees across the two realms (IX-nav). The worker
/// carries `partNames: pres.slides.map((s) => s.partName)`; rebuilding the deck
/// from that projection must resolve every part name identically.
/// ref worker-equivalence.spec.ts:136-157
#[test]
fn worker_part_index_agrees_with_main_mode() {
    let (presentation, _assets) = load(fixture_deck()).expect("fixture deck loads");
    let part_names: Vec<String> = presentation
        .slides
        .iter()
        .map(|slide| slide.part_name.clone().unwrap_or_default())
        .collect();
    assert!(!part_names.is_empty());
    assert!(part_names.iter().all(|name| !name.is_empty()));

    // The worker-meta projection: index + partName only, no slide content.
    let meta: Vec<Slide> = part_names
        .iter()
        .enumerate()
        .map(|(index, part)| {
            serde_json::from_value(serde_json::json!({
                "index": index,
                "slideNumber": index + 1,
                "partName": part,
                "elements": [],
                "elementSources": [],
                "comments": []
            }))
            .expect("worker slide projection")
        })
        .collect();

    for (index, part) in part_names.iter().enumerate() {
        let target = format!("../slides/{}", part.rsplit('/').next().unwrap_or(part));
        assert_eq!(
            office_terminal_core::resolve_internal_slide(&target, &part_names[0], &presentation.slides),
            Some(index)
        );
        assert_eq!(
            office_terminal_core::resolve_internal_slide(&target, &part_names[0], &meta),
            Some(index),
            "{part} resolves identically off the worker projection"
        );
    }
}

// ── VRT deck acceptance (ref visual.spec.ts:250-341) ───────────────────────
/// "differences must be empty" for every parity deck: the browser-free
/// restatement is that a deck renders deterministically, so a reference-PNG
/// comparison cannot be flaky for our own reasons.
/// ref visual.spec.ts:258-281
#[test]
fn a_deck_renders_deterministically_on_every_slide() {
    let (presentation, assets) = load(fixture_deck()).expect("fixture deck loads");
    assert!(!presentation.slides.is_empty(), "the deck reports a slide count");
    let slides: &[Slide] = &presentation.slides;
    for slide in slides {
        let first = render_slide(slide, slides, assets.as_ref());
        let second = render_slide(slide, slides, assets.as_ref());
        assert_eq!(first.width, 480);
        assert_eq!(first.height, 360);
        assert!(first.pixels == second.pixels, "slide {} is deterministic", slide.index);
    }
}
