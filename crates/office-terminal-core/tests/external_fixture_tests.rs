//! Opt-in, self-skipping comparison against the pinned LibreOffice corpus.
//!
//! `run.py` fills `target/external/{fixtures,goldens}/<commit>/`; neither is
//! committed. Without a usable cache the test prints a one-line skip and
//! returns, so plain `cargo test` passes in milliseconds on a machine that
//! never opted in. With the cache it renders every deck that has reference
//! PNGs and prints a summary with decks compared, strict pass/fail,
//! diagnostics, and missing goldens. It is perceptual, not pixel-identical:
//! another renderer and font stack differ.
//!
//! Slides are rendered at the comparison width (400 px) and each 1600 px
//! reference is flattened/downscaled once into
//! `target/external/goldens/thumbnails/`; later runs decode only those
//! thumbnails.
//!
//! The reference viewer is never invoked here; only the cached PNGs are read.
//! Strict fixtures enforce their thresholds. Diagnostic (`strict: false`)
//! fixtures print their metrics but never fail: the 419-deck corpus
//! deliberately includes fuzz/malformed decks and cross-renderer differences.
//! See `tests/golden/external/README.md` for the corpus policy.
//!
//!     python3 crates/office-terminal-core/tests/golden/external/run.py

mod external_fixture_support;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use external_fixture_support::metrics::{compare_slide, DiffMetrics, COMPARE_WIDTH};
use external_fixture_support::render::render_fixture;
use external_fixture_support::scores::{self, ExternalScores, SlideScore};
use external_fixture_support::thumbs::ThumbnailCache;
use external_fixture_support::{
    load_manifest, Corpus, Fixture, Selection, MANIFEST_RELATIVE, MAX_LISTED, RUN_COMMAND,
};

#[derive(Default)]
struct Report {
    decks: usize,
    slides: usize,
    strict_pass: usize,
    strict_fail: usize,
    diagnostics: usize,
    diagnostic_errors: usize,
    errors: Vec<String>,
    failures: Vec<String>,
    scores: std::collections::BTreeMap<String, SlideScore>,
}

impl Report {
    fn record_error(&mut self, fixture: &Fixture, error: String) {
        let line = format!("{}: {error}", fixture.name);
        println!("  error {line}");
        if fixture.strict {
            self.strict_fail += 1;
            self.errors.push(line.clone());
            self.failures.push(line);
        } else {
            self.diagnostics += 1;
            self.diagnostic_errors += 1;
        }
    }
}

fn run_external_comparison() {
    // `run.py` may select several test entry points; render once per process.
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        println!("external corpus comparison already ran; skipping duplicate");
        return;
    }

    let corpus = match Corpus::discover() {
        Ok(corpus) => corpus,
        Err(reason) => {
            println!("skipping external corpus comparison: {reason}");
            return;
        }
    };
    let selection = corpus.select();
    print_selection(&corpus, &selection);

    if selection.compare.is_empty() {
        println!(
            "skipping external corpus comparison: no complete reference PNGs in \
             {}; run `{RUN_COMMAND}`",
            corpus.goldens_dir.display()
        );
        return;
    }

    let report = compare_corpus(&corpus, &selection);
    print_summary(&report, &corpus, &selection);

    assert!(
        report.failures.is_empty() && report.errors.is_empty(),
        "external renderer comparison failed:\n  {}\n\
         Regenerate references with `{RUN_COMMAND} --force-render` \
         and review the metric drift before changing budgets.",
        report
            .failures
            .iter()
            .chain(report.errors.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn compare_corpus(corpus: &Corpus, selection: &Selection) -> Report {
    let workers = worker_count(selection.compare.len());
    let thumbs = ThumbnailCache::new(&corpus.goldens_dir, &corpus.manifest.commit, COMPARE_WIDTH);
    println!(
        "comparing {} deck(s) with {workers} worker(s) at {COMPARE_WIDTH}px \
         (references {}px)",
        selection.compare.len(),
        corpus.manifest.render_width
    );
    let next = AtomicUsize::new(0);
    let collected = Mutex::new(Vec::with_capacity(selection.compare.len()));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let position = next.fetch_add(1, Ordering::Relaxed);
                let Some(&index) = selection.compare.get(position) else {
                    break;
                };
                let fixture = &corpus.manifest.files[index];
                let result = compare_deck(corpus, fixture, &thumbs);
                collected.lock().expect("result mutex poisoned").push((position, result));
            });
        }
    });

    let mut outcomes = collected.into_inner().expect("result mutex poisoned");
    outcomes.sort_by_key(|(position, _)| *position);
    let mut report = Report::default();
    for (position, outcome) in outcomes {
        let fixture = &corpus.manifest.files[selection.compare[position]];
        fold_deck(fixture, outcome, &mut report);
    }
    record_scores(corpus, &mut report);
    report
}

