//! Shared helpers for the golden-image E2E tests.
//!
//! This module is compiled into each integration test that declares
//! `mod golden_support;` (it is not a test target on its own).
//!
//! The tests deliberately exercise the *production* pipeline seam:
//! `DefaultPresentationLoader` -> `DefaultSlideCompiler::compile_in_deck` ->
//! `TinySkiaRasterizer::rasterize`, exactly like the CLI `export` command.
//! Golden PNGs therefore guard the whole deck-to-pixels path, not just one
//! isolated renderer.

#![allow(dead_code)] // Not every test target uses every helper.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crossbeam_channel::unbounded;
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec,
    RgbaSlide, SlideCompiler, SlideRasterizer, Source, TinySkiaRasterizer,
};

/// Per-deck golden comparison policy.
#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    /// Maximum allowed mean absolute per-channel difference (0..=255 scale).
    pub mean_abs_diff: f64,
    /// Maximum allowed fraction of pixels with a channel diff above
    /// `channel_tolerance`.
    pub outlier_fraction: f64,
    /// Per-channel difference that marks a pixel as an outlier (0..=255).
    pub channel_tolerance: u8,
}

impl Thresholds {
    pub const DEFAULT: Self = Self {
        mean_abs_diff: 2.0,
        outlier_fraction: 0.02,
        channel_tolerance: 12,
    };

    pub const LOOSE: Self = Self {
        mean_abs_diff: 4.0,
        outlier_fraction: 0.05,
        channel_tolerance: 16,
    };
}

/// A committed fixture deck plus its fixed deterministic render spec.
#[derive(Clone, Copy, Debug)]
pub struct DeckConfig {
    /// File stem, e.g. `pptxgenjs_sample` (also the golden directory name).
    pub name: &'static str,
    /// Fixed render width in pixels. Height is derived from the slide aspect.
    pub width_px: u32,
    pub thresholds: Thresholds,
}

impl DeckConfig {
    /// Fixed-width target, height derived from the deck's aspect ratio.
    pub fn target(&self, slide_width: i64, slide_height: i64) -> RenderTargetSpec {
        let height = (self.width_px as f64 * slide_height as f64 / slide_width as f64).round();
        RenderTargetSpec {
            width_px: self.width_px,
            height_px: height.max(1.0) as u32,
        }
    }
}

/// Directory containing the committed `.pptx` fixtures.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("fixtures")
}

/// Directory that holds committed golden PNGs.
pub fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("png")
}

pub fn fixture_path(deck: &str) -> PathBuf {
    fixtures_dir().join(format!("{deck}.pptx"))
}

/// Whether the generated `.pptx` fixture is present in the working tree.
pub fn fixture_available(deck: &str) -> bool {
    fixture_path(deck).is_file()
}

pub fn golden_path(deck: &str, slide_index: usize) -> PathBuf {
    goldens_dir()
        .join(deck)
        .join(format!("slide_{:03}.png", slide_index + 1))
}

pub fn update_goldens() -> bool {
    matches!(std::env::var("UPDATE_GOLDENS").as_deref(), Ok("1"))
}

/// Render all slides of one fixture deck through the production pipeline.
pub struct DeckRender {
    pub config: &'static DeckConfig,
    pub width_px: u32,
    pub height_px: u32,
    pub slides: Vec<RgbaSlide>,
}

