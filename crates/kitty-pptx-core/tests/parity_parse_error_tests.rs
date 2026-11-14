//! Parse-error placeholder parity (RB7 / #1974).
//!
//! Ports `packages/pptx/src/renderer.parse-error.test.ts` (reference read-only @
//! `1f5c6e1d`). The reference drives `renderSlide` against a recording 2D
//! context and asserts the painted `fillText`/`fillRect` sequence; this port
//! asserts the same facts on the compiled display list, which is the backend-
//! independent form of the same paint (ref renderer.ts:8188-8274, 8477-8487).
//!
//! One reference case is N/A: `errorPlaceholder: () => ({ title: 1 })` must
//! reject with a `TypeError`. [`ErrorPlaceholder::title`] is a `String`, so an
//! invalid result is unrepresentable rather than rejected.

use std::sync::{Arc, Mutex};

use crossbeam_channel::unbounded;
use kitty_pptx_core::compiler::{
    CompileOptions, DefaultSlideCompiler, ErrorPlaceholder, PartFailure, SlideCompiler,
};
use kitty_pptx_core::contracts::{VectorCommand, VectorPaint, VectorSlide};
use pptx_model::Slide;

const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 6_858_000;
const MESSAGE: &str = "ppt/slides/slide3.xml: unexpected end of stream";
/// The reference's card fill (ref renderer.ts:8198).
const CARD_BG: [u8; 4] = [0xf7, 0xf7, 0xf8, 255];

/// ref `brokenSlide()` — index 2, slide number 3, no elements, one parseError.
fn broken_slide() -> Slide {
    serde_json::from_value(serde_json::json!({
        "index": 2,
        "slideNumber": 3,
        "partName": "ppt/slides/slide3.xml",
        "background": null,
        "elements": [],
        "elementSources": [],
        "comments": [],
        "parseError": MESSAGE,
    }))
    .expect("broken slide model")
}

/// ref `{ index: 0, slideNumber: 1, background: null, elements: [] }`.
fn healthy_slide() -> Slide {
    serde_json::from_value(serde_json::json!({
        "index": 0,
        "slideNumber": 1,
        "background": null,
        "elements": [],
        "elementSources": [],
        "comments": [],
    }))
    .expect("healthy slide model")
}

fn compile(slide: &Slide, options: &CompileOptions) -> VectorSlide {
    let (tx, _rx) = unbounded();
    DefaultSlideCompiler::new()
        .compile_opts(slide, SLIDE_W, SLIDE_H, options, &tx)
        .expect("a parse-error slide compiles to a card")
}

fn texts(slide: &VectorSlide) -> Vec<String> {
    slide
        .commands
        .iter()
        .flat_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => {
                runs.iter().map(|run| run.text.clone()).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect()
}

fn card_fills(slide: &VectorSlide) -> usize {
    slide
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                VectorCommand::DrawPath {
                    fill: Some(VectorPaint::Solid(color)),
                    ..
                } if *color == CARD_BG
            )
        })
        .count()
}

// ── RB7 renderSlide placeholder (ref `describe('RB7 renderSlide placeholder')`)

/// "paints a placeholder carrying the parseError message for a broken slide":
/// the heading names the slide, the detail includes the part-tagged error, and
/// the card is filled. ref renderer.parse-error.test.ts:80-105
#[test]
fn broken_slide_paints_a_part_tagged_placeholder() {
    let compiled = compile(&broken_slide(), &CompileOptions::default());
    let texts = texts(&compiled);

    assert!(
        texts.iter().any(|text| text.contains("Slide 3")),
        "heading names the slide: {texts:?}"
    );
    assert!(
        texts.join(" ").contains("slide3.xml"),
        "detail carries the part tag: {texts:?}"
    );
    assert_eq!(card_fills(&compiled), 1, "one full-slide card fill");
    // Card fill + dashed border + glyph + heading + one detail line. The
    // reference's decision list is exactly one `fillRect`, one `strokeRect` and
    // three `fillText`s.
    assert_eq!(compiled.commands.len(), 5, "card command count");
    assert_eq!(compiled.background, None, "the card replaces the background");
    assert!(compiled.hyperlinks.is_empty());
}

