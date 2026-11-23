//! Reading-order text corpus for in-document search and `dump` (issue #40
//! §7.10).
//!
//! Glyph runs are grouped back into visual lines and ordered per page by
//! [`search_order`]: paragraphs in char order, column sections column by
//! column, tables row by row, list labels attached to their item. A space is
//! inserted at a wide join; the line rect is the union of the run boxes;
//! field results (page numbers) are visible text and stay in.

use std::sync::Arc;

use office_terminal_core::contracts::{EmuRect, SearchCorpus, TextLine};
use oxml_layout::{
    DocumentStructure, GlyphRun, MultilingualGlyphRun, PageFrame, Point, PositionedElement, Transform,
};

use crate::limits::DocxLimits;
use crate::search_order::{reading_order, RunCandidate, TableStructure};

/// A horizontal gap larger than this fraction of the font size reads as a
/// word boundary.
const SPACE_JOIN_FACTOR: f64 = 0.25;

/// Per-page extracted text lines, indexed by physical page.
pub struct DocxSearchCorpus {
    pages: Vec<Vec<TextLine>>,
}

impl DocxSearchCorpus {
    /// Walk laid-out pages into reading-order lines under the corpus limits.
    /// `structure` is the engine's logical structure tree, which decides
    /// which lines are table content (row-major) rather than body flow.
    pub fn build(
        pages: &[Arc<PageFrame>],
        limits: &DocxLimits,
        structure: Option<&DocumentStructure>,
    ) -> Self {
        let tables = TableStructure::index(structure);
        let pages = pages
            .iter()
            .map(|page| build_page_lines(&page.elements, limits, page.width, &tables))
            .collect();
        Self { pages }
    }

    /// Total extracted text bytes (diagnostics / tests).
    pub fn total_text_bytes(&self) -> usize {
        self.pages
            .iter()
            .flat_map(|lines| lines.iter().map(|line| line.text.len()))
            .sum()
    }
}

impl SearchCorpus for DocxSearchCorpus {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page_text(&self, page: usize) -> &[TextLine] {
        self.pages.get(page).map(|lines| lines.as_slice()).unwrap_or(&[])
    }
}

fn build_page_lines(
    elements: &[PositionedElement],
    limits: &DocxLimits,
    page_width: f64,
    tables: &TableStructure,
) -> Vec<TextLine> {
    let mut candidates = Vec::new();
    collect_run_candidates(elements, tables, &mut candidates);

    // Both corpus caps are per page: a page stops producing lines once its
    // line count or its extracted text bytes reach the limit (the line that
    // crosses the byte cap is still emitted whole, since a line is atomic).
    let mut lines: Vec<TextLine> = Vec::new();
    let mut bytes_on_page = 0usize;
    for group in reading_order(candidates, page_width) {
        if lines.len() >= limits.max_corpus_lines_per_page
            || bytes_on_page >= limits.max_corpus_bytes_per_page
        {
            break;
        }
        let mut builder = LineBuilder::new();
        for run in group {
            builder.push(run);
        }
        if let Some(line) = builder.finish() {
            bytes_on_page += line.text.len();
            lines.push(line);
        }
    }
    lines
}

fn collect_run_candidates(
    elements: &[PositionedElement],
    tables: &TableStructure,
    candidates: &mut Vec<RunCandidate>,
) {
    collect_run_candidates_transformed(elements, Transform::IDENTITY, tables, false, candidates);
}

/// Walk the page, projecting every run into page space. `in_table` is
/// inherited from the enclosing `MarkedContent` structure ancestry.
fn collect_run_candidates_transformed(
    elements: &[PositionedElement],
    acc: Transform,
    tables: &TableStructure,
    in_table: bool,
    candidates: &mut Vec<RunCandidate>,
) {
    for element in elements {
        match element {
            PositionedElement::Text(run) => {
                push_candidate(candidates, project_glyph_run(run, acc, in_table));
            }
            PositionedElement::MultilingualText(run) => {
                push_candidate(candidates, project_multilingual(run, acc, in_table));
            }
            PositionedElement::Group(group) => {
                let child_acc = group.transform.then(acc);
                collect_run_candidates_transformed(
                    &group.children,
                    child_acc,
                    tables,
                    in_table,
                    candidates,
                );
            }
            PositionedElement::MarkedContent {
                structure,
                children,
            } => {
                let in_table = in_table || structure.is_some_and(|id| tables.contains(id));
                collect_run_candidates_transformed(children, acc, tables, in_table, candidates);
            }
            _ => {}
        }
    }
}

fn push_candidate(candidates: &mut Vec<RunCandidate>, candidate: Option<RunCandidate>) {
    if let Some(candidate) = candidate {
        if !candidate.text.is_empty() {
            candidates.push(candidate);
        }
    }
}

