//! Optional, self-skipping comparison against pinned LibreOffice fixtures.
//!
//! `run.py` fills `target/external/{fixtures,goldens}/<commit>/`; neither is
//! committed. Without a complete cache and matching `stamp.json` the test
//! prints a one-line skip and returns; with the cache it compares. It is
//! perceptual, not pixel-identical: another renderer and font stack differ.
//!
//!     python3 crates/kitty-pptx-core/tests/golden/external/run.py

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::unbounded;
use kitty_pptx_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec,
    RgbaSlide, SlideCompiler, SlideRasterizer, Source, TinySkiaRasterizer,
};
use serde::Deserialize;

const MANIFEST_RELATIVE: &str = "tests/golden/external/fixtures.json";
const CACHE_RELATIVE: &str = "target/external";
const RUN_COMMAND: &str = "python3 crates/kitty-pptx-core/tests/golden/external/run.py";

/// The committed manifest is the single source of truth for the corpus.
#[derive(Deserialize)]
struct Manifest {
    upstream: String,
    commit: String,
    license: String,
    render_width: u32,
    files: Vec<Fixture>,
}

#[derive(Deserialize)]
struct Fixture {
    name: String,
    path: String,
    sha256: String,
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

/// The subset of `stamp.json` that proves the cached pixels match the manifest.
#[derive(Deserialize)]
struct Stamp {
    commit: String,
    render_width: u32,
    fixtures: Vec<StampFixture>,
}

#[derive(Deserialize)]
struct StampFixture {
    name: String,
    sha256: String,
    slides: usize,
}

/// A discovered, complete external corpus. This is the only place that reads
/// the manifest and cache layout, so adding a fixture needs no test edit.
struct ExternalCorpus {
    manifest: Manifest,
    fixtures_dir: PathBuf,
    goldens_dir: PathBuf,
}

impl ExternalCorpus {
    fn discover() -> Result<Self, String> {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST_RELATIVE);
        let manifest = load_manifest(&manifest_path);
        let commit = manifest.commit.clone();
        let dir = |variable: &str, kind: &str| {
            cache_dir(
                variable,
                repo_root().join(CACHE_RELATIVE).join(kind).join(&commit),
            )
        };
        let fixtures_dir = dir("KITTY_PPTX_EXTERNAL_FIXTURES", "fixtures");
        let goldens_dir = dir("KITTY_PPTX_EXTERNAL_GOLDENS", "goldens");

        for fixture in &manifest.files {
            let path = fixtures_dir.join(format!("{}.pptx", fixture.name));
            if !path.is_file() {
                return Err(unavailable(format!(
                    "fixture cache is incomplete: {} is missing",
                    path.display()
                )));
            }
        }

        let stamp = read_stamp(&goldens_dir)?;
        check_stamp(&stamp, &manifest, &goldens_dir)?;

        for fixture in &manifest.files {
            for slide in 1..=fixture.slides {
                let png = goldens_dir
                    .join(&fixture.name)
                    .join(format!("slide_{slide:03}.png"));
                if png
                    .metadata()
                    .map(|metadata| metadata.len() == 0)
                    .unwrap_or(true)
                {
                    return Err(unavailable(format!(
                        "reference cache is incomplete: {} is missing or empty",
                        png.display()
                    )));
                }
            }
        }

        Ok(Self {
            manifest,
            fixtures_dir,
            goldens_dir,
        })
    }
}

