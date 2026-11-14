//! Parity-ledger hygiene (L0, partial).
//!
//! The full L0 re-derivation also reclassifies every blocked case and runs the
//! external corpus. This test guards the invariants the tree can check without
//! that corpus: no `#[ignore]` survives in compiled parity tests, archived
//! stubs live under `pending/`, and every ledger row stays arithmetic-consistent.

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
        } else if line.contains("`") {
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