/// Scale of the y axis under an accumulated transform (font size in page
/// space), falling back to the authored size for degenerate transforms.
fn page_font_size(acc: &Transform, font_size: f64) -> f64 {
    let scale_y = (acc.b * acc.b + acc.d * acc.d).sqrt();
    if scale_y.is_finite() && scale_y > 0.0 {
        font_size * scale_y
    } else {
        font_size
    }
}

fn project_glyph_run(run: &GlyphRun, acc: Transform, table: bool) -> Option<RunCandidate> {
    let unscaled_advance: f64 = run.advances.iter().sum();
    let p0 = acc.apply(run.origin);
    let p1 = acc.apply(Point {
        x: run.origin.x + unscaled_advance,
        y: run.origin.y,
    });
    Some(RunCandidate {
        node: run.source.map(|span| span.node.get()),
        char_start: run.source.map(|span| span.char_start).unwrap_or(0),
        origin: p0,
        font_size: page_font_size(&acc, run.font_size),
        advance: (p1.x - p0.x).abs(),
        text: run.text.clone(),
        table,
    })
}

fn project_multilingual(
    run: &MultilingualGlyphRun,
    acc: Transform,
    table: bool,
) -> Option<RunCandidate> {
    let unscaled_advance: f64 = run.x_advances.iter().sum();
    let p0 = acc.apply(run.origin);
    let p1 = acc.apply(Point {
        x: run.origin.x + unscaled_advance,
        y: run.origin.y,
    });
    Some(RunCandidate {
        node: run.source.map(|span| span.node.get()),
        char_start: run.source.map(|span| span.char_start).unwrap_or(0),
        origin: p0,
        font_size: page_font_size(&acc, run.font_size),
        advance: (p1.x - p0.x).abs(),
        text: run.logical_text.clone(),
        table,
    })
}

/// Accumulates one visual line's text and bounds.
struct LineBuilder {
    text: String,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    last_end_x: Option<f64>,
    last_font_size: f64,
}

impl LineBuilder {
    fn new() -> Self {
        Self {
            text: String::new(),
            x_min: f64::INFINITY,
            x_max: f64::NEG_INFINITY,
            y_min: f64::INFINITY,
            y_max: f64::NEG_INFINITY,
            last_end_x: None,
            last_font_size: 0.0,
        }
    }

    fn push(&mut self, run: RunCandidate) {
        let run_start = run.origin.x;
        let run_end = run_start + run.advance;
        // Insert a space at a wide join when neither side has whitespace.
        if let Some(last_end) = self.last_end_x {
            let gap = run_start - last_end;
            let threshold = SPACE_JOIN_FACTOR * run.font_size.max(1.0);
            if gap > threshold
                && !self.text.ends_with(char::is_whitespace)
                && !run.text.starts_with(char::is_whitespace)
            {
                self.text.push(' ');
            }
        }
        self.last_end_x = Some(run_end);
        self.last_font_size = run.font_size;
        self.x_min = self.x_min.min(run_start);
        self.x_max = self.x_max.max(run_end);
        let ascent = run.origin.y - run.font_size * 0.8;
        let descent = run.origin.y + run.font_size * 0.2;
        self.y_min = self.y_min.min(ascent);
        self.y_max = self.y_max.max(descent);
        self.text.push_str(&run.text);
    }

    fn finish(self) -> Option<TextLine> {
        if self.text.trim().is_empty() {
            return None;
        }
        let size = self.last_font_size.max(1.0);
        let rect = EmuRect::new(
            pt_to_emu(self.x_min),
            pt_to_emu(self.y_min),
            pt_to_emu(self.x_max - self.x_min).max(pt_to_emu(size * 0.1)),
            pt_to_emu(self.y_max - self.y_min).max(pt_to_emu(size * 0.5)),
        );
        Some(TextLine {
            text: self.text,
            rect,
        })
    }
}

