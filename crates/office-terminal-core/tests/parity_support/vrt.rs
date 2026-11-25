//! Visual-regression driver for the committed synthetic deck.
//!
//! The harness follows the shape of the upstream reference viewer's
//! `visual.spec.ts` driver: load the committed `.pptx` through the production
//! seam (`DefaultPresentationLoader` → `DefaultSlideCompiler::compile_in_deck`
//! → `TinySkiaRasterizer`), rasterize every slide at the deck's fixed width,
//! then pad both render and reference to `max(w) × max(h)` on white and
//! pixelmatch with `{ threshold: 0.20, includeAA: true }`.
//!
//! The reference PNGs are **locked outputs of this renderer**, not an
//! independent fidelity oracle: `synthetic-1.pptx` is generated with generic
//! shapes/text and contains no third-party media. The test is therefore a
//! change detector against an approved rendering, while the ratchet protects
//! against silent visual regressions.
//!
//! A slide fails when `diff_pct > 20`, has no recorded score, or drops more
//! than 0.5 pt below its recorded score. `UPDATE_REFS=1` adopts the current
//! render as the reference (and stops before comparing); `UPDATE_SCORES=1`
//! re-records scores against the untouched PNGs.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crossbeam_channel::unbounded;
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec, RgbaSlide, SlideCompiler,
    SlideRasterizer, Source, TinySkiaRasterizer,
};

use super::pixelmatch::{compare_rgba, pad_to_white, DiffOptions};
use super::scores::{self, Scores};

/// Per-pixel colour tolerance. ref `visual.spec.ts:39` (`PIXEL_THRESHOLD`).
pub const PIXEL_THRESHOLD: f64 = 0.20;

/// Coarse fidelity ceiling; fails even when the ratchet is disabled.
/// ref `visual.spec.ts:42` (`FAIL_ABOVE_PCT = 20`).
pub const FAIL_ABOVE_DIFF_PCT: f64 = 20.0;

/// One committed synthetic regression deck. ref `visual.spec.ts:33-35`
/// (`{ name, slideCount }`).
#[derive(Clone, Copy, Debug)]
pub struct DeckSpec {
    /// Deck stem: `.pptx`, reference directory and scores file stem.
    pub name: &'static str,
    /// Exact declared slide count; a mismatch fails (`visual.spec.ts:111-117`).
    pub slide_count: usize,
    /// Fixed render width in CSS pixels; height follows the deck aspect.
    pub width_px: u32,
}

impl DeckSpec {
    /// The committed synthetic `.pptx` under `tests/fixtures/parity/`.
    pub fn fixture(&self) -> PathBuf {
        fixtures_dir().join(format!("{}.pptx", self.name))
    }

    /// Directory holding this deck's adopted reference PNGs.
    pub fn reference_dir(&self) -> PathBuf {
        fixtures_dir().join("reference").join(self.name)
    }

    /// Reference PNG for a 1-based slide number.
    pub fn reference_png(&self, slide: usize) -> PathBuf {
        self.reference_dir().join(format!("slide-{slide}.png"))
    }

    /// Committed ratchet baseline, `tests/parity/scores/<name>.json`
    /// (crate-relative, unlike the reference repo's `references/<name>/`).
    pub fn scores_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("parity")
            .join("scores")
            .join(format!("{}.json", self.name))
    }
}

/// Regression corpora; keep each declared `slide_count` exact.
pub const DECKS: &[DeckSpec] = &[DeckSpec {
    name: "synthetic-1",
    slide_count: 3,
    width_px: 1280,
}];

/// Looks up a deck by stem; panics with the known names when missing so a
/// typo cannot silently disable coverage.
pub fn deck(name: &str) -> &'static DeckSpec {
    DECKS.iter().find(|spec| spec.name == name).unwrap_or_else(|| {
        let names: Vec<&str> = DECKS.iter().map(|spec| spec.name).collect();
        panic!("no fidelity deck named {name}; DECKS contains {names:?}")
    })
}

/// `tests/fixtures/parity`, resolved against the crate root so the driver
/// works no matter which directory `cargo test` is invoked from.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("parity")
}

/// How this run treats the committed oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Compare, enforce the coarse ceiling, and apply the score ratchet.
    Check,
    /// Adopt the current render as the reference PNG (`UPDATE_REFS=1`).
    UpdateRefs,
    /// Re-record scores against the untouched PNGs (`UPDATE_SCORES=1`).
    UpdateScores,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::UpdateRefs => "UPDATE_REFS",
            Self::UpdateScores => "UPDATE_SCORES",
        }
    }
}

