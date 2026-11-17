use pptx_model::Slide;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// Extracted search metadata for a single presentation slide or outline node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideSearchEntry {
    pub slide_index: usize,
    pub slide_number: usize,
    pub title: String,
    pub notes: String,
    pub level: usize,
    pub expanded: bool,
    pub has_children: bool,
}

/// Interactive state and fuzzy search indexing for the Table of Contents modal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchState {
    query: String,
    matched_indices: Vec<usize>,
    selected_cursor: usize,
    entries: Vec<SlideSearchEntry>,
    /// Per-slide `<p:sld show="0">` flags, index-aligned with `entries`.
    hidden: Vec<bool>,
    skip_hidden: bool,
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

    pub fn entry(&self, entry_idx: usize) -> Option<&SlideSearchEntry> {
        self.entries.get(entry_idx)
    }

    pub fn entry_by_slide_index(&self, slide_idx: usize) -> Option<&SlideSearchEntry> {
        self.entries.iter().find(|e| e.slide_index == slide_idx)
    }

    /// Return the currently highlighted slide index (0-based) from matched search results.
    pub fn selected_slide_index(&self) -> Option<usize> {
        self.matched_indices
            .get(self.selected_cursor)
            .and_then(|&entry_idx| self.entries.get(entry_idx).map(|e| e.slide_index))
    }

    pub fn expand_selected(&mut self) -> bool {
        if let Some(&entry_idx) = self.matched_indices.get(self.selected_cursor) {
            if let Some(entry) = self.entries.get_mut(entry_idx) {
                if entry.has_children && !entry.expanded {
                    entry.expanded = true;
                    self.recompute_matches();
                    return true;
                }
            }
        }
        false
    }

    pub fn collapse_selected(&mut self) -> bool {
        if let Some(&entry_idx) = self.matched_indices.get(self.selected_cursor) {
            if let Some(entry) = self.entries.get_mut(entry_idx) {
                if entry.has_children && entry.expanded {
                    entry.expanded = false;
                    self.recompute_matches();
                    return true;
                }
            }
            let cur_level = self.entries.get(entry_idx).map(|e| e.level).unwrap_or(0);
            if cur_level > 0 {
                for i in (0..entry_idx).rev() {
                    if self.entries[i].level < cur_level {
                        if let Some(pos) = self.matched_indices.iter().position(|&idx| idx == i) {
                            self.selected_cursor = pos;
                            return true;
                        }
                    }
                }
            }
        }
        false
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
        self.sync_entries_filtered(slides, notes, total_slides, &[], false);
    }

    /// Populate the search index with the hidden-slide policy applied: in
    /// `skip` mode hidden slides never match (ref viewer.ts:971-975).
    pub fn sync_entries_filtered(
        &mut self,
        slides: &[Slide],
        notes: &[Option<String>],
        total_slides: usize,
        hidden: &[bool],
        skip_hidden: bool,
    ) {
        self.hidden = hidden.to_vec();
        // An all-hidden deck degenerates to `show`, so search matches every
        // slide again (ref viewer-hidden-skip.test.ts:50-60).
        self.skip_hidden = skip_hidden && hidden.iter().any(|flag| !flag);
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
                level: 0,
                expanded: true,
                has_children: false,
            });
        }
        self.recompute_matches();
    }

    /// Populate TOC entries from document outlines / bookmarks.
    pub fn sync_from_toc(&mut self, toc: &[kitty_pptx_core::TocEntry], total_pages: usize) {
        let existing_expanded: std::collections::HashMap<(usize, String), bool> = self
            .entries
            .iter()
            .map(|e| ((e.slide_index, e.title.clone()), e.expanded))
            .collect();

        self.hidden.clear();
        self.skip_hidden = false;
        self.entries.clear();

        if toc.is_empty() {
            for i in 0..total_pages {
                let title = format!("Page {}", i + 1);
                let expanded = existing_expanded
                    .get(&(i, title.clone()))
                    .copied()
                    .unwrap_or(true);
                self.entries.push(SlideSearchEntry {
                    slide_index: i,
                    slide_number: i + 1,
                    title,
                    notes: String::new(),
                    level: 0,
                    expanded,
                    has_children: false,
                });
            }
        } else {
            for (idx, t) in toc.iter().enumerate() {
                let has_children = toc
                    .get(idx + 1)
                    .map(|next| next.level > t.level)
                    .unwrap_or(false);
                let expanded = existing_expanded
                    .get(&(t.page_index, t.title.clone()))
                    .copied()
                    .unwrap_or(true);
                self.entries.push(SlideSearchEntry {
                    slide_index: t.page_index,
                    slide_number: t.page_index + 1,
                    title: t.title.clone(),
                    notes: String::new(),
                    level: t.level,
                    expanded,
                    has_children,
                });
            }
        }
        self.recompute_matches();
    }

    /// Re-filter and rank slide entries matching the active query.
    pub fn recompute_matches(&mut self) {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            let mut visible_flags = vec![true; self.entries.len()];
            let mut ancestor_stack: Vec<(usize, bool)> = Vec::new();
            for (idx, entry) in self.entries.iter().enumerate() {
                while let Some(&(lvl, _)) = ancestor_stack.last() {
                    if lvl >= entry.level {
                        ancestor_stack.pop();
                    } else {
                        break;
                    }
                }
                let all_ancestors_expanded = ancestor_stack.iter().all(|&(_, exp)| exp);
                visible_flags[idx] = all_ancestors_expanded;
                if entry.has_children {
                    ancestor_stack.push((entry.level, entry.expanded));
                }
            }

            self.matched_indices = (0..self.entries.len())
                .filter(|&index| visible_flags[index] && !self.is_excluded(self.entries[index].slide_index))
                .collect();
            return;
        }

        let has_hierarchy = self.entries.iter().any(|e| e.level > 0);
        if !has_hierarchy {
            let mut scored: Vec<(usize, u32)> = Vec::new();
            for (idx, entry) in self.entries.iter().enumerate() {
                if self.is_excluded(entry.slide_index) {
                    continue;
                }
                let score = calculate_match_score(&q, entry);
                if score > 0 {
                    scored.push((idx, score));
                }
            }
            scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            self.matched_indices = scored.into_iter().map(|(idx, _)| idx).collect();
            return;
        }

        // When searching a hierarchical tree, find direct matches and their ancestors
        let mut matched_set = std::collections::HashSet::new();
        for (idx, entry) in self.entries.iter().enumerate() {
            if self.is_excluded(entry.slide_index) {
                continue;
            }
            let score = calculate_match_score(&q, entry);
            if score > 0 {
                matched_set.insert(idx);

                // Add ancestors for context
                let mut cur_level = entry.level;
                for parent_idx in (0..idx).rev() {
                    if self.entries[parent_idx].level < cur_level {
                        matched_set.insert(parent_idx);
                        cur_level = self.entries[parent_idx].level;
                        if cur_level == 0 {
                            break;
                        }
                    }
                }
            }
        }

        self.matched_indices = (0..self.entries.len())
            .filter(|idx| matched_set.contains(idx))
            .collect();
    }

    /// Whether the hidden-slide policy excludes this slide from matches.
    fn is_excluded(&self, slide_index: usize) -> bool {
        self.skip_hidden && self.hidden.get(slide_index).copied().unwrap_or(false)
    }

    /// Drop the current result set and active match without touching the query.
    /// Entering or leaving `skip` mode does this (`_invalidateFind`).
    // ref viewer.ts:545-554
    pub fn clear_matches(&mut self) {
        self.matched_indices.clear();
        self.selected_cursor = 0;
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
        self.sync_search_entries();
        self.search.clear_query();
        if let Some(pos) = self.search.matched_indices().iter().position(|&idx| {
            self.search.entry(idx).map(|e| e.slide_index == self.current_slide()).unwrap_or(false)
        }) {
            self.search.set_selected_cursor(pos);
        } else {
            self.search.set_selected_cursor(0);
        }
        self.enter_overlay(AppMode::TocSearchModal);
    }

    pub fn search_expand(&mut self) -> bool {
        self.search.expand_selected()
    }

    pub fn search_collapse(&mut self) -> bool {
        self.search.collapse_selected()
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