type Cache = Mutex<HashMap<&'static str, Arc<DeckRender>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Loads, compiles and rasterizes a deck once per test binary.
pub fn render_deck(config: &'static DeckConfig) -> Arc<DeckRender> {
    let mut cache = cache().lock().expect("golden deck cache poisoned");
    if let Some(rendered) = cache.get(config.name) {
        return Arc::clone(rendered);
    }

    let path = {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let file_name = format!("{}.pptx", config.name);
        [
            manifest.join("tests/golden/fixtures").join(&file_name),
            PathBuf::from("tests/golden/fixtures").join(&file_name),
            PathBuf::from("crates/office-terminal-core/tests/golden/fixtures").join(&file_name),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| {
            panic!(
                "golden fixture missing: {}\n\
                 The .pptx fixtures are generated in-repo. Regenerate this deck with \
                 `node crates/office-terminal-core/tests/golden/gen_pptxgenjs.js` and then run \
                 `python3 crates/office-terminal-core/tests/golden/normalize_pptx.py`; see \
                 `tests/golden/README.md` for the fixture policy.",
                manifest
                    .join("tests/golden/fixtures")
                    .join(&file_name)
                    .display()
            )
        })
    };

    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path.clone()))
        .unwrap_or_else(|e| panic!("failed to load fixture {}: {e}", path.display()));
    let target = config.target(presentation.slide_width, presentation.slide_height);

    let compiler = DefaultSlideCompiler::new();
    let rasterizer = TinySkiaRasterizer::new();
    let (tx, _rx) = unbounded();

    let mut slides = Vec::with_capacity(presentation.slides.len());
    for (index, slide) in presentation.slides.iter().enumerate() {
        let vector = compiler
            .compile_in_deck(
                slide,
                &presentation.slides,
                presentation.slide_width,
                presentation.slide_height,
                &tx,
            )
            .unwrap_or_else(|e| {
                panic!("{} slide {} failed to compile: {e}", config.name, index + 1)
            });
        let rendered = rasterizer
            .rasterize(&vector, &target, assets.as_ref())
            .unwrap_or_else(|e| {
                panic!(
                    "{} slide {} failed to rasterize: {e}",
                    config.name,
                    index + 1
                )
            });
        assert_eq!(rendered.width, target.width_px);
        assert_eq!(rendered.height, target.height_px);
        slides.push(rendered);
    }

    let rendered = Arc::new(DeckRender {
        config,
        width_px: target.width_px,
        height_px: target.height_px,
        slides,
    });
    cache.insert(config.name, Arc::clone(&rendered));
    rendered
}

/// Difference statistics between two equally sized RGBA buffers.
#[derive(Clone, Copy, Debug)]
pub struct DiffMetrics {
    pub mean_abs_diff: f64,
    pub outlier_fraction: f64,
    pub max_diff: u8,
    pub outlier_pixels: usize,
    pub total_pixels: usize,
}

/// Pixel comparison used by the golden harness.
pub fn diff_metrics(actual: &[u8], expected: &[u8], channel_tolerance: u8) -> DiffMetrics {
    assert_eq!(
        actual.len(),
        expected.len(),
        "RGBA buffer size mismatch: actual {} bytes vs expected {} bytes",
        actual.len(),
        expected.len()
    );
    assert_eq!(actual.len() % 4, 0);
    let mut sum: u64 = 0;
    let mut outliers = 0usize;
    let mut max_diff = 0u8;
    for (a, e) in actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
    {
        let mut pixel_max = 0u8;
        for channel in 0..4 {
            let diff = a[channel].abs_diff(e[channel]);
            sum += diff as u64;
            pixel_max = pixel_max.max(diff);
            max_diff = max_diff.max(diff);
        }
        if pixel_max > channel_tolerance {
            outliers += 1;
        }
    }
    let total_pixels = actual.len() / 4;
    DiffMetrics {
        mean_abs_diff: sum as f64 / actual.len() as f64,
        outlier_fraction: outliers as f64 / total_pixels.max(1) as f64,
        max_diff,
        outlier_pixels: outliers,
        total_pixels,
    }
}

/// Reads a golden PNG back into tightly packed RGBA8.
pub fn read_png_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let image = image::open(path)
        .map_err(|e| format!("failed to decode {}: {e}", path.display()))?
        .to_rgba8();
    Ok((image.width(), image.height(), image.into_raw()))
}

/// Writes tightly packed RGBA8 as a PNG, creating parent directories.
pub fn write_png_rgba(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let buffer = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| format!("invalid RGBA buffer for {}x{}", width, height))?;
    buffer
        .save(path)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Compares one slide against its committed golden, or writes it in update mode.
