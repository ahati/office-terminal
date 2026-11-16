//! Parity-ledger hygiene (L0).
//!
//! Reconciles the coverage ledger against what is actually running:
//! - No `#[ignore]` survives in compiled parity tests.
//! - Archived stubs live under `pending/` and document the missing API.
//! - Every ledger row stays arithmetic-consistent (`EXACT+BEHAVIOURAL+N/A+BLOCKED == cases`).
//! - Per-batch `pass` counts match compiled test counts.
//! - Blocked cases in `COVERAGE.md` match the pending archive allowlist.
//! - The external corpus score file is linked and valid.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn parity_sources(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("parity_") && name.ends_with(".rs"))
        })
        .collect()
}

#[test]
fn no_ignored_parity_tests_in_core_or_tui() {
    let root = repo_root();
    let mut sources = parity_sources(&root.join("crates/kitty-pptx-core/tests"));
    sources.extend(parity_sources(&root.join("crates/kitty-pptx-tui/tests")));
    assert!(!sources.is_empty(), "found parity test targets");
    for path in sources {
        let text = fs::read_to_string(&path).expect("read parity source");
        assert!(
            !text
                .lines()
                .any(|line| line.trim_start().starts_with("#[ignore")),
            "{} still contains an ignored parity case; move it to tests/parity/pending/",
            path.display()
        );
    }
}

#[test]
fn pending_archives_are_plain_text_and_document_the_missing_api() {
    let pending = repo_root().join("crates/kitty-pptx-core/tests/parity/pending");
    let entries = fs::read_dir(&pending).expect("pending archive directory");
    let mut archives = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }
        archives += 1;
        let text = fs::read_to_string(&path).expect("read archive");
        assert!(
            text.contains("Missing API") || text.contains("BLOCKED"),
            "archive {} must name the missing API",
            path.display()
        );
    }
    assert!(archives > 0, "pending archives exist");
}

fn parse_row(line: &str) -> Option<Vec<u64>> {
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    if cells.len() < 8 {
        return None;
    }
    // skip the header/separator/name columns; the last 7 are numeric
    cells[cells.len() - 7..]
        .iter()
        .map(|cell| cell.replace('*', "").parse::<u64>().ok())
        .collect()
}

fn parse_batch_name(line: &str) -> Option<&str> {
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    if cells.len() < 8 {
        return None;
    }
    let name = cells[0].trim_matches('`');
    if name == "batch" || name.starts_with(':') || name == "**total**" {
        None
    } else {
        Some(name)
    }
}

#[test]
fn coverage_rows_and_total_are_arithmetic_consistent() {
    let coverage =
        fs::read_to_string(repo_root().join("crates/kitty-pptx-core/tests/parity/COVERAGE.md"))
            .expect("COVERAGE.md");
    let mut rows = Vec::new();
    let mut total = None;
    for line in coverage.lines() {
        let Some(numbers) = parse_row(line) else {
            continue;
        };
        if line.contains("**total**") {
            total = Some(numbers);
        } else if line.contains('`') {
            rows.push(numbers);
        }
    }
    assert!(!rows.is_empty(), "ledger has batch rows");
    let mut sums = vec![0u64; 7];
    for numbers in &rows {
        assert_eq!(
            numbers[1] + numbers[2] + numbers[3] + numbers[4],
            numbers[0],
            "row is not EXACT+BEHAVIOURAL+N/A+BLOCKED == cases: {numbers:?}"
        );
        for (sum, value) in sums.iter_mut().zip(numbers) {
            *sum += value;
        }
    }
    let total = total.expect("total row");
    assert_eq!(total, sums, "total row must sum the batch rows");
}

fn count_tests_in_file(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| line.trim().starts_with("#[test]"))
        .count() as u64
}