/// Write or ratchet the diagnostic baseline (`tests/parity/scores/external.json`).
fn record_scores(corpus: &Corpus, report: &mut Report) {
    let current = ExternalScores {
        commit: corpus.manifest.commit.clone(),
        compare_width: COMPARE_WIDTH,
        slides: report.scores.clone(),
    };
    if std::env::var_os("UPDATE_EXTERNAL_SCORES").is_some() {
        if let Err(error) = scores::write(&current) {
            report.failures.push(format!("external score write failed: {error}"));
        } else {
            println!("recorded {} external slide score(s)", current.slides.len());
        }
        return;
    }
    let baseline = match scores::load() {
        Ok(Some(baseline)) => baseline,
        Ok(None) => return,
        Err(error) => {
            report.failures.push(error);
            return;
        }
    };
    if baseline.commit != current.commit {
        println!(
            "external score baseline is for {} (current {}); not ratcheting",
            baseline.commit, current.commit
        );
        return;
    }
    if baseline.compare_width != current.compare_width {
        println!(
            "external score baseline was recorded at {}px (current {COMPARE_WIDTH}px); \
             not ratcheting; rerun with UPDATE_EXTERNAL_SCORES=1",
            baseline.compare_width
        );
        return;
    }
    let strict: std::collections::BTreeSet<&str> = corpus
        .manifest
        .files
        .iter()
        .filter(|fixture| fixture.strict)
        .map(|fixture| fixture.name.as_str())
        .collect();
    for (key, now) in &current.slides {
        let deck = key.split_once('/').map_or(key.as_str(), |(deck, _)| deck);
        if strict.contains(deck) {
            continue;
        }
        let Some(before) = baseline.slides.get(key) else {
            continue;
        };
        if now.cell_pass_ratio < before.cell_pass_ratio - 0.05 {
            report.failures.push(format!(
                "diagnostic {key}: cell pass {:.3} -> {:.3} (ratchet -0.05)",
                before.cell_pass_ratio, now.cell_pass_ratio
            ));
        }
        if now.global_mean_abs_diff > before.global_mean_abs_diff + 2.0 {
            report.failures.push(format!(
                "diagnostic {key}: global diff {:.2} -> {:.2} (ratchet +2.0)",
                before.global_mean_abs_diff, now.global_mean_abs_diff
            ));
        }
    }
}

fn worker_count(decks: usize) -> usize {
    let available = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    available.min(16).min(decks).max(1)
}

/// Per-deck metrics, computed off-thread so printing stays in manifest order.
enum DeckResult {
    Rendered {
        rendered_slides: usize,
        metrics: Vec<Result<DiffMetrics, String>>,
    },
    Error(String),
}

fn compare_deck(corpus: &Corpus, fixture: &Fixture, thumbs: &ThumbnailCache) -> DeckResult {
    let deck = corpus.fixtures_dir.join(format!("{}.pptx", fixture.name));
    // The metric compares at `COMPARE_WIDTH`; rendering larger only produces
    // pixels that `resize_box` immediately averages away.
    let rendered = match render_fixture(&deck, COMPARE_WIDTH) {
        Ok(slides) => slides,
        Err(error) => return DeckResult::Error(error),
    };
    let compared = rendered.len().min(fixture.slides);
    let metrics = (1..=compared)
        .map(|slide| -> Result<DiffMetrics, String> {
            let reference = thumbs.load(&fixture.golden(&corpus.goldens_dir, slide))?;
            Ok(compare_slide(&rendered[slide - 1], reference))
        })
        .collect();
    DeckResult::Rendered {
        rendered_slides: rendered.len(),
        metrics,
    }
}

