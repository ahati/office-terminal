//! Golden-image E2E tests for the deck-to-pixels pipeline.
//!
//! Each fixture deck is loaded, compiled and rasterized through the same
//! production seam as `kitty-pptx-cli export --format png`, then compared
//! against committed reference PNGs under `tests/golden/png/<deck>/`.
//!
//! Regenerate references after an intentional rendering change:
//!
//! ```sh
//! UPDATE_GOLDENS=1 cargo test -p kitty-pptx-core --test golden_tests
//! ```
//!
//! The font stack matters: goldens are generated with DejaVu Sans installed
//! (see `tests/golden/README.md`). Different fonts change glyph rasterization
//! and require regenerating the text-heavy decks.

mod golden_support;

use crossbeam_channel::unbounded;
use golden_support::{compare_or_update, render_deck, sample_fractional, DeckConfig, Thresholds};
use kitty_pptx_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RgbaSlide, SlideCompiler,
    SlideRasterizer, Source, TinySkiaRasterizer,
};

/// One row per committed fixture deck.
///
/// `width_px` is fixed for reproducibility; the height is derived from the
/// slide aspect ratio. Tune the thresholds here when a deck legitimately
/// needs more slack (for example a deck dominated by text on CI images with a
/// different font stack). The default policy is deliberately strict: a real
/// renderer regression moves millions of pixels, not a handful of AA edges.
const DECKS: &[DeckConfig] = &[
    DeckConfig {
        name: "e2e_deck",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "pptxgenjs_sample",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "sample",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "KEY02",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "ShapePlusImage",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "chart_pt_color_bg1",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "smartart-org-chart",
        width_px: 800,
        thresholds: Thresholds::DEFAULT,
    },
    DeckConfig {
        name: "ArtisticEffectSample",
        width_px: 800,
        thresholds: Thresholds::LOOSE,
    },
];

fn deck(name: &str) -> &'static DeckConfig {
    DECKS
        .iter()
        .find(|config| config.name == name)
        .unwrap_or_else(|| panic!("no golden config for deck {name}"))
}

fn run_deck(config: &'static DeckConfig) {
    let rendered = render_deck(config);
    assert!(
        !rendered.slides.is_empty(),
        "{}: fixture contains no slides",
        config.name
    );
    for index in 0..rendered.slides.len() {
        let label = format!("{} slide {}", config.name, index + 1);
        compare_or_update(&rendered, index, &label);
    }
}

#[test]
fn golden_e2e_deck() {
    run_deck(deck("e2e_deck"));
}

#[test]
fn golden_pptxgenjs_sample() {
    run_deck(deck("pptxgenjs_sample"));
}

#[test]
fn golden_sample() {
    run_deck(deck("sample"));
}

#[test]
fn golden_key02() {
    run_deck(deck("KEY02"));
}

#[test]
fn golden_shape_plus_image() {
    run_deck(deck("ShapePlusImage"));
}

#[test]
fn golden_chart_pt_color_bg1() {
    run_deck(deck("chart_pt_color_bg1"));
}

#[test]
fn golden_smartart_org_chart() {
    run_deck(deck("smartart-org-chart"));
}

#[test]
fn golden_artistic_effect_sample() {
    run_deck(deck("ArtisticEffectSample"));
}

// ---------------------------------------------------------------------------
// Font-independent invariants
//
// These assertions only touch solid shape fills and the background gradient.
// They run without golden files and therefore stay meaningful on machines
// whose font stack differs from the one that produced the reference PNGs.
// ---------------------------------------------------------------------------

/// Verified ground truth for `e2e_deck.pptx` slide 1 (16:9, 800x450 target).
const E2E_SLIDE1_POINTS: &[(f64, f64, [u8; 3], &str)] = &[
    (0.163, 0.444, [0xC0, 0x00, 0x00], "red rect"),
    (0.375, 0.444, [0x00, 0x70, 0xC0], "blue ellipse"),
    (0.542, 0.444, [0xED, 0x7D, 0x31], "orange roundRect"),
    (0.731, 0.444, [0x70, 0xAD, 0x47], "green star5"),
    (0.194, 0.728, [0x70, 0x30, 0xA0], "purple rightArrow"),
    (0.412, 0.756, [0x00, 0xB0, 0xB0], "teal triangle"),
    (0.500, 0.050, [0xFF, 0xFF, 0xFF], "white background"),
];

