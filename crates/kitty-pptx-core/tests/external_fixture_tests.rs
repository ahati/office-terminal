//! Optional cross-renderer comparison against pinned LibreOffice fixtures.
//!
//! The fixtures are fetched into `target/external/fixtures/<commit>/` by
//! `tests/golden/external/run.py`; the reference PNGs are rendered by
//! `office-open-xml-viewer` through Playwright into
//! `target/external/goldens/<commit>/`. Neither the fixtures nor the reference
//! images are committed.
//!
//! This is intentionally an opt-in, expensive test:
//!
//! ```sh
//! python3 crates/kitty-pptx-core/tests/golden/external/run.py
//! ```
//!
//! The normal `cargo test` run ignores it. The comparison is perceptual, not
//! pixel-identical: another renderer and another font stack are expected to
//! differ on glyphs and anti-aliasing.

use std::path::{Path, PathBuf};

use crossbeam_channel::unbounded;
use kitty_pptx_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec,
    RgbaSlide, SlideCompiler, SlideRasterizer, Source, TinySkiaRasterizer,
};
use serde::Deserialize;

const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/external/fixtures.json"
);

#[derive(Debug, Deserialize)]
struct Manifest {
    commit: String,
    render_width: u32,
    files: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    slides: usize,
    strict: bool,
    note: String,
    thresholds: Option<Thresholds>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct Thresholds {
    min_cell_pass_ratio: f64,
    max_global_mean_abs_diff: f64,
    max_ink_fraction_delta: f64,
}

impl Thresholds {
    const DEFAULT: Self = Self {
        min_cell_pass_ratio: 0.90,
        max_global_mean_abs_diff: 8.0,
        max_ink_fraction_delta: 0.03,
    };
}

struct RenderedDeck {
    slides: Vec<RgbaSlide>,
}

#[derive(Clone, Debug)]
struct RgbImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbImage {
    fn from_rgba(rgba: &RgbaSlide) -> Self {
        Self::from_raw_rgba(rgba.width, rgba.height, &rgba.pixels)
    }

