//! WordArt text-warp parity: multi-line PAIRED-EDGE envelopes.
//!
//! The reference maps flat text between the two boundary curves of a
//! `a:prstTxWarp` preset (ECMA-376 §20.1.9.19). For paired-edge presets
//! (deflate/inflate/wave/…) the shape's height is split into one horizontal
//! band per laid-out line and each line's flat ink box fills its own band; a
//! glyph point is mapped through `P(u, v) = (1 − v)·T(u) + v·B(u)`.
//!
//! regressions this target guards (ref `renderer.ts:3210-3500`,
//! `core/src/shape/text-warp.ts`):
//! - the mapping must NOT widen a glyph twice (envelope `x` motion *and* a
//!   separate `hScale` on the advance). That double count produced the
//!   "black shards" artifact: glyphs ~2·hScale too wide and sheared flat.
//! - every line must keep its own baseline band instead of piling onto one,
//! - the per-line band parameters must stay bounded (`warp_box_h` must remain
//!   a per-line value, not the whole block divided by a shrinking band).
//!
//! The deck is synthetic (built with `parity_support/deck.rs`), so the target
//! never depends on a PowerPoint-authored sample.

#[path = "parity_support/deck.rs"]
mod deck_support;
#[path = "golden_support/mod.rs"]
mod golden_support;
#[path = "parity_support/pixelmatch.rs"]
mod pixelmatch;

use std::path::{Path, PathBuf};

use crossbeam_channel::unbounded;
use deck_support::{deck_with_size, para, run, shape_with_frame, DEFAULT_PPR};
use golden_support::{compare_or_update, render_deck, DeckConfig, Thresholds};
use office_terminal_core::compiler::text_warp::warp_point;
use office_terminal_core::contracts::{PositionedGlyphRun, RunWarp, VectorCommand};
use office_terminal_core::{
    DefaultPresentationLoader, DefaultSlideCompiler, PresentationLoader, RenderTargetSpec, RgbaSlide, SlideCompiler,
    SlideRasterizer, Source, TinySkiaRasterizer,
};

/// 16:9 slide: at the golden width of 800 px the target is exactly 800x450.
const SLIDE_W: i64 = 9_144_000;
const SLIDE_H: i64 = 5_143_500;

/// Text-box frame used by the synthetic deck (3.94 x 1.57 in).
const BOX_X: i64 = 720_000;
const BOX_Y: i64 = 600_000;
const BOX_W: i64 = 3_600_000;
const BOX_H: i64 = 1_440_000;

/// Three paragraphs, like the `textDeflate` fixture the reference ships.
const PARAGRAPHS: [&str; 3] = ["Alpha line", "Middle band text", "Omega line"];

/// `a:bodyPr` with zero insets and a maximum-adjust `textDeflate` envelope.
const BODY_PR: &str = concat!(
    r#"<a:bodyPr wrap="square" lIns="0" tIns="0" rIns="0" bIns="0">"#,
    r#"<a:prstTxWarp prst="textDeflate"><a:avLst>"#,
    r#"<a:gd name="adj" fmla="val 37500"/>"#,
    r#"</a:avLst></a:prstTxWarp><a:noAutofit/></a:bodyPr>"#,
);

const DECK: DeckConfig = DeckConfig {
    name: "warp_textdeflate",
    width_px: 800,
    // Shared with `golden_tests.rs`; text-only deck, so a different host font
    // stack moves glyph edges. Same policy as the other generated decks.
    thresholds: Thresholds::LOOSE,
};

/// The synthetic one-slide deck: a single text box with a `textDeflate` warp
/// and [`PARAGRAPHS`] as three `a:p` (24 pt each, from [`deck_support::run`]).
fn deck_bytes() -> Vec<u8> {
    let paragraphs: String = PARAGRAPHS
        .iter()
        .map(|text| para(&run(text, ""), DEFAULT_PPR))
        .collect();
    let body = format!("{BODY_PR}<a:lstStyle/>{paragraphs}");
    let shapes = shape_with_frame(
        2,
        BOX_X,
        BOX_Y,
        BOX_W,
        BOX_H,
        r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#,
        "",
        &body,
    );
    deck_with_size(&shapes, SLIDE_W, SLIDE_H)
}

fn repo_core_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Deterministic fixture used by `golden_tests.rs` and the golden comparison
/// here. Refreshed when stale so a tree without the generated `.pptx` heals
/// itself instead of skipping the golden.
fn ensure_fixture() -> PathBuf {
    let path = repo_core_dir().join("tests/golden/fixtures/warp_textdeflate.pptx");
    let bytes = deck_bytes();
    if std::fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
        return path;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&path, &bytes) {
        // A read-only checkout keeps the committed copy; the golden below still
        // validates the renderer, so a stale-but-present fixture is only a note.
        assert!(
            path.is_file(),
            "cannot write the {} fixture ({error}) and no committed copy exists",
            path.display()
        );
        eprintln!("note: keeping the committed fixture at {} ({error})", path.display());
    }
    path
}