fn pt_to_emu(pt: f64) -> f32 {
    (pt * 12_700.0).round() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxml_layout::{Color, FontId, SourceNodeId, SourceSpan, StructureId};

    fn glyph_run(text: &str, origin: Point, source: Option<SourceSpan>) -> GlyphRun {
        GlyphRun {
            origin,
            font_id: FontId(0),
            font_size: 12.0,
            glyph_ids: vec![1; text.chars().count()],
            advances: vec![6.0; text.chars().count()],
            text: text.to_string(),
            source,
            color: Color::BLACK,
            bold: false,
            italic: false,
            field_kind: None,
            field_source: None,
            note: None,
            note_reference_source: None,
            tab_aligned: None,
        }
    }

    fn node(id: u32) -> SourceNodeId {
        SourceNodeId::new(id).expect("non-zero")
    }

    fn lines(elements: Vec<PositionedElement>) -> Vec<TextLine> {
        build_page_lines(&elements, &DocxLimits::default(), 612.0, &TableStructure::default())
    }

    fn texts(elements: Vec<PositionedElement>) -> Vec<String> {
        lines(elements).into_iter().map(|line| line.text).collect()
    }

    #[test]
    fn same_node_runs_concatenate_in_char_order() {
        let span_a = SourceSpan {
            node: node(7),
            char_start: 0,
            char_end: 5,
        };
        let span_b = SourceSpan {
            node: node(7),
            char_start: 5,
            char_end: 11,
        };
        let elements = vec![
            PositionedElement::Text(glyph_run("world", Point { x: 30.0, y: 50.0 }, Some(span_b))),
            PositionedElement::Text(glyph_run("Hello ", Point { x: 0.0, y: 50.0 }, Some(span_a))),
        ];
        assert_eq!(texts(elements), ["Hello world"]);
    }

    #[test]
    fn wide_joins_insert_spaces_and_baseline_runs_sort_by_x() {
        let elements = vec![
            PositionedElement::Text(glyph_run("second", Point { x: 200.0, y: 50.0 }, None)),
            PositionedElement::Text(glyph_run("first", Point { x: 0.0, y: 50.0 }, None)),
        ];
        assert_eq!(texts(elements), ["first second"]);
    }

    #[test]
    fn different_baselines_split_lines() {
        let elements = vec![
            PositionedElement::Text(glyph_run("top", Point { x: 0.0, y: 20.0 }, None)),
            PositionedElement::Text(glyph_run("bottom", Point { x: 0.0, y: 60.0 }, None)),
        ];
        assert_eq!(texts(elements), ["top", "bottom"]);
    }

    #[test]
    fn corpus_implements_the_contract_and_bounds_are_emu() {
        let elements = vec![PositionedElement::Text(glyph_run(
            "word",
            Point { x: 10.0, y: 42.0 },
            None,
        ))];
        let pages = vec![Arc::new(PageFrame::new(1, 612.0, 792.0, elements))];
        let corpus = DocxSearchCorpus::build(&pages, &DocxLimits::default(), None);
        assert_eq!(corpus.page_count(), 1);
        assert_eq!(corpus.page_text(0).len(), 1);
        assert_eq!(corpus.page_text(0)[0].text, "word");
        let rect = corpus.page_text(0)[0].rect;
        assert!(rect.w > 0.0 && rect.h > 0.0);
        assert!(corpus.total_text_bytes() >= 4);
        assert!(corpus.page_text(99).is_empty());
    }

    #[test]
    fn line_caps_are_respected() {
        let elements: Vec<_> = (0..50)
            .map(|index| {
                PositionedElement::Text(glyph_run(
                    "line",
                    Point {
                        x: 0.0,
                        y: index as f64 * 20.0,
                    },
                    None,
                ))
            })
            .collect();
        assert_eq!(lines(elements.clone()).len(), 50);
        let capped = build_page_lines(
            &elements,
            &DocxLimits {
                max_corpus_lines_per_page: 10,
                ..DocxLimits::default()
            },
            612.0,
            &TableStructure::default(),
        );
        assert_eq!(capped.len(), 10);
    }

    #[test]
    fn byte_cap_is_per_page_not_per_line() {
        // 50 lines of 4 bytes each (200 bytes): a 50-byte page budget must
        // stop well short of 50 lines, while no single line exceeds it.
        let elements: Vec<_> = (0..50)
            .map(|index| {
                PositionedElement::Text(glyph_run(
                    "line",
                    Point {
                        x: 0.0,
                        y: index as f64 * 20.0,
                    },
                    None,
                ))
            })
            .collect();
        let capped = build_page_lines(
            &elements,
            &DocxLimits {
                max_corpus_bytes_per_page: 50,
                ..DocxLimits::default()
            },
            612.0,
            &TableStructure::default(),
        );
        let total: usize = capped.iter().map(|line| line.text.len()).sum();
        assert!(
            capped.len() < 50,
            "the page byte budget must cut the page short, got {} lines",
            capped.len()
        );
        assert!(
            total <= 50 + "line".len(),
            "at most the crossing line may exceed the budget, got {total} bytes"
        );
    }

    #[test]
    fn transformed_group_text_positions_are_in_page_space() {
        let group = oxml_layout::GroupElement {
            transform: Transform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 100.0,
                f: 200.0,
            },
            clip: None,
            opacity: 1.0,
            effects: Vec::new(),
            children: vec![PositionedElement::Text(glyph_run(
                "group text",
                Point { x: 10.0, y: 20.0 },
                None,
            ))],
        };
        let elements = vec![PositionedElement::Group(group)];
        let lines = lines(elements);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "group text");
        assert_eq!(lines[0].rect.x, pt_to_emu(110.0));
    }

    #[test]
    fn unattributed_labels_attach_to_the_adjacent_item_line() {
        // A numbered list: the label has no source node, the item has one.
        let item = SourceSpan {
            node: node(7),
            char_start: 0,
            char_end: 7,
        };
        let elements = vec![
            PositionedElement::Text(glyph_run("Headings", Point { x: 108.0, y: 50.0 }, Some(item))),
            PositionedElement::Text(glyph_run("1.", Point { x: 90.0, y: 50.0 }, None)),
            PositionedElement::Text(glyph_run("Lists", Point { x: 108.0, y: 70.0 }, Some(SourceSpan { node: node(8), char_start: 0, char_end: 5 }))),
            PositionedElement::Text(glyph_run("2.", Point { x: 90.0, y: 70.0 }, None)),
        ];
        assert_eq!(texts(elements), ["1. Headings", "2. Lists"]);
    }

    #[test]
    fn body_text_in_two_column_bands_reads_column_by_column() {
        // Column 1 (x = 72..280) and column 2 (x = 324..530) share baselines.
        let para = |node_id: u32, text: &str, x: f64, y: f64| {
            PositionedElement::Text(glyph_run(
                text,
                Point { x, y },
                Some(SourceSpan { node: node(node_id), char_start: 0, char_end: text.len() as u32 }),
            ))
        };
        let elements = vec![
            para(1, "left one", 72.0, 50.0),
            para(2, "right one", 324.0, 50.0),
            para(3, "left two", 72.0, 70.0),
            para(4, "right two", 324.0, 70.0),
        ];
        assert_eq!(texts(elements), ["left one", "left two", "right one", "right two"]);
    }

    #[test]
    fn table_cells_keep_row_major_order_even_with_wide_gaps() {
        // Same geometry as two columns, but marked as table content through
        // the structure tree: rows read left to right.
        let structure = oxml_layout::DocumentStructure {
            root: StructureId::new(1).expect("non-zero"),
            nodes: vec![
                oxml_layout::StructureNode { id: StructureId::new(1).unwrap(), role: oxml_layout::StructureRole::Document, children: vec![StructureId::new(2).unwrap()], alternate_text: None },
                oxml_layout::StructureNode { id: StructureId::new(2).unwrap(), role: oxml_layout::StructureRole::Table, children: vec![StructureId::new(3).unwrap(), StructureId::new(4).unwrap()], alternate_text: None },
                oxml_layout::StructureNode { id: StructureId::new(3).unwrap(), role: oxml_layout::StructureRole::TableCell, children: Vec::new(), alternate_text: None },
                oxml_layout::StructureNode { id: StructureId::new(4).unwrap(), role: oxml_layout::StructureRole::TableCell, children: Vec::new(), alternate_text: None },
            ],
        };
        let tables = TableStructure::index(Some(&structure));
        let para = |node_id: u32, text: &str, x: f64, y: f64, structure: u32| PositionedElement::MarkedContent {
            structure: StructureId::new(structure),
            children: vec![PositionedElement::Text(glyph_run(
                text,
                Point { x, y },
                Some(SourceSpan { node: node(node_id), char_start: 0, char_end: text.len() as u32 }),
            ))],
        };
        let elements = vec![
            para(1, "A1", 72.0, 50.0, 3),
            para(2, "B1", 324.0, 50.0, 4),
            para(3, "A2", 72.0, 70.0, 3),
            para(4, "B2", 324.0, 70.0, 4),
        ];
        let lines = build_page_lines(&elements, &DocxLimits::default(), 612.0, &tables);
        let texts: Vec<String> = lines.into_iter().map(|line| line.text).collect();
        assert_eq!(texts, ["A1", "B1", "A2", "B2"]);
    }

    #[test]
    fn a_wide_line_splits_column_strips() {
        // Full-width paragraph between two column bands: each strip is read
        // on its own, in top-down order.
        let para = |node_id: u32, text: &str, x: f64, y: f64| {
            PositionedElement::Text(glyph_run(
                text,
                Point { x, y },
                Some(SourceSpan { node: node(node_id), char_start: 0, char_end: text.len() as u32 }),
            ))
        };
        let mut wide = glyph_run("wide paragraph spanning the page", Point { x: 72.0, y: 90.0 }, None);
        wide.advances = vec![16.0; wide.glyph_ids.len()];
        let elements = vec![
            para(1, "left one", 72.0, 50.0),
            para(2, "right one", 324.0, 50.0),
            PositionedElement::Text(wide),
            para(3, "left two", 72.0, 130.0),
            para(4, "right two", 324.0, 130.0),
        ];
        assert_eq!(
            texts(elements),
            ["left one", "right one", "wide paragraph spanning the page", "left two", "right two"]
        );
    }
}
