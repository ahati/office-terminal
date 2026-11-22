//! Reading-order text corpus for in-document search and `dump` (issue #40
//! §7.10).
//!
//! Glyph runs are grouped back into visual lines. Primary grouping follows
//! the source node (paragraph / cell), concatenating in `char_start` order so
//! logical order survives tables, columns and RTL. Without source
//! attribution, runs on the same baseline within a tolerance sort by pen x.
//! A space is inserted at a wide join; the line rect is the union of the run
//! boxes; field results (page numbers) are visible text and stay in.

use std::sync::Arc;

use office_terminal_core::contracts::{EmuRect, SearchCorpus, TextLine};
use oxml_layout::{
    GlyphRun, MultilingualGlyphRun, PageFrame, Point, PositionedElement, Transform,
};

use crate::limits::DocxLimits;

/// Baseline grouping tolerance in typographic points.
const BASELINE_TOLERANCE_PT: f64 = 0.5;
/// A horizontal gap larger than this fraction of the font size reads as a
/// word boundary.
const SPACE_JOIN_FACTOR: f64 = 0.25;

/// Per-page extracted text lines, indexed by physical page.
pub struct DocxSearchCorpus {
    pages: Vec<Vec<TextLine>>,
}

impl DocxSearchCorpus {
    /// Walk laid-out pages into reading-order lines under the corpus limits.
    pub fn build(pages: &[Arc<PageFrame>], limits: &DocxLimits) -> Self {
        let pages = pages
            .iter()
            .map(|page| build_page_lines(&page.elements, limits))
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

/// One candidate run with its reading-order keys.
struct RunCandidate {
    node: Option<u32>,
    char_start: u32,
    origin: Point,
    font_size: f64,
    advance: f64,
    text: String,
}

fn build_page_lines(elements: &[PositionedElement], limits: &DocxLimits) -> Vec<TextLine> {
    let mut candidates = Vec::new();
    collect_run_candidates(elements, &mut candidates);

    // Group candidates into lines. With source attribution, a node is one
    // line fragment; without it, the baseline band is the line.
    let mut ordered: Vec<RunCandidate> = Vec::with_capacity(candidates.len());
    let mut remaining: Vec<RunCandidate> = candidates;

    // Node groups first, ordered by their first run's (y, x).
    let mut node_groups: Vec<(u32, Vec<RunCandidate>)> = Vec::new();
    remaining.retain_mut(|candidate| {
        let Some(node) = candidate.node else {
            return true;
        };
        match node_groups.iter_mut().find(|(id, _)| *id == node) {
            Some((_, runs)) => runs.push(take_candidate(candidate)),
            None => node_groups.push((node, vec![take_candidate(candidate)])),
        }
        false
    });
    node_groups.sort_by(|(_, a), (_, b)| {
        let ka = (a.first().map(|r| r.origin.y).unwrap_or(0.0), a.first().map(|r| r.origin.x).unwrap_or(0.0));
        let kb = (b.first().map(|r| r.origin.y).unwrap_or(0.0), b.first().map(|r| r.origin.x).unwrap_or(0.0));
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    for (_, mut runs) in node_groups {
        runs.sort_by_key(|run| run.char_start);
        ordered.append(&mut runs);
    }

    // Baseline-band fallback for unattributed runs.
    remaining.sort_by(|a, b| {
        let band = |run: &RunCandidate| (run.origin.y / BASELINE_TOLERANCE_PT).round();
        let ka = (band(a), a.origin.x);
        let kb = (band(b), b.origin.x);
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    ordered.append(&mut remaining);

    // Concatenate into lines, splitting whenever the grouping key changes.
    let mut lines: Vec<TextLine> = Vec::new();
    let mut bytes_on_page = 0usize;
    let mut current: Option<LineBuilder> = None;
    for run in ordered {
        let key = LineKey {
            node: run.node,
            band: (run.origin.y / BASELINE_TOLERANCE_PT).round() as i64,
        };
        let builder = current.take();
        let mut builder = match builder {
            Some(builder) if builder.key == key && builder.node_continuous(&run) => builder,
            Some(builder) => {
                if let Some(line) = builder.finish() {
                    bytes_on_page += line.text.len();
                    if lines.len() < limits.max_corpus_lines_per_page {
                        lines.push(line);
                    }
                }
                LineBuilder::new(key)
            }
            None => LineBuilder::new(key),
        };
        builder.push(run);
        if builder.bytes > limits.max_corpus_bytes_per_page
            || lines.len() >= limits.max_corpus_lines_per_page
        {
            if let Some(line) = builder.finish() {
                bytes_on_page += line.text.len();
                if lines.len() < limits.max_corpus_lines_per_page {
                    lines.push(line);
                }
            }
            break;
        }
        current = Some(builder);
    }
    if let Some(builder) = current {
        if let Some(line) = builder.finish() {
            bytes_on_page += line.text.len();
            if lines.len() < limits.max_corpus_lines_per_page {
                lines.push(line);
            }
        }
    }
    let _ = bytes_on_page;
    lines
}

fn take_candidate(candidate: &mut RunCandidate) -> RunCandidate {
    RunCandidate {
        node: candidate.node,
        char_start: candidate.char_start,
        origin: candidate.origin,
        font_size: candidate.font_size,
        advance: candidate.advance,
        text: std::mem::take(&mut candidate.text),
    }
}

fn collect_run_candidates(
    elements: &[PositionedElement],
    candidates: &mut Vec<RunCandidate>,
) {
    collect_run_candidates_transformed(elements, Transform::IDENTITY, candidates);
}

fn collect_run_candidates_transformed(
    elements: &[PositionedElement],
    acc: Transform,
    candidates: &mut Vec<RunCandidate>,
) {
    for element in elements {
        match element {
            PositionedElement::Text(run) => {
                push_candidate(candidates, project_glyph_run(run, acc));
            }
            PositionedElement::MultilingualText(run) => {
                push_candidate(candidates, project_multilingual(run, acc));
            }
            PositionedElement::Group(group) => {
                let child_acc = group.transform.then(acc);
                collect_run_candidates_transformed(&group.children, child_acc, candidates);
            }
            PositionedElement::MarkedContent { children, .. } => {
                collect_run_candidates_transformed(children, acc, candidates);
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

fn project_glyph_run(run: &GlyphRun, acc: Transform) -> Option<RunCandidate> {
    let unscaled_advance: f64 = run.advances.iter().sum();
    let p0 = acc.apply(run.origin);
    let p1 = acc.apply(Point {
        x: run.origin.x + unscaled_advance,
        y: run.origin.y,
    });
    let advance = (p1.x - p0.x).abs();
    let scale_y = (acc.b * acc.b + acc.d * acc.d).sqrt();
    let font_size = if scale_y.is_finite() && scale_y > 0.0 {
        run.font_size * scale_y
    } else {
        run.font_size
    };
    Some(RunCandidate {
        node: run.source.map(|span| span.node.get()),
        char_start: run.source.map(|span| span.char_start).unwrap_or(0),
        origin: p0,
        font_size,
        advance,
        text: run.text.clone(),
    })
}

fn project_multilingual(run: &MultilingualGlyphRun, acc: Transform) -> Option<RunCandidate> {
    let unscaled_advance: f64 = run.x_advances.iter().sum();
    let p0 = acc.apply(run.origin);
    let p1 = acc.apply(Point {
        x: run.origin.x + unscaled_advance,
        y: run.origin.y,
    });
    let advance = (p1.x - p0.x).abs();
    let scale_y = (acc.b * acc.b + acc.d * acc.d).sqrt();
    let font_size = if scale_y.is_finite() && scale_y > 0.0 {
        run.font_size * scale_y
    } else {
        run.font_size
    };
    Some(RunCandidate {
        node: run.source.map(|span| span.node.get()),
        char_start: run.source.map(|span| span.char_start).unwrap_or(0),
        origin: p0,
        font_size,
        advance,
        text: run.logical_text.clone(),
    })
}

#[derive(PartialEq, Clone, Copy)]
struct LineKey {
    node: Option<u32>,
    band: i64,
}

struct LineBuilder {
    key: LineKey,
    text: String,
    bytes: usize,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    last_end_x: Option<f64>,
    last_font_size: f64,
}

impl LineBuilder {
    fn new(key: LineKey) -> Self {
        Self {
            key,
            text: String::new(),
            bytes: 0,
            x_min: f64::INFINITY,
            x_max: f64::NEG_INFINITY,
            y_min: f64::INFINITY,
            y_max: f64::NEG_INFINITY,
            last_end_x: None,
            last_font_size: 0.0,
        }
    }

    /// Same-node runs continue a line only in char order; different nodes
    /// always start a new line even on the same band.
    fn node_continuous(&self, run: &RunCandidate) -> bool {
        self.key.node.is_none() || self.key.node == run.node
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
                self.bytes += 1;
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
        self.bytes += run.text.len();
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
    use oxml_layout::{Color, FontId, SourceNodeId, SourceSpan};

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
        let lines = build_page_lines(&elements, &DocxLimits::default());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
    }

    #[test]
    fn wide_joins_insert_spaces_and_baseline_runs_sort_by_x() {
        let elements = vec![
            PositionedElement::Text(glyph_run("second", Point { x: 200.0, y: 50.0 }, None)),
            PositionedElement::Text(glyph_run("first", Point { x: 0.0, y: 50.0 }, None)),
        ];
        let lines = build_page_lines(&elements, &DocxLimits::default());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "first second");
    }

    #[test]
    fn different_baselines_split_lines() {
        let elements = vec![
            PositionedElement::Text(glyph_run("top", Point { x: 0.0, y: 20.0 }, None)),
            PositionedElement::Text(glyph_run("bottom", Point { x: 0.0, y: 60.0 }, None)),
        ];
        let lines = build_page_lines(&elements, &DocxLimits::default());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "top");
        assert_eq!(lines[1].text, "bottom");
    }

    #[test]
    fn corpus_implements_the_contract_and_bounds_are_emu() {
        let elements = vec![PositionedElement::Text(glyph_run(
            "word",
            Point { x: 10.0, y: 42.0 },
            None,
        ))];
        let pages = vec![Arc::new(PageFrame::new(1, 612.0, 792.0, elements))];
        let corpus = DocxSearchCorpus::build(&pages, &DocxLimits::default());
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
        let lines = build_page_lines(&elements, &DocxLimits::default());
        assert_eq!(lines.len(), 50);
        let capped = build_page_lines(
            &elements,
            &DocxLimits {
                max_corpus_lines_per_page: 10,
                ..DocxLimits::default()
            },
        );
        assert_eq!(capped.len(), 10);
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
        let lines = build_page_lines(&elements, &DocxLimits::default());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "group text");
        assert_eq!(lines[0].rect.x, pt_to_emu(110.0));
    }
}
