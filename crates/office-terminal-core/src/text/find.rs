//! Run-level find over slide text (V2).
//!
//! Ports the shared search contract (`core/src/viewer/find-controller.ts` +
//! `search/text-index.ts`) onto the PowerPoint model. A match may span runs of
//! one text body but never a table-cell boundary; case folding is
//! length-preserving and matches non-overlapping in document order.

use crate::contracts::VectorSlide;
use pptx_model::{Presentation, Slide, SlideElement, SlideElementOrigin, TextRun};

/// One rendered text run participating in search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindRun {
    pub text: String,
    pub element_index: usize,
    pub origin: &'static str,
    pub shape_id: Option<String>,
    /// `(row, column)` when the run lives in a table cell.
    pub table_cell: Option<(usize, usize)>,
}

/// A matched range within one run, in character offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindSlice {
    pub run_index: usize,
    pub start: usize,
    pub end: usize,
}

/// One document match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub match_index: usize,
    pub slide: usize,
    pub text: String,
    pub slices: Vec<FindSlice>,
}

/// Highlight slices for one slide; the active match is flagged distinctly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindHighlight {
    pub slices: Vec<FindSlice>,
    pub active: bool,
}

/// Search state for a sequence of slides.
#[derive(Clone, Debug, Default)]
pub struct FindController {
    slides: Vec<Vec<FindRun>>,
    hidden: Vec<bool>,
    skip_hidden: bool,
    matches: Vec<FindMatch>,
    active: Option<usize>,
}

impl FindController {
    /// Build the run index from a whole presentation.
    pub fn from_presentation(presentation: &Presentation) -> Self {
        let slides = presentation
            .slides
            .iter()
            .map(slide_runs)
            .collect::<Vec<_>>();
        let hidden = presentation.slides.iter().map(|slide| slide.hidden).collect();
        Self {
            slides,
            hidden,
            skip_hidden: false,
            matches: Vec::new(),
            active: None,
        }
    }

    /// Build from a slide slice plus an index-aligned hidden mask.
    pub fn from_slides(slides: &[Slide], hidden: &[bool]) -> Self {
        Self {
            slides: slides.iter().map(slide_runs).collect(),
            hidden: (0..slides.len())
                .map(|index| hidden.get(index).copied().unwrap_or(false))
                .collect(),
            skip_hidden: false,
            matches: Vec::new(),
            active: None,
        }
    }

    /// Exclude `<p:sld show="0">` slides from matching (Skip mode).
    pub fn set_skip_hidden(&mut self, skip: bool) {
        self.skip_hidden = skip;
    }

    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }

    pub fn runs(&self, slide: usize) -> Option<&[FindRun]> {
        self.slides.get(slide).map(Vec::as_slice)
    }

    /// Replace one slide's run list (a fresh render publishes newer geometry).
    pub fn set_slide_runs(&mut self, slide: usize, runs: Vec<FindRun>) {
        if slide >= self.slides.len() {
            self.slides.resize_with(slide + 1, Vec::new);
            self.hidden.resize(slide + 1, false);
        }
        self.slides[slide] = runs;
        self.matches.clear();
        self.active = None;
    }

    /// Clear all matches and the active cursor, as an empty query does.
    pub fn clear(&mut self) {
        self.matches.clear();
        self.active = None;
    }

    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    /// Find every non-overlapping occurrence (case-insensitive by default).
    pub fn find(&mut self, query: &str, case_sensitive: bool) -> Vec<FindMatch> {
        self.matches.clear();
        self.active = None;
        if query.is_empty() {
            return Vec::new();
        }
        let query: Vec<char> = if case_sensitive {
            query.chars().collect()
        } else {
            query.chars().map(fold_char).collect()
        };
        if query.is_empty() {
            return Vec::new();
        }
        for slide in 0..self.slides.len() {
            if self.skip_hidden && self.hidden.get(slide).copied().unwrap_or(false) {
                continue;
            }
            let runs = &self.slides[slide];
            for group in run_groups(runs) {
                let mut flat = Vec::new();
                let mut positions = Vec::new();
                for run_index in group {
                    for (char_index, ch) in runs[run_index].text.chars().enumerate() {
                        flat.push(if case_sensitive { ch } else { fold_char(ch) });
                        positions.push((run_index, char_index));
                    }
                }
                if flat.len() < query.len() {
                    continue;
                }
                let mut start = 0usize;
                while start + query.len() <= flat.len() {
                    if flat[start..start + query.len()] == query[..] {
                        let slices = slice_match(&positions, start, start + query.len(), runs);
                        if !slices.is_empty() {
                            let text = slices
                                .iter()
                                .map(|slice| {
                                    runs[slice.run_index]
                                        .text
                                        .chars()
                                        .skip(slice.start)
                                        .take(slice.end - slice.start)
                                        .collect::<String>()
                                })
                                .collect::<String>();
                            let match_index = self.matches.len();
                            self.matches.push(FindMatch {
                                match_index,
                                slide,
                                text,
                                slices,
                            });
                        }
                        start += query.len();
                    } else {
                        start += 1;
                    }
                }
            }
        }
        self.matches.clone()
    }

    /// Advance the active match with wrap-around; none starts at the first.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<FindMatch> {
        if self.matches.is_empty() {
            self.active = None;
            return None;
        }
        self.active = Some(match self.active {
            None => 0,
            Some(index) => (index + 1) % self.matches.len(),
        });
        self.active.and_then(|index| self.matches.get(index).cloned())
    }

    /// Step back with wrap-around; none starts at the last match.
    pub fn prev(&mut self) -> Option<FindMatch> {
        if self.matches.is_empty() {
            self.active = None;
            return None;
        }
        self.active = Some(match self.active {
            None => self.matches.len() - 1,
            Some(0) => self.matches.len() - 1,
            Some(index) => index - 1,
        });
        self.active.and_then(|index| self.matches.get(index).cloned())
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active_slide(&self) -> Option<usize> {
        self.active
            .and_then(|index| self.matches.get(index))
            .map(|matched| matched.slide)
    }

    /// Highlight slices for `slide`, with the active match flagged.
    pub fn highlights(&self, slide: usize) -> Vec<FindHighlight> {
        self.matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| matched.slide == slide)
            .map(|(index, matched)| FindHighlight {
                slices: matched.slices.clone(),
                active: self.active == Some(index),
            })
            .collect()
    }
}

