//! Continuous multi-column section layout through the real engine: the
//! body band a column-count change starts on the current page (vendored
//! engine patch `crates/patches/0009`), equal and explicit tracks, the
//! separator rule, reading order, and the known upstream gaps recorded as
//! `#[ignore]`d tests.

mod common;

use std::sync::Arc;

use office_terminal_core::contracts::{PageSource, VectorCommand};
use office_terminal_docx::DocxPageSource;

fn open(bytes: Vec<u8>) -> DocxPageSource {
    DocxPageSource::open(Arc::from(bytes.into_boxed_slice())).expect("document opens")
}

/// Reading-order text of one page, lines joined by single spaces.
fn page_text(source: &DocxPageSource, index: usize) -> String {
    let corpus = source.search_corpus();
    corpus
        .page_text(index)
        .iter()
        .map(|line| line.text.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `(x_pt, y_pt, text)` of every non-blank text run on a page, in points.
fn text_runs_on_page(source: &DocxPageSource, index: usize) -> Vec<(f32, f32, String)> {
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(index, &tx).expect("compile");
    let mut runs = Vec::new();
    for command in &slide.commands {
        if let VectorCommand::DrawText { runs: page_runs, .. } = command {
            for run in page_runs {
                if run.text.trim().is_empty() {
                    continue;
                }
                runs.push((
                    run.glyphs[0].x / 12_700.0,
                    run.glyphs[0].y / 12_700.0,
                    run.text.clone(),
                ));
            }
        }
    }
    runs
}

/// Column tracks work: text fills column 1 (x = 72 pt) down the page, then
/// continues at the top of column 2 (x = 72 + 216 + 36 = 324 pt), then the
/// next page. The continuous section starts on the intro's page (patch 0009).
#[test]
fn two_column_tracks_fill_left_column_before_right() {
    let source = open(common::columns_docx(100));
    assert!(
        source.len() >= 2,
        "100 lines need more than one page of two columns"
    );

    // Page 1: the intro, then the band below it with both tracks in use.
    let runs = text_runs_on_page(&source, 0);
    let column_x = |runs: &[(f32, f32, String)]| -> Vec<i32> {
        runs.iter()
            .filter(|run| run.2.starts_with("Column"))
            .map(|run| run.0.round() as i32)
            .collect()
    };
    let xs = column_x(&runs);
    assert!(xs.contains(&72), "column 1 at the left margin, got {xs:?}");
    assert!(xs.contains(&324), "column 2 after the 0.5in gap, got {xs:?}");

    // Page 2: full-height tracks, column 1 first.
    let next = text_runs_on_page(&source, 1);
    assert!(column_x(&next).contains(&72), "page 2 resumes in column 1");

    // Reading order on page 1: column 1 top-to-bottom before column 2. The
    // corpus joins word runs into lines, so line numbers are read there.
    let corpus = source.search_corpus();
    let lines: Vec<(i32, usize)> = corpus
        .page_text(0)
        .iter()
        .filter_map(|line| {
            let number = line
                .text
                .trim()
                .strip_prefix("Column line ")?
                .parse::<usize>()
                .ok()?;
            Some(((line.rect.x / 12_700.0).round() as i32, number))
        })
        .collect();
    let last_of_column_1 = lines
        .iter()
        .filter(|(x, _)| *x == 72)
        .map(|(_, number)| *number)
        .max()
        .expect("column 1 lines");
    let first_of_column_2 = lines
        .iter()
        .filter(|(x, _)| *x == 324)
        .map(|(_, number)| *number)
        .min()
        .expect("column 2 lines");
    let text = page_text(&source, 0);
    let last = text
        .find(&format!("Column line {last_of_column_1}"))
        .expect("text");
    let first = text
        .find(&format!("Column line {first_of_column_2}"))
        .expect("text");
    assert!(last < first, "column 1 reads before column 2");
}

/// Word starts a continuous section that changes the column count on the
/// **current page**, right below the preceding content. Upstream `rdocx`
/// 0.17 started it on a new page (`paginator.rs` `continuous_geometry_matches`
/// required identical tracks and margins); the vendored engine carries the
/// fix as `crates/patches/0009`.
#[test]
fn continuous_column_change_starts_on_the_current_page() {
    let source = open(common::columns_docx(4));
    assert_eq!(
        source.len(),
        1,
        "a one-line intro and four column lines fit one page"
    );

    let runs = text_runs_on_page(&source, 0);
    let intro_y = runs
        .iter()
        .find(|run| run.2.starts_with("Single"))
        .map(|run| run.1)
        .expect("intro on page 1");
    let column_y = runs
        .iter()
        .filter(|run| run.2.starts_with("Column"))
        .map(|run| run.1)
        .reduce(f32::min)
        .expect("column lines on page 1");
    assert!(
        column_y > intro_y,
        "the two-column section follows the intro on the same page"
    );
}

/// The `sample3.docx` shape: the section above the columns declares a
/// different bottom margin. Word keeps the continuous section on the page
/// (the member's own margins apply from its next page), so the band still
/// starts under the preceding content.
#[test]
fn continuous_column_change_with_different_margins_stays_on_the_page() {
    let source = open(common::columns_docx_with(
        4,
        common::ColumnOpts {
            first_section_bottom_margin: Some(720),
            ..common::ColumnOpts::default()
        },
    ));
    assert_eq!(source.len(), 1, "margins alone do not force a page");
    let runs = text_runs_on_page(&source, 0);
    assert!(
        runs.iter().any(|run| run.2.starts_with("Column")),
        "the column section starts on the intro's page"
    );
}

/// Explicit unequal tracks: a 2in first column (x = 72 pt, 144 pt wide) and
/// a 4.5in second column (x = 72 + 144 + 36 = 252 pt).
#[test]
fn unequal_column_widths_lay_out_on_their_tracks() {
    let source = open(common::columns_docx_with(
        100,
        common::ColumnOpts {
            unequal: true,
            ..common::ColumnOpts::default()
        },
    ));
    let runs = text_runs_on_page(&source, 0);
    let xs: Vec<i32> = runs
        .iter()
        .filter(|run| run.2.starts_with("Column"))
        .map(|run| run.0.round() as i32)
        .collect();
    assert!(xs.contains(&72), "narrow first track, got {xs:?}");
    assert!(xs.contains(&252), "wide second track after the gap, got {xs:?}");
}

/// `w:sep="1"` draws the vertical rule between the tracks, midway across
/// the 0.5in gap (x = 72 + 216 + 18 = 306 pt).
#[test]
fn column_separator_rule_is_drawn_between_tracks() {
    let source = open(common::columns_docx_with(
        60,
        common::ColumnOpts {
            separator: true,
            ..common::ColumnOpts::default()
        },
    ));
    let (tx, _rx) = crossbeam_channel::unbounded();
    let slide = source.compile_page(0, &tx).expect("compile");
    let emu = |pt: f32| pt * 12_700.0;
    let rule = slide.commands.iter().any(|command| {
        let VectorCommand::DrawPath { path, stroke, .. } = command else {
            return false;
        };
        if stroke.is_none() {
            return false;
        }
        let xs: Vec<f32> = path
            .segments
            .iter()
            .filter_map(|segment| match segment {
                office_terminal_core::contracts::PathSegment::MoveTo { x, .. }
                | office_terminal_core::contracts::PathSegment::LineTo { x, .. } => Some(*x),
                _ => None,
            })
            .collect();
        let Some((&min, _)) = xs.iter().min_by(|a, b| a.total_cmp(b)).map(|m| (m, m)) else {
            return false;
        };
        let max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        path.segments.len() >= 2
            && (max - min).abs() < emu(1.0)
            && (min - emu(306.0)).abs() < emu(20.0)
    });
    assert!(rule, "a vertical stroked rule sits in the column gap");
}

/// Word balances a multi-column section whose text ends mid-page (document
/// end or a following continuous break): 8 lines after a one-line intro split
/// 4 / 4 across the two columns. The engine fills column 1 first and only
/// spills into column 2 once column 1 is full.
#[test]
#[ignore = "rdocx 0.17 does not balance columns that end mid-page (docs/issues/docx-input-support.md §7.18)"]
fn mid_page_column_sections_are_balanced() {
    let source = open(common::columns_docx(8));
    assert_eq!(source.len(), 1);
    let runs = text_runs_on_page(&source, 0);
    assert!(
        runs
            .iter()
            .any(|run| run.2.starts_with("Column") && (run.0 - 324.0).abs() < 2.0),
        "Word places part of the 8 lines in column 2"
    );
}

/// A column break (`<w:br w:type="column"/>`) moves the text after it to
/// the top of the next column, like Word's Ctrl+Shift+Enter.
#[test]
#[ignore = "rdocx 0.17 does not model column breaks (paginator.rs: \"F-269 does not model it\"; docs/issues/docx-input-support.md §7.18)"]
fn column_breaks_move_to_the_top_of_the_next_column() {
    let source = open(common::columns_docx_with(
        2,
        common::ColumnOpts {
            column_break_after: Some(0),
            ..common::ColumnOpts::default()
        },
    ));
    assert_eq!(source.len(), 1);
    let runs = text_runs_on_page(&source, 0);
    let after = runs
        .iter()
        .find(|run| run.2.starts_with("Column line 1"))
        .expect("the paragraph after the break");
    assert!(
        (after.0 - 324.0).abs() < 2.0,
        "text after a column break starts in column 2, got x={:.1}",
        after.0
    );
}