/// Reads the mode flags. The two update modes are mutually exclusive: running
/// both would simultaneously rewrite the oracle and bless the render against
/// it, which is never a meaningful operation.
pub fn mode() -> Mode {
    match (env_flag("UPDATE_REFS"), env_flag("UPDATE_SCORES")) {
        (true, true) => panic!(
            "UPDATE_REFS=1 and UPDATE_SCORES=1 are mutually exclusive; \
             adopt references first, then re-record scores in a second run"
        ),
        (true, false) => Mode::UpdateRefs,
        (false, true) => Mode::UpdateScores,
        (false, false) => Mode::Check,
    }
}

/// Only the literal `1` enables a mode, matching `process.env.X === '1'`
/// (ref `visual.spec.ts:51,56`).
fn env_flag(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("1"))
}

/// All slides of one deck, rasterized at a fixed target.
struct RenderedDeck {
    width: u32,
    height: u32,
    slides: Vec<RgbaSlide>,
}

type CacheKey = (PathBuf, u32);
type Cache = Mutex<HashMap<CacheKey, Arc<RenderedDeck>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Loads, compiles and rasterizes one slide through the production seam.
///
/// `index` is 0-based; `width` is the render width and the height follows the
/// deck aspect (1280×720 here). Panics with context on any pipeline error or an
/// out-of-range index. Reference equivalent: `fixture.html` plus
/// `canvas.toDataURL()` for one `?slide=` (`visual.spec.ts:92-129`).
pub fn render_slide_rgba(path: &Path, index: usize, width: u32) -> RgbaSlide {
    let deck = render_deck_cached(path, width);
    deck.slides.get(index).cloned().unwrap_or_else(|| {
        panic!(
            "{} has no slide at index {index}; the renderer produced {} slide(s)",
            path.display(),
            deck.slides.len()
        )
    })
}

/// Renders the whole deck once per test binary and clones the cached slide.
fn render_deck_cached(path: &Path, width: u32) -> Arc<RenderedDeck> {
    let key = (path.to_path_buf(), width);
    let mut cache = cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.get(&key) {
        return Arc::clone(cached);
    }
    let rendered = Arc::new(try_render_deck(path, width).unwrap_or_else(|e| panic!("{e}")));
    cache.insert(key, Arc::clone(&rendered));
    rendered
}

fn try_render_deck(path: &Path, width: u32) -> Result<RenderedDeck, String> {
    if width == 0 {
        return Err(format!(
            "cannot render {} at width 0; the fidelity driver needs a positive width",
            path.display()
        ));
    }
    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path.to_path_buf()))
        .map_err(|e| format!("cannot load fidelity deck {}: {e}", path.display()))?;
    if presentation.slide_width <= 0 || presentation.slide_height <= 0 {
        return Err(format!(
            "fidelity deck {} declares an invalid slide size {}x{} EMU",
            path.display(),
            presentation.slide_width,
            presentation.slide_height
        ));
    }
    let height = (width as f64 * presentation.slide_height as f64 / presentation.slide_width as f64)
        .round()
        .max(1.0) as u32;
    let target = RenderTargetSpec {
        width_px: width,
        height_px: height,
    };

    let compiler = DefaultSlideCompiler::new();
    let rasterizer = TinySkiaRasterizer::new();
    let (warning_tx, _warning_rx) = unbounded();

    let mut slides = Vec::with_capacity(presentation.slides.len());
    for (index, slide) in presentation.slides.iter().enumerate() {
        let number = index + 1;
        let vector = compiler
            .compile_in_deck(
                slide,
                &presentation.slides,
                presentation.slide_width,
                presentation.slide_height,
                &warning_tx,
            )
            .map_err(|e| format!("{} slide {number} failed to compile: {e}", path.display()))?;
        let rendered = rasterizer
            .rasterize(&vector, &target, assets.as_ref())
            .map_err(|e| format!("{} slide {number} failed to rasterize: {e}", path.display()))?;
        slides.push(rendered);
    }

    Ok(RenderedDeck { width, height, slides })
}

