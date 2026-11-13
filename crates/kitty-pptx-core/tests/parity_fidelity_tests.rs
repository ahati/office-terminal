//! Reference-parity fidelity VRT.
//!
//! One `#[test]` per committed deck in `parity_support::vrt::DECKS`; each test
//! renders every slide through the production seam
//! (`DefaultPresentationLoader` → `DefaultSlideCompiler::compile_in_deck` →
//! `TinySkiaRasterizer`) and compares it against the adopted reference PNGs.
//!
//! Port of `packages/pptx/tests/visual/visual.spec.ts` (1f5c6e1d) "visual
//! fidelity" for the single tracked deck:
//!
//! - pixelmatch with `threshold = 0.20`, `includeAA: true`, both images padded
//!   to a common size on white;
//! - fail when a slide's diff exceeds 20 %;
//! - fail when `tests/parity/scores/<deck>.json` has no score for a slide;
//! - fail when a slide's match-% drops more than 0.5 pt below its committed
//!   score (the ratchet).
//!
//! Run with `--nocapture` to see the per-slide score table:
//!
//! ```sh
//! cargo test -p kitty-pptx-core --test parity_fidelity_tests -- --nocapture
//! ```
//!
//! Oracle modes (see `tests/fixtures/parity/README.md`):
//!
//! ```sh
//! UPDATE_REFS=1   cargo test -p kitty-pptx-core --test parity_fidelity_tests
//! UPDATE_SCORES=1 cargo test -p kitty-pptx-core --test parity_fidelity_tests
//! ```
//!
//! `UPDATE_REFS` adopts the current render as ground truth; only do that when
//! a reference-PNG change is explicitly approved, because the ratchet scores
//! then have to be regenerated as well. The two modes are mutually exclusive.

mod parity_support;

use parity_support::vrt::{self, DeckSpec};

fn run_fidelity_deck(spec: &'static DeckSpec) {
    let report = vrt::run_deck(spec);
    print!("{}", vrt::format_table(&report));

    for slide in &report.slides {
        if let Some((ref_w, ref_h)) = slide.size_mismatch() {
            eprintln!(
                "  {} slide {}: size mismatch reference={ref_w}x{ref_h} \
                 render={}x{}; compared after padding",
                report.name, slide.slide, slide.actual_width, slide.actual_height
            );
        }
    }

    assert!(
        report.violations().is_empty(),
        "{} fidelity failures:\n  {}",
        report.name,
        report.violations().join("\n  ")
    );
}

#[test]
fn parity_fidelity_sample_1() {
    // Add one test per `vrt::DECKS` entry. Lookup panics on a typo instead of
    // silently skipping coverage.
    run_fidelity_deck(vrt::deck("sample-1"));
}