fn fold_deck(fixture: &Fixture, outcome: DeckResult, report: &mut Report) {
    let (rendered_slides, metrics) = match outcome {
        DeckResult::Error(error) => {
            report.record_error(fixture, error);
            return;
        }
        DeckResult::Rendered {
            rendered_slides,
            metrics,
        } => (rendered_slides, metrics),
    };
    report.decks += 1;
    report.slides += rendered_slides;
    for (index, metrics) in metrics.iter().enumerate() {
        if let Ok(metrics) = metrics {
            report.scores.insert(
                scores::key(&fixture.name, index + 1),
                SlideScore::from(metrics),
            );
        }
    }
    if fixture.strict {
        if compare_strict(fixture, rendered_slides, metrics, report) {
            report.strict_pass += 1;
        } else {
            report.strict_fail += 1;
        }
    } else {
        report.diagnostics += 1;
        compare_diagnostic(fixture, rendered_slides, metrics, report);
    }
}

/// Returns whether the fixture met every strict threshold.
fn compare_strict(
    fixture: &Fixture,
    rendered_slides: usize,
    metrics: Vec<Result<DiffMetrics, String>>,
    report: &mut Report,
) -> bool {
    let mut passed = true;
    if rendered_slides != fixture.slides {
        passed = false;
        report.failures.push(format!(
            "{}: expected {} slide(s), rendered {}",
            fixture.name, fixture.slides, rendered_slides
        ));
    }
    let thresholds = fixture.thresholds();
    for (index, metrics) in metrics.into_iter().enumerate() {
        let metrics = match metrics {
            Ok(metrics) => metrics,
            Err(error) => {
                passed = false;
                report.failures.push(format!("{}: {error}", fixture.name));
                continue;
            }
        };
        println!(
            "strict {} slide {}: {} ({})",
            fixture.name,
            index + 1,
            describe(&metrics),
            fixture.note
        );
        for failure in threshold_failures(fixture, index + 1, &metrics, &thresholds) {
            passed = false;
            report.failures.push(failure);
        }
    }
    passed
}

fn compare_diagnostic(
    fixture: &Fixture,
    rendered_slides: usize,
    metrics: Vec<Result<DiffMetrics, String>>,
    report: &mut Report,
) {
    if rendered_slides != fixture.slides {
        println!(
            "diagnostic {}: expected {} slide(s), rendered {} (not enforced)",
            fixture.name, fixture.slides, rendered_slides
        );
    }
    for (index, metrics) in metrics.into_iter().enumerate() {
        match metrics {
            Ok(metrics) => println!(
                "diagnostic {} slide {}: {} ({})",
                fixture.name,
                index + 1,
                describe(&metrics),
                fixture.note
            ),
            Err(error) => {
                println!("diagnostic {} slide {}: {error}", fixture.name, index + 1);
                report.diagnostic_errors += 1;
            }
        }
    }
}

fn threshold_failures(
    fixture: &Fixture,
    slide: usize,
    metrics: &DiffMetrics,
    thresholds: &external_fixture_support::Thresholds,
) -> Vec<String> {
    let mut failures = Vec::new();
    if metrics.cell_pass_ratio < thresholds.min_cell_pass_ratio {
        failures.push(format!(
            "{} slide {slide}: {:.1}% of cells within 40/255 (need {:.1}%)",
            fixture.name,
            metrics.cell_pass_ratio * 100.0,
            thresholds.min_cell_pass_ratio * 100.0
        ));
    }
    if metrics.global_mean_abs_diff > thresholds.max_global_mean_abs_diff {
        failures.push(format!(
            "{} slide {slide}: global mean abs diff {:.2} > {:.2}",
            fixture.name, metrics.global_mean_abs_diff, thresholds.max_global_mean_abs_diff
        ));
    }
    if metrics.ink_fraction_delta > thresholds.max_ink_fraction_delta {
        failures.push(format!(
            "{} slide {slide}: ink coverage differs by {:.1}pp > {:.1}pp",
            fixture.name,
            metrics.ink_fraction_delta * 100.0,
            thresholds.max_ink_fraction_delta * 100.0
        ));
    }
    failures
}