/// "a healthy slide (no parseError) does NOT draw the placeholder heading".
/// ref renderer.parse-error.test.ts:107-124
#[test]
fn healthy_slide_never_paints_the_parse_error_placeholder() {
    let compiled = compile(&healthy_slide(), &CompileOptions::default());
    let texts = texts(&compiled);
    assert!(
        !texts.iter().any(|text| text.contains("could not be displayed")),
        "healthy slide must not paint a placeholder: {texts:?}"
    );
    assert_eq!(card_fills(&compiled), 0);
    assert!(compiled.commands.is_empty(), "no elements, no commands");
}

// ── #1974 errorPlaceholder on the slide placeholder ────────────────────────

/// "keeps the built-in text when the option is absent or returns undefined".
/// ref renderer.parse-error.test.ts:135-140
#[test]
fn builtin_text_is_kept_when_the_option_is_absent_or_returns_none() {
    let slide = broken_slide();
    let builtin = texts(&compile(&slide, &CompileOptions::default()));
    assert!(builtin.contains(&"Slide 3 could not be displayed".to_string()));
    assert!(builtin.contains(&MESSAGE.to_string()));

    // `() => undefined` keeps the built-in text verbatim.
    let undefined = CompileOptions::with_placeholder_fn(|_| None);
    assert_eq!(texts(&compile(&slide, &undefined)), builtin);
}

/// "paints a custom title and hides the detail for detail: null".
/// ref renderer.parse-error.test.ts:142-147
#[test]
fn custom_title_replaces_the_heading_and_detail_none_hides_the_part() {
    for detail in [None, Some(String::new())] {
        let options = CompileOptions::with_placeholder_text("Unavailable", detail);
        let texts = texts(&compile(&broken_slide(), &options));
        assert!(texts.contains(&"Unavailable".to_string()), "{texts:?}");
        assert!(
            !texts.join(" ").contains("slide3.xml"),
            "detail: null drops the part tag: {texts:?}"
        );
        assert_eq!(texts.len(), 2, "glyph + heading only");
    }
}

/// "resolves a function from the slide in-realm": the resolver sees exactly one
/// failure record and its text is painted.
/// ref renderer.parse-error.test.ts:149-163
#[test]
fn error_placeholder_closure_receives_the_slide_failure() {
    let seen: Arc<Mutex<Vec<PartFailure>>> = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&seen);
    let options = CompileOptions::with_placeholder_fn(move |failure| {
        capture.lock().expect("failure log").push(failure.clone());
        Some(ErrorPlaceholder {
            title: format!("Broken slide #{}", failure.slide_number),
            detail: Some("Ask the author".to_string()),
        })
    });
    let texts = texts(&compile(&broken_slide(), &options));

    assert_eq!(
        *seen.lock().expect("failure log"),
        vec![PartFailure {
            kind: "slide",
            slide_index: 2,
            slide_number: 3,
            message: MESSAGE.to_string(),
        }]
    );
    assert!(texts.contains(&"Broken slide #3".to_string()));
    assert!(texts.contains(&"Ask the author".to_string()));
}

/// "never evaluates the function for a healthy slide".
/// ref renderer.parse-error.test.ts:166-172
#[test]
fn healthy_slide_never_evaluates_the_error_placeholder() {
    let calls = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&calls);
    let options = CompileOptions::with_placeholder_fn(move |_| {
        *counter.lock().expect("call counter") += 1;
        Some(ErrorPlaceholder {
            title: "x".to_string(),
            detail: None,
        })
    });
    compile(&healthy_slide(), &options);
    assert_eq!(*calls.lock().expect("call counter"), 0);
}