fn load_manifest(path: &Path) -> Manifest {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn unavailable(reason: String) -> String {
    format!("{reason}; run `{RUN_COMMAND}`")
}

fn read_stamp(goldens_dir: &Path) -> Result<Stamp, String> {
    let path = goldens_dir.join("stamp.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| unavailable(format!("reference cache is missing: {}", path.display())))?;
    serde_json::from_str(&text).map_err(|error| {
        unavailable(format!(
            "{} is not a valid render stamp ({error})",
            path.display()
        ))
    })
}

/// Requires the stamp's commit, width, and fixture identities to match.
fn check_stamp(stamp: &Stamp, manifest: &Manifest, goldens_dir: &Path) -> Result<(), String> {
    if stamp.commit != manifest.commit || stamp.render_width != manifest.render_width {
        return Err(unavailable(format!(
            "references are {} at {}px, fixtures.json pins {} at {}px",
            stamp.commit, stamp.render_width, manifest.commit, manifest.render_width
        )));
    }
    let matches = |fixture: &Fixture| {
        stamp.fixtures.iter().any(|entry| {
            entry.name == fixture.name
                && entry.sha256 == fixture.sha256
                && entry.slides == fixture.slides
        })
    };
    if stamp.fixtures.len() != manifest.files.len() || !manifest.files.iter().all(matches) {
        return Err(unavailable(format!(
            "references in {} do not match fixtures.json",
            goldens_dir.display()
        )));
    }
    Ok(())
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
        let pixels = data
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| {
                let alpha = f64::from(pixel[3]) / 255.0;
                pixel[..3].iter().map(move |channel| {
                    (f64::from(*channel) * alpha + 255.0 * (1.0 - alpha)).round() as u8
                })
            })
            .collect();
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

fn thresholds_for(fixture: &Fixture) -> Thresholds {
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

fn render_fixture(path: &Path, width_px: u32) -> Vec<RgbaSlide> {
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
            .unwrap_or_else(|error| panic!("{} slide {}: {error}", path.display(), index + 1));
        let rendered = rasterizer
            .rasterize(&vector, &target, assets.as_ref())
            .unwrap_or_else(|error| panic!("{} slide {}: {error}", path.display(), index + 1));
        slides.push(rendered);
    }
    slides
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
                .map(|(a, r)| (*a as f64 / count as f64 - *r as f64 / count as f64).abs())
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

/// Renders every manifest fixture and compares it against the cached references.
fn compare_corpus(corpus: &ExternalCorpus) {
    let mut failures = Vec::new();
    for fixture in &corpus.manifest.files {
        let path = corpus.fixtures_dir.join(format!("{}.pptx", fixture.name));
        let rendered = render_fixture(&path, corpus.manifest.render_width);
        if rendered.len() != fixture.slides {
            failures.push(format!(
                "{}: expected {} slide(s), rendered {}",
                fixture.name,
                fixture.slides,
                rendered.len()
            ));
            continue;
        }

        let thresholds = thresholds_for(fixture);
        for (index, actual) in rendered.iter().enumerate() {
            let reference_path = corpus
                .goldens_dir
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
         Regenerate references with `{RUN_COMMAND} --force-render` \
         and review the metric drift before changing budgets.",
        failures.join("\n  ")
    );
}

fn run_external_comparison() {
    // `--include-ignored` selects both entry points; render once per process.
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        println!("external corpus comparison already ran; skipping duplicate");
        return;
    }
    match ExternalCorpus::discover() {
        Ok(corpus) => compare_corpus(&corpus),
        Err(reason) => println!("skipping external corpus comparison: {reason}"),
    }
}

#[test]
fn external_libreoffice_renderer_comparison() {
    run_external_comparison();
}

/// Alias for `run.py`'s `--ignored` filter; drop it when `run.py` passes
/// `--include-ignored`.
#[test]
#[ignore = "alias for run.py's --ignored filter; the main test is self-skipping"]
fn external_libreoffice_renderer_comparison_via_run_py() {
    run_external_comparison();
}

#[test]
fn manifest_is_internally_consistent() {
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST_RELATIVE));

    assert!(
        manifest.upstream.starts_with("https://"),
        "upstream must be https"
    );
    assert!(
        !manifest.license.trim().is_empty(),
        "license must be recorded"
    );
    assert!(
        is_lower_hex(&manifest.commit, 40),
        "commit must be 40 lowercase hex characters"
    );
    assert!(
        (1..=10_000).contains(&manifest.render_width),
        "render_width out of range"
    );
    assert!(
        !manifest.files.is_empty(),
        "manifest must list at least one fixture"
    );

    let mut names = BTreeSet::new();
    for fixture in &manifest.files {
        assert!(
            names.insert(fixture.name.as_str()),
            "duplicate fixture name {:?}",
            fixture.name
        );
        assert_fixture_consistent(fixture);
    }
    assert!(
        manifest.files.iter().any(|fixture| fixture.strict),
        "at least one fixture must be strict so the comparison can fail"
    );
}

fn assert_fixture_consistent(fixture: &Fixture) {
    let name = &fixture.name;
    assert!(is_safe_name(name), "unsafe fixture name {name:?}");
    assert!(
        is_lower_hex(&fixture.sha256, 64),
        "{name}: sha256 must be 64 lowercase hex characters"
    );
    assert!(fixture.slides > 0, "{name}: slide count must be > 0");
    assert!(
        fixture.path.starts_with("sd/qa/unit/data/pptx/"),
        "{name}: path must be LibreOffice pptx QA data"
    );
    assert!(
        fixture.path.ends_with(".pptx") && !fixture.path.contains(".."),
        "{name}: invalid path {:?}",
        fixture.path
    );
    assert!(
        !fixture.note.trim().is_empty(),
        "{name}: note must describe what the fixture covers"
    );
    let thresholds = thresholds_for(fixture);
    assert!(
        thresholds.min_cell_pass_ratio > 0.0
            && thresholds.min_cell_pass_ratio <= 1.0
            && thresholds.max_global_mean_abs_diff > 0.0
            && thresholds.max_global_mean_abs_diff <= 255.0
            && thresholds.max_ink_fraction_delta > 0.0
            && thresholds.max_ink_fraction_delta <= 1.0,
        "{name}: thresholds {thresholds:?} out of range"
    );
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