/// Outcome of one slide comparison.
#[derive(Clone, Debug)]
pub struct SlideReport {
    /// 1-based slide number, matching the reference `slide-N.png` name.
    pub slide: usize,
    pub actual_width: u32,
    pub actual_height: u32,
    /// Reference image dimensions when one was read.
    pub reference_size: Option<(u32, u32)>,
    pub diff_pixels: usize,
    pub total_pixels: usize,
    pub diff_pct: f64,
    pub match_pct: f64,
    pub mean_abs_diff: f64,
    /// Committed score before this run (`None` = missing, or `UPDATE_REFS`).
    pub prior: Option<f64>,
    pub reference_written: bool,
    pub score_written: bool,
    /// Every threshold/ratchet failure this slide contributes.
    pub failures: Vec<String>,
}

impl SlideReport {
    /// Score key used in `scores.json` (`slide-N`, ref `visual.spec.ts:204`).
    pub fn key(&self) -> String {
        format!("slide-{}", self.slide)
    }

    /// True when the slide met every applicable threshold.
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }

    /// Reference dimensions when they differ; the harness only logs this and
    /// white padding absorbs it (`visual.spec.ts:147-156`).
    pub fn size_mismatch(&self) -> Option<(u32, u32)> {
        let (rw, rh) = self.reference_size?;
        (rw != self.actual_width || rh != self.actual_height).then_some((rw, rh))
    }
}

/// Outcome of one deck run.
#[derive(Clone, Debug)]
pub struct DeckReport {
    pub name: &'static str,
    pub mode: Mode,
    pub declared_slides: usize,
    pub rendered_slides: usize,
    pub width: u32,
    pub height: u32,
    pub slides: Vec<SlideReport>,
    pub failures: Vec<String>,
}

impl DeckReport {
    /// Every failure the test should assert on (`visual.spec.ts:111-117,193-215`).
    pub fn violations(&self) -> &[String] {
        &self.failures
    }
}

/// Renders, compares and (in the update modes) writes every slide of `spec`.
///
/// `UPDATE_REFS` replaces the synthetic reference and returns before
/// comparing. Otherwise every rendered slide is compared; missing references
/// or unreadable score files are hard errors (broken corpus, not a visual
/// regression).
pub fn run_deck(spec: &DeckSpec) -> DeckReport {
    let mode = mode();
    let deck = render_deck_cached(&spec.fixture(), spec.width_px);

    let mut failures = Vec::new();
    if deck.slides.len() != spec.slide_count {
        failures.push(format!(
            "{}: the renderer produced {} slide(s) but DECKS declares {}; \
             keep the declared count exact so no slide goes unchecked",
            spec.name,
            deck.slides.len(),
            spec.slide_count
        ));
    }

    let mut slides = Vec::with_capacity(deck.slides.len());
    for (index, rendered) in deck.slides.iter().enumerate() {
        let report = run_slide(spec, index + 1, rendered, mode);
        failures.extend(report.failures.iter().cloned());
        slides.push(report);
    }

    DeckReport {
        name: spec.name,
        mode,
        declared_slides: spec.slide_count,
        rendered_slides: deck.slides.len(),
        width: deck.width,
        height: deck.height,
        slides,
        failures,
    }
}