/// Groups of run indices that may join across run boundaries: one shape body
/// is one group, each table cell is its own group.
type GroupKey = (usize, Option<(usize, usize)>);

fn run_groups(runs: &[FindRun]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(GroupKey, Vec<usize>)> = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        let key = (run.element_index, run.table_cell);
        if let Some((last_key, members)) = groups.last_mut() {
            if *last_key == key {
                members.push(index);
                continue;
            }
        }
        groups.push((key, vec![index]));
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

fn slice_match(
    positions: &[(usize, usize)],
    start: usize,
    end: usize,
    runs: &[FindRun],
) -> Vec<FindSlice> {
    let mut slices: Vec<FindSlice> = Vec::new();
    for &(run_index, char_index) in &positions[start..end] {
        match slices.last_mut() {
            Some(last) if last.run_index == run_index && last.end == char_index => {
                last.end += 1;
            }
            _ => slices.push(FindSlice {
                run_index,
                start: char_index,
                end: char_index + 1,
            }),
        }
    }
    slices.retain(|slice| slice.start < runs[slice.run_index].text.chars().count());
    slices
}

fn fold_char(ch: char) -> char {
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(folded), None) => folded,
        _ => ch,
    }
}

fn origin_name(origin: SlideElementOrigin) -> &'static str {
    match origin {
        SlideElementOrigin::Master => "master",
        SlideElementOrigin::Layout => "layout",
        SlideElementOrigin::Slide => "slide",
    }
}

fn push_body_runs(
    out: &mut Vec<FindRun>,
    body: Option<&pptx_model::TextBody>,
    element_index: usize,
    origin: &'static str,
    shape_id: Option<String>,
    table_cell: Option<(usize, usize)>,
) {
    let Some(body) = body else {
        return;
    };
    for paragraph in &body.paragraphs {
        for run in &paragraph.runs {
            let text = match run {
                TextRun::Text(data) => data.text.clone(),
                TextRun::Break { .. } => "\n".to_string(),
                TextRun::Math { .. } => "[equation]".to_string(),
            };
            if text.is_empty() {
                continue;
            }
            out.push(FindRun {
                text,
                element_index,
                origin,
                shape_id: shape_id.clone(),
                table_cell,
            });
        }
    }
}

/// Flatten one slide's searchable text in paint order.
pub fn slide_runs(slide: &Slide) -> Vec<FindRun> {
    let mut runs = Vec::new();
    for (element_index, element) in slide.elements.iter().enumerate() {
        let origin = slide
            .element_sources
            .get(element_index)
            .map(|source| origin_name(source.origin))
            .unwrap_or("unknown");
        match element {
            SlideElement::Shape(shape) => push_body_runs(
                &mut runs,
                shape.text_body.as_ref(),
                element_index,
                origin,
                shape.id.clone(),
                None,
            ),
            SlideElement::Table(table) => {
                for (row_index, row) in table.rows.iter().enumerate() {
                    for (column_index, cell) in row.cells.iter().enumerate() {
                        push_body_runs(
                            &mut runs,
                            cell.text_body.as_ref(),
                            element_index,
                            origin,
                            table.id.clone(),
                            Some((row_index, column_index)),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    runs
}

/// Convenience for a compiled scene: its text runs carry no container identity
/// (that lives in the model), so search should be built from `Slide`.
pub fn scene_has_text(scene: &VectorSlide) -> bool {
    scene
        .commands
        .iter()
        .any(|command| matches!(command, crate::contracts::VectorCommand::DrawText { .. }))
}