///
/// Returns `true` when the render matched (or when the golden was written).
pub fn compare_or_update(rendered: &DeckRender, slide_index: usize, label: &str) -> bool {
    let path = golden_path(rendered.config.name, slide_index);
    let actual = &rendered.slides[slide_index];
    let update = update_goldens();

    if update {
        write_png_rgba(&path, rendered.width_px, rendered.height_px, &actual.pixels)
            .unwrap_or_else(|e| panic!("{e}"));
        println!("UPDATED golden {}", path.display());
        return true;
    }

    if !path.is_file() {
        panic!(
            "golden PNG missing: {}\n\
             Generate reference images with:\n    \
             UPDATE_GOLDENS=1 cargo test -p office-terminal-core --test golden_tests\n\
             then commit the new files.",
            path.display()
        );
    }

    let (width, height, expected) = read_png_rgba(&path).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (width, height),
        (rendered.width_px, rendered.height_px),
        "{label}: golden dimensions changed ({}x{} vs {}x{}); regenerate with UPDATE_GOLDENS=1",
        width,
        height,
        rendered.width_px,
        rendered.height_px
    );

    let thresholds = rendered.config.thresholds;
    let metrics = diff_metrics(&actual.pixels, &expected, thresholds.channel_tolerance);
    println!(
        "{label}: mean_abs_diff={:.4} outliers={}/{} ({:.4}%) max_diff={}",
        metrics.mean_abs_diff,
        metrics.outlier_pixels,
        metrics.total_pixels,
        metrics.outlier_fraction * 100.0,
        metrics.max_diff
    );

    let mut failures = Vec::new();
    if metrics.mean_abs_diff > thresholds.mean_abs_diff {
        failures.push(format!(
            "mean_abs_diff {:.4} > {:.4}",
            metrics.mean_abs_diff, thresholds.mean_abs_diff
        ));
    }
    if metrics.outlier_fraction > thresholds.outlier_fraction {
        failures.push(format!(
            "outlier_fraction {:.4} > {:.4} ({} pixels differ by > {}/255)",
            metrics.outlier_fraction,
            thresholds.outlier_fraction,
            metrics.outlier_pixels,
            thresholds.channel_tolerance
        ));
    }
    assert!(
        failures.is_empty(),
        "{label}: rendered pixels differ from {}: {}\n\
         If the change is intentional (renderer, font stack, goldens), \
         regenerate with UPDATE_GOLDENS=1 and review the diff.",
        path.display(),
        failures.join("; ")
    );
    true
}

/// Average RGBA of a square window centred at `(cx, cy)` (clamped to bounds).
pub fn average_window(slide: &RgbaSlide, cx: i32, cy: i32, radius: i32) -> [f64; 4] {
    let width = slide.width as i32;
    let height = slide.height as i32;
    let x0 = (cx - radius).max(0);
    let x1 = (cx + radius + 1).min(width);
    let y0 = (cy - radius).max(0);
    let y1 = (cy + radius + 1).min(height);
    assert!(x0 < x1 && y0 < y1, "sample window outside the framebuffer");

    let mut totals = [0.0f64; 4];
    let mut count = 0.0f64;
    for y in y0..y1 {
        for x in x0..x1 {
            let index = ((y * width + x) * 4) as usize;
            for (channel, total) in totals.iter_mut().enumerate() {
                *total += slide.pixels[index + channel] as f64;
            }
            count += 1.0;
        }
    }
    totals.map(|total| total / count)
}

/// Samples a fractional slide coordinate (0..=1) using a 5x5 average.
pub fn sample_fractional(slide: &RgbaSlide, fx: f64, fy: f64) -> [f64; 4] {
    let cx = (fx * slide.width as f64).round() as i32;
    let cy = (fy * slide.height as f64).round() as i32;
    average_window(slide, cx, cy, 2)
}

/// Rec. 709 relative luminance approximation for the sampled RGB average.
pub fn luminance(rgb: [f64; 4]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}