fn run_slide(spec: &DeckSpec, number: usize, rendered: &RgbaSlide, mode: Mode) -> SlideReport {
    if mode == Mode::UpdateRefs {
        let path = spec.reference_png(number);
        write_reference(&path, rendered).unwrap_or_else(|e| panic!("{e}"));
        return SlideReport {
            slide: number,
            actual_width: rendered.width,
            actual_height: rendered.height,
            reference_size: Some((rendered.width, rendered.height)),
            diff_pixels: 0,
            total_pixels: (rendered.width as usize) * (rendered.height as usize),
            diff_pct: 0.0,
            match_pct: 100.0,
            mean_abs_diff: 0.0,
            prior: None,
            reference_written: true,
            score_written: false,
            failures: Vec::new(),
        };
    }

    let reference = spec.reference_png(number);
    let (ref_w, ref_h, ref_pixels) = read_reference(&reference).unwrap_or_else(|e| panic!("{e}"));

    // ref `visual.spec.ts:159-168`: pad both images to max(w) × max(h),
    // filling the new area with white.
    let width = rendered.width.max(ref_w);
    let height = rendered.height.max(ref_h);
    let ref_padded = pad_to_white(&ref_pixels, ref_w, ref_h, width, height);
    let actual_padded = pad_to_white(&rendered.pixels, rendered.width, rendered.height, width, height);
    let options = DiffOptions {
        threshold: PIXEL_THRESHOLD,
        include_aa: true,
    };
    // ref `visual.spec.ts:171-177`: pixelmatch(refPadded, actualPadded, …).
    let diff = compare_rgba(&ref_padded, &actual_padded, width, height, &options);

    let key = format!("slide-{number}");
    let scores_path = spec.scores_path();
    let prior = load_scores(&scores_path).get(&key).copied();
    let score_written = mode == Mode::UpdateScores;
    if score_written {
        scores::record(&scores_path, &key, diff.match_pct).unwrap_or_else(|e| panic!("{e}"));
    }

    let mut failures = Vec::new();
    if diff.diff_pct > FAIL_ABOVE_DIFF_PCT {
        failures.push(format!(
            "{} slide {number} pixel diff {:.1}% exceeds {FAIL_ABOVE_DIFF_PCT}%",
            spec.name, diff.diff_pct
        ));
    }
    if mode == Mode::Check {
        if let Err(e) = scores::ratchet(&key, diff.match_pct, prior) {
            failures.push(format!("{} {e}", spec.name));
        }
    }

    SlideReport {
        slide: number,
        actual_width: rendered.width,
        actual_height: rendered.height,
        reference_size: Some((ref_w, ref_h)),
        diff_pixels: diff.diff_pixels,
        total_pixels: diff.total_pixels,
        diff_pct: diff.diff_pct,
        match_pct: diff.match_pct,
        mean_abs_diff: diff.mean_abs_diff,
        prior,
        reference_written: false,
        score_written,
        failures,
    }
}

fn load_scores(path: &Path) -> Scores {
    scores::load(path).unwrap_or_else(|e| panic!("{e}"))
}

/// Fixed-width report table, one row per slide, plus the deck header. Printed
/// under `--nocapture`; the reference logs the same fields per slide
/// (`visual.spec.ts:187-192`).
pub fn format_table(report: &DeckReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}: {}x{}, {} slide(s) rendered (declared {}), mode={}\n",
        report.name,
        report.width,
        report.height,
        report.rendered_slides,
        report.declared_slides,
        report.mode.label()
    ));
    out.push_str(&format!(
        "{:>5}  {:>11}  {:>11}  {:>17}  {:>7}  {:>7}  {:>7}  {:>7}  {}\n",
        "slide", "render", "reference", "diff px", "diff%", "match%", "prior", "mae", "status"
    ));
    for slide in &report.slides {
        let reference = slide
            .reference_size
            .map_or_else(|| "missing".to_string(), |(w, h)| format!("{w}x{h}"));
        let prior = slide.prior.map_or_else(|| "-".to_string(), |p| format!("{p:.2}"));
        let status = if slide.reference_written {
            "reference updated".to_string()
        } else if slide.score_written {
            format!("score -> {:.2}", scores::round2(slide.match_pct))
        } else if slide.ok() {
            "ok".to_string()
        } else {
            slide.failures.join("; ")
        };
        out.push_str(&format!(
            "{:>5}  {:>11}  {:>11}  {:>17}  {:>7.2}  {:>7.2}  {:>7}  {:>7.4}  {status}\n",
            slide.slide,
            format!("{}x{}", slide.actual_width, slide.actual_height),
            reference,
            format!("{}/{}", slide.diff_pixels, slide.total_pixels),
            slide.diff_pct,
            slide.match_pct,
            prior,
            slide.mean_abs_diff,
        ));
    }
    out
}

fn read_reference(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    if !path.is_file() {
        return Err(format!(
            "missing fidelity reference {}; run UPDATE_REFS=1 to adopt the current render",
            path.display()
        ));
    }
    let image = image::open(path)
        .map_err(|e| format!("cannot decode {}: {e}", path.display()))?
        .to_rgba8();
    let (width, height) = (image.width(), image.height());
    Ok((width, height, image.into_raw()))
}

/// Writes the current render as the new reference PNG, creating directories.
/// ref `visual.spec.ts:130-136`.
fn write_reference(path: &Path, rendered: &RgbaSlide) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create reference dir {}: {e}", parent.display()))?;
    }
    let buffer = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels.to_vec())
        .ok_or_else(|| format!("slide buffer is not {}x{} RGBA", rendered.width, rendered.height))?;
    buffer
        .save(path)
        .map_err(|e| format!("cannot write fidelity reference {}: {e}", path.display()))
}