#[test]
fn e2e_slide1_solid_regions_match_ground_truth() {
    let rendered = render_deck(deck("e2e_deck"));
    let slide = &rendered.slides[0];
    for &(fx, fy, expected, label) in E2E_SLIDE1_POINTS {
        let sample = sample_fractional(slide, fx, fy);
        for channel in 0..3 {
            let delta = (sample[channel] - expected[channel] as f64).abs();
            assert!(
                delta <= 8.0,
                "{label} at ({fx:.3}, {fy:.3}): channel {channel} sampled {:.1}, \
                 expected {:.1} (delta {:.1} > 8); full sample {sample:?}",
                sample[channel],
                expected[channel] as f64,
                delta
            );
        }
        assert!(
            sample[3] >= 250.0,
            "{label}: expected opaque fill, sampled alpha {:.1}",
            sample[3]
        );
    }
}

#[test]
fn e2e_slide2_background_gradient_is_light_towards_bottom_right() {
    let rendered = render_deck(deck("e2e_deck"));
    let slide = &rendered.slides[1];
    // Stay clear of the title text box (top-left) and the preset shapes.
    let top_left = sample_fractional(slide, 0.02, 0.02);
    let bottom_right = sample_fractional(slide, 0.98, 0.98);
    let luma_tl = golden_support::luminance(top_left);
    let luma_br = golden_support::luminance(bottom_right);
    assert!(
        luma_br > luma_tl + 8.0,
        "slide 2 gradient should be darker at the top-left and lighter at the \
         bottom-right: top-left {top_left:?} (luma {luma_tl:.1}), bottom-right \
         {bottom_right:?} (luma {luma_br:.1})"
    );
}

// ---------------------------------------------------------------------------
// Harness self-check
// ---------------------------------------------------------------------------

#[test]
fn diff_metrics_classifies_pixels_and_channels() {
    let expected = [0u8, 0, 0, 255, 0, 0, 0, 255];
    let actual = [10u8, 10, 10, 255, 20, 0, 0, 255];
    let metrics = golden_support::diff_metrics(&actual, &expected, 12);
    assert_eq!(metrics.total_pixels, 2);
    assert_eq!(metrics.outlier_pixels, 1);
    assert!((metrics.outlier_fraction - 0.5).abs() < f64::EPSILON);
    assert_eq!(metrics.max_diff, 20);
    // Channel deltas: 10+10+10+0+20+0+0+0 = 50 over 8 channels.
    assert!((metrics.mean_abs_diff - 6.25).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Stability
// ---------------------------------------------------------------------------

/// The rasterizer must be byte-deterministic; otherwise golden comparisons and
/// terminal image caching produce spurious diffs.
#[test]
fn rasterization_is_byte_deterministic() {
    let config = deck("e2e_deck");
    let path = golden_support::fixture_path(config.name);
    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path))
        .expect("e2e_deck fixture must load");
    let target = config.target(presentation.slide_width, presentation.slide_height);
    let compiler = DefaultSlideCompiler::new();
    let rasterizer = TinySkiaRasterizer::new();
    let (tx, _rx) = unbounded();
    let vector = compiler
        .compile_in_deck(
            &presentation.slides[0],
            &presentation.slides,
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        )
        .expect("slide 1 compiles");

    let first: RgbaSlide = rasterizer
        .rasterize(&vector, &target, assets.as_ref())
        .expect("first rasterization");
    let second: RgbaSlide = rasterizer
        .rasterize(&vector, &target, assets.as_ref())
        .expect("second rasterization");
    assert_eq!(first.width, second.width);
    assert_eq!(first.height, second.height);
    assert!(
        first.pixels == second.pixels,
        "two consecutive rasterizations of the same VectorSlide differ"
    );
}
