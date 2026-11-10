use pptx_model::Slide;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// Extracted search metadata for a single presentation slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideSearchEntry {
    pub slide_index: usize,
    pub slide_number: usize,
    pub title: String,
    pub notes: String,
}

/// Interactive state and fuzzy search indexing for the Table of Contents modal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchState {
    query: String,
    matched_indices: Vec<usize>,
    selected_cursor: usize,
    entries: Vec<SlideSearchEntry>,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matched_indices(&self) -> &[usize] {
        &self.matched_indices
    }

    pub fn selected_cursor(&self) -> usize {
        self.selected_cursor
    }

    pub fn entries(&self) -> &[SlideSearchEntry] {
        &self.entries
    }

    pub fn entry_by_slide_index(&self, slide_idx: usize) -> Option<&SlideSearchEntry> {
        self.entries.get(slide_idx)
    }

    /// Return the currently highlighted slide index (0-based) from matched search results.
    pub fn selected_slide_index(&self) -> Option<usize> {
        self.matched_indices.get(self.selected_cursor).copied()
    }

    /// Set the cursor to a specific match index, clamping to valid range.
    pub fn set_selected_cursor(&mut self, cursor: usize) {
        if self.matched_indices.is_empty() {
            self.selected_cursor = 0;
        } else {
            self.selected_cursor = cursor.min(self.matched_indices.len().saturating_sub(1));
        }
    }

    /// Append a character to the search query and recompute filtered matches.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.recompute_matches();
        self.selected_cursor = 0;
    }

    /// Delete the last character in the search query and recompute filtered matches.
    pub fn backspace(&mut self) {
        if self.query.pop().is_some() {
            self.recompute_matches();
            self.selected_cursor = 0;
        }
    }

    /// Clear the search query string and reset match results.
    pub fn clear_query(&mut self) {
        self.query.clear();
        self.recompute_matches();
        self.selected_cursor = 0;
    }

    /// Move selection cursor up in the matched results list.
    pub fn select_up(&mut self) {
        self.selected_cursor = self.selected_cursor.saturating_sub(1);
    }

    /// Move selection cursor down in the matched results list.
    pub fn select_down(&mut self) {
        if !self.matched_indices.is_empty() && self.selected_cursor + 1 < self.matched_indices.len() {
            self.selected_cursor += 1;
        }
    }

    /// Populate search entries from slide models and speaker notes.
    pub fn sync_entries(
        &mut self,
        slides: &[Slide],
        notes: &[Option<String>],
        total_slides: usize,
    ) {
        self.entries.clear();
        for i in 0..total_slides {
            let slide_number = i + 1;
            let title = if let Some(slide) = slides.get(i) {
                extract_slide_title(slide)
            } else {
                format!("Slide {}", slide_number)
            };

            let note_text = notes.get(i)
                .and_then(|n| n.as_deref())
                .unwrap_or("")
                .to_string();

            self.entries.push(SlideSearchEntry {
                slide_index: i,
                slide_number,
                title,
                notes: note_text,
            });
        }
        self.recompute_matches();
    }

    /// Re-filter and rank slide entries matching the active query.
    pub fn recompute_matches(&mut self) {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            self.matched_indices = (0..self.entries.len()).collect();
            return;
        }

        let mut scored_matches: Vec<(usize, u32)> = Vec::new();

        for entry in &self.entries {
            let score = calculate_match_score(&q, entry);
            if score > 0 {
                scored_matches.push((entry.slide_index, score));
            }
        }

        // Sort descending by score; on tie, preserve ascending slide index
        scored_matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.matched_indices = scored_matches.into_iter().map(|(idx, _)| idx).collect();
    }
}

/// PresentationState extensions for Fuzzy TOC Search.
impl PresentationState {
    pub fn search_state(&self) -> &SearchState {
        &self.search
    }

    pub fn search_state_mut(&mut self) -> &mut SearchState {
        &mut self.search
    }

    pub fn toggle_search(&mut self) {
        if self.mode() == AppMode::TocSearchModal {
            self.exit_search(false);
        } else {
            self.open_search();
        }
    }

    pub fn open_search(&mut self) {
        let total = self.total_slides();
        let slides = self.slides.clone();
        let notes = self.notes.clone();
        self.search.sync_entries(&slides, &notes, total);
        self.search.clear_query();
        self.search.set_selected_cursor(self.current_slide());
        self.enter_overlay(AppMode::TocSearchModal);
    }

    pub fn exit_search(&mut self, apply_selection: bool) -> bool {
        if self.mode() != AppMode::TocSearchModal {
            return false;
        }
        let mut slide_changed = false;
        if apply_selection {
            if let Some(target) = self.search.selected_slide_index() {
                slide_changed = self.set_current_slide(target);
            }
        }
        self.exit_overlay();
        slide_changed
    }

    pub fn search_push_char(&mut self, ch: char) {
        self.search.push_char(ch);
    }

    pub fn search_backspace(&mut self) {
        self.search.backspace();
    }

    pub fn search_up(&mut self) {
        self.search.select_up();
    }

    pub fn search_down(&mut self) {
        self.search.select_down();
    }
}

/// Calculate relevance score for matching query against slide number, title, and notes.
fn calculate_match_score(query: &str, entry: &SlideSearchEntry) -> u32 {
    let num_str = entry.slide_number.to_string();
    if num_str == query {
        return 1000;
    }
    if num_str.starts_with(query) {
        return 800;
    }

    let title_lower = entry.title.to_lowercase();
    if title_lower == query {
        return 700;
    }
    if title_lower.starts_with(query) {
        return 600;
    }
    if title_lower.contains(query) {
        return 500;
    }
    if fuzzy_subsequence(query, &title_lower) {
        return 300;
    }

    let notes_lower = entry.notes.to_lowercase();
    if notes_lower.contains(query) {
        return 200;
    }
    if fuzzy_subsequence(query, &notes_lower) {
        return 100;
    }

    0
}

/// Check if characters of `query` appear sequentially in `target`.
fn fuzzy_subsequence(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for q_char in query.chars() {
        if !target_chars.any(|t_char| t_char == q_char) {
            return false;
        }
    }
    true
}

/// Extract human-readable title from slide elements or placeholders.
pub fn extract_slide_title(slide: &Slide) -> String {
    // 1. Look for placeholder title shapes
    for elem in &slide.elements {
        if let pptx_model::SlideElement::Shape(shape) = elem {
            if let Some(ref ph) = shape.placeholder_type {
                if ph == "title" || ph == "ctrTitle" {
                    let text = extract_shape_text(shape);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
        }
    }

    // 2. Look for subtitle placeholder
    for elem in &slide.elements {
        if let pptx_model::SlideElement::Shape(shape) = elem {
            if let Some(ref ph) = shape.placeholder_type {
                if ph == "subTitle" {
                    let text = extract_shape_text(shape);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
        }
    }

    // 3. Fallback: first shape with non-empty text
    for elem in &slide.elements {
        if let pptx_model::SlideElement::Shape(shape) = elem {
            let text = extract_shape_text(shape);
            if !text.is_empty() {
                return text;
            }
        }
    }

    format!("Slide {}", slide.slide_number)
}

fn extract_shape_text(shape: &pptx_model::ShapeElement) -> String {
    let mut text = String::new();
    if let Some(ref body) = shape.text_body {
        for para in &body.paragraphs {
            for run in &para.runs {
                if let pptx_model::TextRun::Text(ref data) = run {
                    text.push_str(&data.text);
                }
            }
        }
    }
    text.trim().to_string()
}
