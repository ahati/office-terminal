//! In-document find state (`Ctrl+F`, V2).
//!
//! The core [`FindController`](office_terminal_core::text::find::FindController)
//! owns the run index and match arithmetic; this module owns the modal query,
//! the selected match and the hidden-slide policy.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use office_terminal_core::text::find::{FindController, FindMatch};
use pptx_model::Slide;

use crate::app::events::AppAction;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

#[derive(Clone, Debug, Default)]
pub struct FindState {
    controller: FindController,
    query: String,
    matches: Vec<FindMatch>,
    selected: usize,
    open: bool,
}

impl FindState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_slide(&self) -> Option<usize> {
        self.matches.get(self.selected).map(|matched| matched.slide)
    }

    pub fn sync(&mut self, slides: &[Slide], hidden: &[bool], skip_hidden: bool) {
        self.controller = FindController::from_slides(slides, hidden);
        self.controller.set_skip_hidden(skip_hidden);
        self.recompute();
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.matches.clear();
        self.selected = 0;
        self.controller.clear();
    }

    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.recompute();
    }

    pub fn backspace(&mut self) {
        if self.query.pop().is_some() {
            self.recompute();
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        self.selected = (self.selected + 1) % self.matches.len();
        self.controller.next();
        self.selected_slide()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        self.selected = (self.selected + self.matches.len() - 1) % self.matches.len();
        self.controller.prev();
        self.selected_slide()
    }

    fn recompute(&mut self) {
        self.matches = self.controller.find(&self.query, false);
        self.selected = 0;
        if !self.matches.is_empty() {
            self.controller.next();
        }
    }
}

impl PresentationState {
    /// Rebuild the content-find index from the current slides and hidden mask.
    pub fn sync_find_index(&mut self) {
        let hidden: Vec<bool> = (0..self.total_slides).map(|index| self.nav.is_hidden(index)).collect();
        self.find
            .sync(self.deck.slides(), &hidden, self.nav.hidden_mode().is_skip());
    }

    pub fn open_find(&mut self) {
        self.find.open();
        self.enter_overlay(AppMode::FindModal);
    }

    pub fn close_find(&mut self) {
        self.find.close();
        if self.mode() == AppMode::FindModal {
            self.exit_overlay();
        }
    }

    pub fn find_state(&self) -> &FindState {
        &self.find
    }

    pub fn find_push_char(&mut self, ch: char) {
        self.find.push_char(ch);
    }

    pub fn find_backspace(&mut self) {
        self.find.backspace();
    }

    /// Move the active match and jump to its slide. Returns whether the slide moved.
    pub fn find_next(&mut self) -> bool {
        self.find
            .next()
            .map(|slide| self.set_current_slide(slide))
            .unwrap_or(false)
    }

    pub fn find_prev(&mut self) -> bool {
        self.find
            .prev()
            .map(|slide| self.set_current_slide(slide))
            .unwrap_or(false)
    }

    /// Commit the selected match, close the modal and report a slide change.
    pub fn find_accept(&mut self) -> bool {
        let changed = self
            .find
            .selected_slide()
            .map(|slide| self.set_current_slide(slide))
            .unwrap_or(false);
        self.close_find();
        changed
    }
}

/// Handle keys while the find modal is open.
pub(crate) fn handle_find_key(key: KeyEvent, state: &mut PresentationState) -> AppAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
        state.close_find();
        return AppAction::Redraw;
    }
    match key.code {
        KeyCode::Esc => {
            state.close_find();
            AppAction::Redraw
        }
        KeyCode::Enter => {
            if state.find_accept() {
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Up => {
            if state.find_prev() {
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Down => {
            if state.find_next() {
                AppAction::SlideChanged
            } else {
                AppAction::Redraw
            }
        }
        KeyCode::Backspace => {
            state.find_backspace();
            AppAction::Redraw
        }
        KeyCode::Char(ch) => {
            state.find_push_char(ch);
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}