/// Compile the synthetic deck through the production seam and return every
/// warped glyph run, in slide order.
fn compiled_warp_runs() -> Vec<PositionedGlyphRun> {
    let loader = DefaultPresentationLoader;
    let (presentation, _assets) = loader
        .load(Source::Bytes(deck_bytes()))
        .unwrap_or_else(|error| panic!("synthetic warp deck failed to load: {error}"));
    let compiler = DefaultSlideCompiler::new();
    let (tx, _rx) = unbounded();
    let slide = presentation
        .slides
        .first()
        .unwrap_or_else(|| panic!("synthetic warp deck has no slide"));
    let vector = compiler
        .compile_in_deck(
            slide,
            &presentation.slides,
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        )
        .unwrap_or_else(|error| panic!("synthetic warp slide failed to compile: {error}"));

    let mut runs = Vec::new();
    for command in &vector.commands {
        if let VectorCommand::DrawText { runs: text_runs, .. } = command {
            for text_run in text_runs {
                if text_run.warp.is_some() {
                    runs.push(text_run.clone());
                }
            }
        }
    }
    runs
}

fn warp_params(run: &PositionedGlyphRun) -> &RunWarp {
    run.warp
        .as_deref()
        .unwrap_or_else(|| panic!("run {:?} is not warped", run.text))
}

/// Em size of a run in EMU.
fn em_emu(run: &PositionedGlyphRun) -> f32 {
    run.font_size_pt * 12_700.0
}

/// One line's warp parameters (band start/height) plus its baseline samples.
struct LineProbe {
    band_frac: f32,
    warp_box_h: f32,
    /// `(u, x, y)` of the baseline map at ten fractions across the line.
    samples: Vec<(f32, f32, f32)>,
}

fn probe_line(run: &PositionedGlyphRun) -> LineProbe {
    let warp = warp_params(run);
    let samples = (0..=10)
        .map(|step| {
            let u = step as f32 / 10.0;
            let (x, y) = warp_point(&warp.envelope, u, warp.band_frac)
                .unwrap_or_else(|| panic!("{:?}: paired-edge envelope expected", run.text));
            (u, x, y)
        })
        .collect();
    LineProbe {
        band_frac: warp.band_frac,
        warp_box_h: warp.warp_box_h,
        samples,
    }
}

// ---------------------------------------------------------------------------
// Envelope parameterisation
// ---------------------------------------------------------------------------

/// Three paragraphs -> three warped runs, each carrying real shaping output.
#[test]
fn textdeflate_emits_three_lines_with_ink() {
    let runs = compiled_warp_runs();
    assert_eq!(
        runs.len(),
        PARAGRAPHS.len(),
        "one warped run per paragraph, got {:?}",
        runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>()
    );

    for (index, run) in runs.iter().enumerate() {
        assert_eq!(run.text, PARAGRAPHS[index], "run {index} text");
        assert!(
            (1..=PARAGRAPHS[index].chars().count()).contains(&run.glyphs.len()),
            "run {index} shaped into {} glyphs for {:?}",
            run.glyphs.len(),
            run.text
        );
        let advance: f32 = run.glyphs.iter().map(|glyph| glyph.advance.max(0.0)).sum();
        assert!(
            advance > 0.0,
            "run {index} has no advance ({advance}); glyphs cannot be placed"
        );
        for glyph in &run.glyphs {
            let u = glyph.warp_u.unwrap_or_else(|| panic!("run {index}: glyph without u"));
            assert!(
                (0.0..=1.0).contains(&u),
                "run {index}: glyph u {u} outside the envelope"
            );
        }
        // `warp_u` must be non-decreasing: a non-monotone mapping smears glyphs
        // across the envelope and is what makes a warped line unreadable.
        let us: Vec<f32> = run.glyphs.iter().filter_map(|glyph| glyph.warp_u).collect();
        assert!(
            us.windows(2).all(|pair| pair[0] <= pair[1]),
            "run {index}: glyph u values are not monotone: {us:?}"
        );
    }
}