#[test]
fn pass_counts_match_compiled_test_counts_per_batch() {
    let root = repo_root();
    let core_tests = root.join("crates/kitty-pptx-core/tests");
    let tui_tests = root.join("crates/kitty-pptx-tui/tests");

    let batch_files: HashMap<&str, Vec<PathBuf>> = HashMap::from([
        (
            "elements",
            vec![
                core_tests.join("parity_elements_tests.rs"),
                core_tests.join("parity_elements_support/decode_plan_cases.rs"),
                core_tests.join("parity_elements_table_tests.rs"),
                core_tests.join("parity_elements_chart_tests.rs"),
            ],
        ),
        (
            "fonts",
            vec![
                core_tests.join("parity_fonts_tests.rs"),
                core_tests.join("parity_fonts_slot_tests.rs"),
            ],
        ),
        (
            "misc",
            vec![
                core_tests.join("parity_misc_tests.rs"),
                core_tests.join("parity_parse_error_tests.rs"),
            ],
        ),
        (
            "shapes-a",
            vec![
                core_tests.join("parity_shapes_a_tests.rs"),
                core_tests.join("shapes_a_support/image_fill_cases.rs"),
                core_tests.join("shapes_a_support/image_fill_paint_cases.rs"),
                core_tests.join("shapes_a_support/gradient_cases.rs"),
            ],
        ),
        (
            "shapes-b",
            vec![
                core_tests.join("parity_shapes_b_tests.rs"),
                core_tests.join("shapes_b_support/line_end_cases.rs"),
                core_tests.join("shapes_b_support/bevel_effect_geometry.rs"),
            ],
        ),
        (
            "text-box-a",
            vec![
                core_tests.join("parity_text_box_a_tests.rs"),
                core_tests.join("parity_text_box_a_faces_tests.rs"),
                core_tests.join("parity_text_box_a_compat_tests.rs"),
                core_tests.join("parity_text_box_a_fontalgn_tests.rs"),
                core_tests.join("parity_text_box_a_spacing_tests.rs"),
                core_tests.join("parity_text_box_a_autofit_tests.rs"),
            ],
        ),
        (
            "text-box-b",
            vec![
                core_tests.join("parity_text_box_b_tests.rs"),
                core_tests.join("parity_text_box_b_bullet_image_tests.rs"),
                core_tests.join("parity_text_box_b_highlight_tests.rs"),
                core_tests.join("text_box_b_support/line_end_trim_cases.rs"),
                core_tests.join("parity_text_box_b_eavert_tests.rs"),
                core_tests.join("parity_text_box_b_stacked_tests.rs"),
                core_tests.join("text_box_b_support/stacked_decor_cases.rs"),
                core_tests.join("text_box_b_support/stacked_feature_cases.rs"),
                core_tests.join("parity_text_box_b_warp_tests.rs"),
            ],
        ),
        (
            "text-flow-a",
            vec![
                core_tests.join("parity_text_flow_a_tests.rs"),
                core_tests.join("parity_text_flow_a_tests_tabs.rs"),
                core_tests.join("parity_text_flow_a_tests_wrap.rs"),
                core_tests.join("parity_text_flow_a_draw_tests.rs"),
            ],
        ),
        (
            "text-flow-b",
            vec![
                core_tests.join("parity_text_flow_b_tests.rs"),
                core_tests.join("parity_text_flow_b_bidi_tests.rs"),
                core_tests.join("parity_text_flow_b_sea_tests.rs"),
                core_tests.join("parity_text_flow_b_indent_tests.rs"),
            ],
        ),
        (
            "viewer",
            vec![
                core_tests.join("parity_viewer_nav_tests.rs"),
                tui_tests.join("parity_viewer_nav_tests.rs"),
                core_tests.join("parity_viewer_selection_tests.rs"),
                core_tests.join("parity_viewer_lifecycle_tests.rs"),
                core_tests.join("parity_viewer_worker_tests.rs"),
                core_tests.join("parity_playwright_tests.rs"),
            ],
        ),
    ]);

    let coverage =
        fs::read_to_string(root.join("crates/kitty-pptx-core/tests/parity/COVERAGE.md"))
            .expect("COVERAGE.md");

    for line in coverage.lines() {
        let Some(batch) = parse_batch_name(line) else {
            continue;
        };
        let Some(numbers) = parse_row(line) else {
            continue;
        };
        let pass_in_table = numbers[5]; // pass is column index 5 in the 7 numeric columns
        let files = batch_files.get(batch).expect("known batch");
        let actual_tests: u64 = files.iter().map(|f| count_tests_in_file(f)).sum();
        assert_eq!(
            pass_in_table, actual_tests,
            "batch `{batch}`: pass in COVERAGE.md ({pass_in_table}) does not match #[test] count ({actual_tests})"
        );
    }
}

#[test]
fn blocked_cases_match_pending_archives_allowlist() {
    let root = repo_root();
    let pending_dir = root.join("crates/kitty-pptx-core/tests/parity/pending");
    let coverage =
        fs::read_to_string(root.join("crates/kitty-pptx-core/tests/parity/COVERAGE.md"))
            .expect("COVERAGE.md");

    let mut blocked_fn_names = Vec::new();

    let entries = fs::read_dir(&pending_dir).expect("read pending dir");
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read pending file");
        let mut is_blocked = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[ignore") {
                is_blocked = trimmed.to_lowercase().contains("blocked");
            } else if trimmed.starts_with("fn ") && trimmed.ends_with("() {}") {
                if is_blocked {
                    let fn_name = trimmed
                        .trim_start_matches("fn ")
                        .trim_end_matches("() {}")
                        .trim();
                    blocked_fn_names.push(fn_name.to_string());
                }
                is_blocked = false;
            }
        }
    }

    assert_eq!(
        blocked_fn_names.len(),
        19,
        "expected exactly 19 residual blocked functions, found {}",
        blocked_fn_names.len()
    );

    // Every archived function must appear in COVERAGE.md
    for fn_name in &blocked_fn_names {
        assert!(
            coverage.contains(fn_name),
            "blocked function `{fn_name}` must appear in COVERAGE.md"
        );
    }
}

#[test]
fn external_corpus_score_file_is_linked_and_valid() {
    let root = repo_root();
    let coverage =
        fs::read_to_string(root.join("crates/kitty-pptx-core/tests/parity/COVERAGE.md"))
            .expect("COVERAGE.md");
    assert!(
        coverage.contains("scores/external.json"),
        "COVERAGE.md must link to scores/external.json"
    );

    let score_path = root.join("crates/kitty-pptx-core/tests/parity/scores/external.json");
    assert!(score_path.exists(), "scores/external.json must exist");

    let content = fs::read_to_string(&score_path).expect("read external.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("parse external.json as valid json");

    let slides = parsed
        .get("slides")
        .and_then(|s| s.as_object())
        .expect("slides object in external.json");
    assert!(
        slides.len() >= 400,
        "external corpus must contain >= 400 slides, found {}",
        slides.len()
    );
}
