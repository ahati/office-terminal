//! Fuzzy TOC search logical page labels (PDF `/PageLabels`) tests.

use crate::app::search_state::{calculate_match_score, SearchState};
use office_terminal_core::TocEntry;

fn toc_entries() -> Vec<TocEntry> {
    vec![
        TocEntry {
            title: "Front Matter".to_string(),
            page_index: 0,
            level: 0,
        },
        TocEntry {
            title: "Chapter 1: Getting Started".to_string(),
            page_index: 6,
            level: 0,
        },
        TocEntry {
            title: "Chapter 2: Going Deeper".to_string(),
            page_index: 26,
            level: 0,
        },
    ]
}

/// Front matter roman, body decimal restarting at 1: page 7 shows "1".
fn labels() -> Vec<String> {
    vec![
        "cover".to_string(),
        "i".to_string(),
        "ii".to_string(),
        "iii".to_string(),
        "iv".to_string(),
        "v".to_string(),
        "1".to_string(),
        "2".to_string(),
        "3".to_string(),
        "4".to_string(),
        "5".to_string(),
        "6".to_string(),
        "7".to_string(),
        "8".to_string(),
        "9".to_string(),
        "10".to_string(),
        "11".to_string(),
        "12".to_string(),
        "13".to_string(),
        "14".to_string(),
        "15".to_string(),
        "16".to_string(),
        "17".to_string(),
        "18".to_string(),
        "19".to_string(),
        "20".to_string(),
        "21".to_string(),
    ]
}

#[test]
fn toc_entries_carry_logical_labels() {
    let mut search = SearchState::new();
    search.sync_from_toc(&toc_entries(), 27, &labels());

    let entries = search.entries();
    assert_eq!(entries.len(), 3);
    // Physical page 7 (0-based 6) displays as the logical label "1".
    assert_eq!(entries[1].page_label, "1");
    assert_eq!(entries[1].slide_number, 7);
    assert_eq!(entries[2].page_label, "21");
    assert_eq!(entries[2].slide_number, 27);
}

#[test]
fn entries_without_labels_default_to_physical_numbers() {
    let mut search = SearchState::new();
    search.sync_from_toc(&toc_entries(), 27, &[]);

    let entries = search.entries();
    assert!(entries.iter().all(|e| e.page_label.is_empty()));
    assert_eq!(entries[1].slide_number, 7);
}

#[test]
fn label_query_outranks_title_substring() {
    let mut search = SearchState::new();
    search.sync_from_toc(&toc_entries(), 27, &labels());

    // Typing the printed page number "1" must score the "1"-labelled page
    // above the front matter whose physical slide number is also 1 — the
    // logical label is the user-facing page identity.
    let query = "1";
    let scored: Vec<(usize, u32)> = search
        .entries()
        .iter()
        .enumerate()
        .map(|(idx, e)| (idx, calculate_match_score(query, e)))
        .collect();
    let best = scored.iter().copied().max_by_key(|&(_, s)| s).expect("entries");
    assert_eq!(best.0, 1, "the logical '1' page must win the query '1'");

    // Label prefixes score at page-number strength too.
    assert_eq!(calculate_match_score("2", &search.entries()[2]), 800);
    assert_eq!(calculate_match_score("cov", &search.entries()[0]), 800);
}

#[test]
fn flat_page_rows_carry_labels() {
    let mut search = SearchState::new();
    search.sync_from_toc(&[], 4, &labels()[..4]);

    let entries = search.entries();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].page_label, "cover");
    assert_eq!(entries[3].page_label, "iii");
}
