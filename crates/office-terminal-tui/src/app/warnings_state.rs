use office_terminal_core::WarningEvent;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

/// Fidelity warning attached to a presentation slide or general session.
#[derive(Clone, Debug, PartialEq)]
pub struct SlideWarning {
    pub slide_index: Option<usize>,
    pub event: WarningEvent,
}

/// State tracking fidelity warnings and drawer scrolling.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WarningsState {
    warnings: Vec<SlideWarning>,
    scroll: usize,
}

impl WarningsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fidelity warning event, optionally associated with a specific slide.
    ///
    /// Idempotent per `(slide, event)`: resizing/zooming recompiles the scene,
    /// and a warning must not accumulate duplicates in the drawer.
    pub fn add_warning(&mut self, slide_index: Option<usize>, event: WarningEvent) {
        let warning = SlideWarning { slide_index, event };
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// Set warnings directly.
    pub fn set_warnings(&mut self, warnings: Vec<SlideWarning>) {
        self.warnings = warnings;
        self.scroll = 0;
    }

    /// List all registered warnings in the presentation session.
    pub fn warnings(&self) -> &[SlideWarning] {
        &self.warnings
    }

    /// Filter warnings relevant to a specific slide index or general session.
    pub fn warnings_for_slide(&self, slide_index: usize) -> Vec<&SlideWarning> {
        self.warnings
            .iter()
            .filter(|w| w.slide_index.is_none_or(|idx| idx == slide_index))
            .collect()
    }

    /// Current scroll line offset in the drawer.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Scroll drawer content up.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll drawer content down.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Reset scroll offset to the top.
    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
    }

    /// Clear all warnings.
    pub fn clear(&mut self) {
        self.warnings.clear();
        self.scroll = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.warnings.len()
    }
}

/// PresentationState extensions for Fidelity Warnings.
impl PresentationState {
    pub fn warnings(&self) -> &[SlideWarning] {
        self.warnings.warnings()
    }

    pub fn warnings_state(&self) -> &WarningsState {
        &self.warnings
    }

    pub fn warnings_state_mut(&mut self) -> &mut WarningsState {
        &mut self.warnings
    }

    pub fn add_warning(&mut self, event: WarningEvent) {
        let slide = if self.total_slides() > 0 { Some(self.current_slide()) } else { None };
        self.warnings.add_warning(slide, event);
    }

    pub fn add_session_warning(&mut self, event: WarningEvent) {
        self.warnings.add_warning(None, event);
    }

    pub fn add_slide_warning(&mut self, slide_index: usize, event: WarningEvent) {
        self.warnings.add_warning(Some(slide_index), event);
    }

    pub fn set_warnings(&mut self, warnings: Vec<SlideWarning>) {
        self.warnings.set_warnings(warnings);
    }

    pub fn current_slide_warnings(&self) -> Vec<&SlideWarning> {
        self.warnings.warnings_for_slide(self.current_slide())
    }

    pub fn warnings_scroll(&self) -> usize {
        self.warnings.scroll()
    }

    pub fn scroll_warnings_up(&mut self) {
        self.warnings.scroll_up();
    }

    pub fn scroll_warnings_down(&mut self) {
        self.warnings.scroll_down();
    }

    pub fn toggle_warnings(&mut self) {
        if self.mode() == AppMode::WarningDrawer {
            self.close_warnings();
        } else {
            self.open_warnings();
        }
    }

    pub fn open_warnings(&mut self) {
        self.warnings.reset_scroll();
        self.enter_overlay(AppMode::WarningDrawer);
    }

    pub fn close_warnings(&mut self) {
        if self.mode() == AppMode::WarningDrawer {
            self.exit_overlay();
        }
    }
}