fn describe(metrics: &DiffMetrics) -> String {
    format!(
        "cells={}x{} pass={:.3} global={:.2} worst={:.2} ink_delta={:.4}",
        metrics.cells_x,
        metrics.cells_y,
        metrics.cell_pass_ratio,
        metrics.global_mean_abs_diff,
        metrics.worst_cell_diff,
        metrics.ink_fraction_delta
    )
}

fn print_selection(corpus: &Corpus, selection: &Selection) {
    let strict = corpus
        .manifest
        .files
        .iter()
        .filter(|fixture| fixture.strict)
        .count();
    println!(
        "external corpus: manifest={} ({strict} strict, {} diagnostic), comparable={}, \
         missing_goldens={}, missing_fixtures={}",
        corpus.manifest.files.len(),
        corpus.manifest.files.len() - strict,
        selection.compare.len(),
        selection.missing_goldens.len(),
        selection.missing_fixtures.len()
    );
    print_names("missing goldens", &selection.missing_goldens);
    print_names("missing fixtures", &selection.missing_fixtures);
    let unrenderable = corpus.unrenderable(&selection.missing_goldens);
    for name in &unrenderable {
        if let Some(reason) = corpus.failures.get(*name) {
            println!("  unrenderable {name}: {reason}");
        }
    }
}

fn print_names(label: &str, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let shown = names.iter().take(MAX_LISTED).cloned().collect::<Vec<_>>();
    let suffix = if names.len() > shown.len() {
        format!(" ... +{} more", names.len() - shown.len())
    } else {
        String::new()
    };
    println!("  {label} ({}): {}{suffix}", names.len(), shown.join(", "));
}

fn print_summary(report: &Report, corpus: &Corpus, selection: &Selection) {
    let unrenderable = corpus.unrenderable(&selection.missing_goldens);
    println!(
        "external corpus summary: compared={} slides={} strict={} (pass={} fail={}) \
         diagnostics={} missing_goldens={} missing_fixtures={} errors={} \
         diagnostic_errors={} known_unrenderable={}",
        report.decks,
        report.slides,
        report.strict_pass + report.strict_fail,
        report.strict_pass,
        report.strict_fail,
        report.diagnostics,
        selection.missing_goldens.len(),
        selection.missing_fixtures.len(),
        report.errors.len(),
        report.diagnostic_errors,
        unrenderable.len()
    );
}

#[test]
fn external_libreoffice_renderer_comparison() {
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

    let mut skipped = BTreeSet::new();
    for entry in &manifest.skipped {
        assert!(
            is_safe_name(&entry.name),
            "unsafe skipped name {:?}",
            entry.name
        );
        assert!(skipped.insert(entry.name.as_str()), "duplicate skip");
        assert!(
            !names.contains(entry.name.as_str()),
            "{} is both rendered and skipped",
            entry.name
        );
        assert!(
            entry.path.starts_with("sd/qa/unit/data/pptx/") && entry.path.ends_with(".pptx"),
            "{}: skipped path must be LibreOffice pptx QA data",
            entry.name
        );
        assert!(
            !entry.reason.trim().is_empty(),
            "{}: skip must record a reason",
            entry.name
        );
    }
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
        fixture.path.ends_with(&format!("{name}.pptx")) && !fixture.path.contains(".."),
        "{name}: path {:?} must end with {name}.pptx",
        fixture.path
    );
    assert!(
        !fixture.note.trim().is_empty(),
        "{name}: note must describe what the fixture covers"
    );
    let thresholds = fixture.thresholds();
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