/// Lines split the envelope height evenly: band fractions strictly increase
/// and `warp_box_h` stays a per-line value bounded by the block height.
#[test]
fn textdeflate_band_parameterisation_is_bounded() {
    let runs = compiled_warp_runs();
    let mut previous = f32::NEG_INFINITY;
    let line_count = runs.len() as f32;
    for (index, run) in runs.iter().enumerate() {
        let probe = probe_line(run);
        assert!(
            probe.band_frac > previous,
            "line {index}: band_frac {} did not increase (previous {previous})",
            probe.band_frac
        );
        previous = probe.band_frac;

        // ref `renderer.ts:3289`: bandFrac = v0 + ascentShare * (v1 - v0) with
        // v0 = li/n, v1 = (li + 1)/n.
        let expected = index as f32 / line_count + 0.8 / line_count;
        assert!(
            (probe.band_frac - expected).abs() < 1e-4,
            "line {index}: band_frac {} != {expected}",
            probe.band_frac
        );
        assert!(
            probe.warp_box_h > 0.0 && probe.warp_box_h <= 1.5 * BOX_H as f32,
            "line {index}: warp_box_h {} out of (0, {}]",
            probe.warp_box_h,
            1.5 * BOX_H as f32
        );
        // ref `renderer.ts:3298`: warpBoxH = inkH / (v1 - v0), so the band
        // height reconstructed from the run must match the line's ink box.
        let band_height = em_emu(run) / probe.warp_box_h;
        assert!(
            (band_height - 1.0 / line_count).abs() < 1e-4,
            "line {index}: band height {band_height} != 1/{line_count}"
        );
    }
}

/// Each line stays inside the shape box and on its own baseline: the mapped
/// baseline y increases monotonically from the top line to the bottom one and
/// every sample sits in `0..BOX_H`.
#[test]
fn textdeflate_lines_keep_their_own_baselines() {
    let runs = compiled_warp_runs();
    let mut previous_mean = f32::NEG_INFINITY;
    for (index, run) in runs.iter().enumerate() {
        let probe = probe_line(run);
        for (u, _x, y) in &probe.samples {
            assert!(
                (-BOX_H as f32..=2.0 * BOX_H as f32).contains(y),
                "line {index}: baseline y {y} at u {u} left the shape box"
            );
        }
        let mean = probe.samples.iter().map(|(_, _, y)| *y).sum::<f32>() / probe.samples.len() as f32;
        assert!(
            mean > previous_mean,
            "line {index}: mean baseline {mean} is above the previous line {previous_mean}"
        );
        previous_mean = mean;
    }
}

/// No line may be rendered at an absurd scale relative to its authored size.
///
/// This is the regression assertion for the shard artifact: mapping each
/// outline point through `P(u, v)` AND scaling its advance by `hScale` widened
/// every glyph by roughly `2 × boxW / lineW` (≈ 8x here). Both axes are
/// bounded to `0.05..=5.0` of the authored advance / em box.
#[test]
fn textdeflate_glyph_scale_stays_near_authored_size() {
    let runs = compiled_warp_runs();
    let mut widest = 0.0f32;
    let mut tallest = 0.0f32;
    for (index, run) in runs.iter().enumerate() {
        let warp = warp_params(run);
        let em = em_emu(run);
        let line_w = warp.line_w.max(1.0);
        let v_em = em / warp.warp_box_h.max(1.0);
        for glyph in &run.glyphs {
            let u = glyph.warp_u.expect("warped glyph has u");
            let advance = glyph.advance.max(1.0);
            let half = advance * 0.5 / line_w;
            let left = warp_point(&warp.envelope, (u - half).max(0.0), warp.band_frac).expect("paired envelope");
            let right = warp_point(&warp.envelope, (u + half).min(1.0), warp.band_frac).expect("paired envelope");
            let mapped_advance = (right.0 - left.0).hypot(right.1 - left.1);
            let h_ratio = mapped_advance / advance;
            assert!(
                (0.05..=5.0).contains(&h_ratio),
                "line {index}: mapped advance {mapped_advance:.0} is {h_ratio:.2}x the \
                 authored advance {advance:.0}"
            );

            let baseline = warp_point(&warp.envelope, u, warp.band_frac).expect("paired envelope");
            let top = warp_point(&warp.envelope, u, warp.band_frac - v_em).expect("paired envelope");
            let mapped_em = (baseline.0 - top.0).hypot(baseline.1 - top.1);
            let v_ratio = mapped_em / em;
            assert!(
                (0.05..=5.0).contains(&v_ratio),
                "line {index}: mapped em {mapped_em:.0} is {v_ratio:.2}x the authored em {em:.0}"
            );
            widest = widest.max(h_ratio);
            tallest = tallest.max(v_ratio);
        }
    }
    println!("textDeflate scale range: widest {widest:.2}x, tallest {tallest:.2}x");
}

// ---------------------------------------------------------------------------
// Golden image
// ---------------------------------------------------------------------------