    fn from_raw_rgba(width: u32, height: u32, data: &[u8]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
        for pixel in data.as_chunks::<4>().0 {
            let alpha = f64::from(pixel[3]) / 255.0;
            for channel in &pixel[..3] {
                let value = f64::from(*channel) * alpha + 255.0 * (1.0 - alpha);
                pixels.push(value.round() as u8);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn ink_fraction(&self) -> f64 {
        let ink = self
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|pixel| pixel.iter().any(|channel| *channel < 250))
            .count();
        ink as f64 / (self.width as f64 * self.height as f64)
    }
}

#[derive(Clone, Copy, Debug)]
struct DiffMetrics {
    cell_pass_ratio: f64,
    global_mean_abs_diff: f64,
    worst_cell_diff: f64,
    ink_fraction_delta: f64,
    cells_x: u32,
    cells_y: u32,
}

fn mainline_thresholds(fixture: &Fixture) -> Thresholds {
    fixture.thresholds.unwrap_or(Thresholds::DEFAULT)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate must live under <repo>/crates")
        .to_path_buf()
}

fn cache_dir(variable: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

fn render_fixture(path: &Path, width_px: u32) -> RenderedDeck {
    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path.to_path_buf()))
        .unwrap_or_else(|error| panic!("failed to load {}: {error}", path.display()));
    let height_px = (width_px as f64 * presentation.slide_height as f64
        / presentation.slide_width as f64)
        .round()
        .max(1.0) as u32;
    let target = RenderTargetSpec {
        width_px,
        height_px,
    };
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
            .unwrap_or_else(|error| {
                panic!(
                    "{} slide {} failed to compile: {error}",
                    path.display(),
                    index + 1
                )
            });
        let rendered = rasterizer
            .rasterize(&vector, &target, assets.as_ref())
            .unwrap_or_else(|error| {
                panic!(
                    "{} slide {} failed to rasterize: {error}",
                    path.display(),
                    index + 1
                )
            });
        slides.push(rendered);
    }
    RenderedDeck { slides }
}

fn resize_box(input: &RgbImage, width: u32, height: u32) -> RgbImage {
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    let scale_x = input.width as f64 / width as f64;
    let scale_y = input.height as f64 / height as f64;

    for y in 0..height {
        let y0 = (y as f64 * scale_y).floor() as u32;
        let y1 = (((y + 1) as f64 * scale_y).floor() as u32)
            .max(y0 + 1)
            .min(input.height);
        for x in 0..width {
            let x0 = (x as f64 * scale_x).floor() as u32;
            let x1 = (((x + 1) as f64 * scale_x).floor() as u32)
                .max(x0 + 1)
                .min(input.width);
            let mut sums = [0u64; 3];
            let mut count = 0u64;
            for source_y in y0..y1 {
                for source_x in x0..x1 {
                    let offset = ((source_y * input.width + source_x) * 3) as usize;
                    for (channel, sum) in sums.iter_mut().enumerate() {
                        *sum += u64::from(input.pixels[offset + channel]);
                    }
                    count += 1;
                }
            }
            let target = ((y * width + x) * 3) as usize;
            for (channel, value) in pixels[target..target + 3].iter_mut().enumerate() {
                *value = (sums[channel] / count) as u8;
            }
        }
    }
    RgbImage {
        width,
        height,
        pixels,
    }
}

fn compare(actual: &RgbImage, reference: &RgbImage, tolerance: f64) -> DiffMetrics {
    let cell = 10u32;
    let columns = actual.width.div_ceil(cell);
    let rows = actual.height.div_ceil(cell);
    let mut sum = 0.0;
    let mut cells = 0u64;
    let mut passing = 0u64;
    let mut worst: f64 = 0.0;

    for cell_y in 0..rows {
        for cell_x in 0..columns {
            let mut actual_sum = [0u64; 3];
            let mut reference_sum = [0u64; 3];
            let mut count = 0u64;
            for y in cell_y * cell..((cell_y + 1) * cell).min(actual.height) {
                for x in cell_x * cell..((cell_x + 1) * cell).min(actual.width) {
                    let offset = ((y * actual.width + x) * 3) as usize;
                    for (channel, (actual_sum, reference_sum)) in actual_sum
                        .iter_mut()
                        .zip(reference_sum.iter_mut())
                        .enumerate()
                    {
                        *actual_sum += u64::from(actual.pixels[offset + channel]);
                        *reference_sum += u64::from(reference.pixels[offset + channel]);
                    }
                    count += 1;
                }
            }
            let delta = actual_sum
                .iter()
                .zip(reference_sum.iter())
                .map(|(actual_sum, reference_sum)| {
                    let actual_mean = *actual_sum as f64 / count as f64;
                    let reference_mean = *reference_sum as f64 / count as f64;
                    (actual_mean - reference_mean).abs()
                })
                .sum::<f64>()
                / 3.0;
            sum += delta;
            cells += 1;
            if delta <= tolerance {
                passing += 1;
            }
            worst = worst.max(delta);
        }
    }

    let ink_actual = actual.ink_fraction();
    let ink_reference = reference.ink_fraction();
    DiffMetrics {
        cell_pass_ratio: passing as f64 / cells as f64,
        global_mean_abs_diff: sum / cells as f64,
        worst_cell_diff: worst,
        ink_fraction_delta: (ink_actual - ink_reference).abs(),
        cells_x: columns,
        cells_y: rows,
    }
}

fn load_reference(path: &Path) -> RgbImage {
    let image = image::open(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .to_rgba8();
    RgbImage::from_raw_rgba(image.width(), image.height(), image.as_raw())
}

#[test]
#[ignore = "expensive external renderer; run crates/kitty-pptx-core/tests/golden/external/run.py"]
fn external_libreoffice_renderer_comparison() {
    let manifest_text =
        std::fs::read_to_string(MANIFEST).expect("external fixture manifest must be readable");
    let manifest: Manifest =
        serde_json::from_str(&manifest_text).expect("external fixture manifest must parse");

    let fixtures_dir = cache_dir(
        "KITTY_PPTX_EXTERNAL_FIXTURES",
        repo_root()
            .join("target/external/fixtures")
            .join(&manifest.commit),
    );
    let goldens_dir = cache_dir(
        "KITTY_PPTX_EXTERNAL_GOLDENS",
        repo_root()
            .join("target/external/goldens")
            .join(&manifest.commit),
    );

    assert!(
        fixtures_dir.is_dir(),
        "missing external fixtures at {}; run \
         `python3 crates/kitty-pptx-core/tests/golden/external/run.py` first",
        fixtures_dir.display()
    );
    assert!(
        goldens_dir.is_dir(),
        "missing external references at {}; run \
         `python3 crates/kitty-pptx-core/tests/golden/external/run.py` first",
        goldens_dir.display()
    );

    let mut failures = Vec::new();
    for fixture in &manifest.files {
        let path = fixtures_dir.join(format!("{}.pptx", fixture.name));
        let rendered = render_fixture(&path, manifest.render_width);
        if rendered.slides.len() != fixture.slides {
            failures.push(format!(
                "{}: expected {} slide(s), rendered {}",
                fixture.name,
                fixture.slides,
                rendered.slides.len()
            ));
            continue;
        }

        let thresholds = mainline_thresholds(fixture);
        for (index, actual) in rendered.slides.iter().enumerate() {
            let reference_path = goldens_dir
                .join(&fixture.name)
                .join(format!("slide_{:03}.png", index + 1));
            let reference = load_reference(&reference_path);
            let actual = RgbImage::from_rgba(actual);
            let compare_width = 400;
            let compare_height = ((compare_width as f64 * actual.height as f64
                / actual.width as f64)
                .round() as u32)
                .max(1);
            let actual = resize_box(&actual, compare_width, compare_height);
            let reference = resize_box(&reference, compare_width, compare_height);
            let metrics = compare(&actual, &reference, 40.0);
            println!(
                "{} slide {}: cells={}x{} pass={:.3} global={:.2} worst={:.2} \
                 ink_delta={:.4} ({})",
                fixture.name,
                index + 1,
                metrics.cells_x,
                metrics.cells_y,
                metrics.cell_pass_ratio,
                metrics.global_mean_abs_diff,
                metrics.worst_cell_diff,
                metrics.ink_fraction_delta,
                fixture.note,
            );

            if !fixture.strict {
                println!("  known gap: {} (not enforced)", fixture.name);
                continue;
            }
            if metrics.cell_pass_ratio < thresholds.min_cell_pass_ratio {
                failures.push(format!(
                    "{} slide {}: {:.1}% of cells within 40/255 (need {:.1}%)",
                    fixture.name,
                    index + 1,
                    metrics.cell_pass_ratio * 100.0,
                    thresholds.min_cell_pass_ratio * 100.0
                ));
            }
            if metrics.global_mean_abs_diff > thresholds.max_global_mean_abs_diff {
                failures.push(format!(
                    "{} slide {}: global mean abs diff {:.2} > {:.2}",
                    fixture.name,
                    index + 1,
                    metrics.global_mean_abs_diff,
                    thresholds.max_global_mean_abs_diff
                ));
            }
            if metrics.ink_fraction_delta > thresholds.max_ink_fraction_delta {
                failures.push(format!(
                    "{} slide {}: ink coverage differs by {:.1}pp > {:.1}pp",
                    fixture.name,
                    index + 1,
                    metrics.ink_fraction_delta * 100.0,
                    thresholds.max_ink_fraction_delta * 100.0
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "external renderer comparison failed:\n  {}\n\
         Regenerate references with `python3 \
         crates/kitty-pptx-core/tests/golden/external/run.py --force-render` \
         and review the metric drift before changing budgets.",
        failures.join("\n  ")
    );
}