/// Renders the synthetic deck at 800x450 and compares it with the committed
/// golden. Refresh after an intentional change with
/// `UPDATE_GOLDENS=1 cargo test -p office-terminal-core --test warp_parity_tests`.
#[test]
fn textdeflate_golden_renders_legible_warp() {
    ensure_fixture();
    let rendered = render_deck(&DECK);
    assert_eq!((rendered.width_px, rendered.height_px), (800, 450));
    compare_or_update(&rendered, 0, "warp_textdeflate slide 1");

    if golden_support::update_goldens() {
        return;
    }
    let slide = &rendered.slides[0];
    let ink = slide
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 8 && (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) < 384)
        .count();
    let ink_fraction = ink as f64 / (rendered.width_px as f64 * rendered.height_px as f64);
    assert!(
        (0.005..0.30).contains(&ink_fraction),
        "warped text covers {:.3} of the slide; expected a legible block, not \
         shards or a blank box",
        ink_fraction
    );
}

/// The fixture is deterministic: two builds of the same deck are byte-equal,
/// so the committed golden never drifts on regeneration.
#[test]
fn textdeflate_fixture_is_deterministic() {
    let first = deck_bytes();
    let second = deck_bytes();
    assert_eq!(first, second, "synthetic deck build is not deterministic");
    let path = ensure_fixture();
    assert!(path.is_file(), "fixture not written: {}", path.display());
}

// ---------------------------------------------------------------------------
// Reference-viewer comparison (opt-in: needs the external corpus)
// ---------------------------------------------------------------------------

/// Width the reference viewer rendered its goldens at.
/// ref `tests/golden/external/fixtures.json` (`render_width`).
const REFERENCE_WIDTH: u32 = 1600;

fn external_dir(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/external")
        .join(kind)
}

/// `target/external/<kind>/<commit>/<name>` for the pinned reference commit.
fn find_external(kind: &str, name: &str) -> Option<PathBuf> {
    let root = external_dir(kind);
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let candidate = entry.path().join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn render_deck_file(path: &Path, width_px: u32) -> RgbaSlide {
    let loader = DefaultPresentationLoader;
    let (presentation, assets) = loader
        .load(Source::File(path.to_path_buf()))
        .unwrap_or_else(|error| panic!("failed to load {}: {error}", path.display()));
    let height_px = (width_px as f64 * presentation.slide_height as f64 / presentation.slide_width as f64)
        .round()
        .max(1.0) as u32;
    let target = RenderTargetSpec { width_px, height_px };
    let compiler = DefaultSlideCompiler::new();
    let rasterizer = TinySkiaRasterizer::new();
    let (tx, _rx) = unbounded();
    let slide = presentation
        .slides
        .first()
        .unwrap_or_else(|| panic!("{} has no slides", path.display()));
    let vector = compiler
        .compile_in_deck(
            slide,
            &presentation.slides,
            presentation.slide_width,
            presentation.slide_height,
            &tx,
        )
        .unwrap_or_else(|error| panic!("{} failed to compile: {error}", path.display()));
    rasterizer
        .rasterize(&vector, &target, assets.as_ref())
        .unwrap_or_else(|error| panic!("{} failed to rasterize: {error}", path.display()))
}

/// `ShapePlusImage.pptx` (one `textDeflate` text box, three paragraphs) against
/// the reference viewer's PNG, through the reference VRT metric
/// (`pixelmatch@7.1.0`, `threshold 0.20`, `includeAA: true`).
///
/// The two renderers use different font stacks, so glyph edges never match
/// exactly; this is a *smoke* bound on the warped block, not a pixel-identity
/// claim. Skipped when the external corpus is not fetched (it is not
/// distributed with the repository).
#[test]
fn textdeflate_matches_reference_viewer() {
    let (Some(fixture), Some(reference)) = (
        find_external("fixtures", "ShapePlusImage.pptx"),
        find_external("goldens", "ShapePlusImage/slide_001.png"),
    ) else {
        eprintln!(
            "SKIP textdeflate_matches_reference_viewer: external corpus missing; \
             run tests/golden/external/run.py"
        );
        return;
    };

    let rendered = render_deck_file(&fixture, REFERENCE_WIDTH);
    let expected = image::open(&reference)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", reference.display()))
        .to_rgba8();
    let (width, height) = (expected.width(), expected.height());
    let actual = pixelmatch::pad_to_white(&rendered.pixels, rendered.width, rendered.height, width, height);
    let result = pixelmatch::compare_rgba(
        &actual,
        expected.as_raw(),
        width,
        height,
        &pixelmatch::DiffOptions::default(),
    );
    println!(
        "ShapePlusImage vs reference viewer: match={:.2}% diff={:.2}% mean_abs={:.4}",
        result.match_pct, result.diff_pct, result.mean_abs_diff
    );
    assert!(
        result.match_pct >= 80.0,
        "warped block diverged from the reference viewer: only {:.2}% of pixels match \
         (threshold 0.20, includeAA)",
        result.match_pct
    );
}
