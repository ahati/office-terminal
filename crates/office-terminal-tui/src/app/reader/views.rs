//! Per-page view state for [`ReaderState`].
//!
//! The views map (`page -> PageView`) is a lazily-populated LRU-ish cache:
//! unseen pages resolve to a default view derived from the reader's default
//! fit/zoom, and zoom adjustments fan out across every cached view.

use crate::app::reader_state::ReaderState;
use crate::app::reader_types::{PageView, ReaderFitMode};

impl ReaderState {
    pub fn page_view(&self, page: usize) -> PageView {
        self.views.get(&page).cloned().unwrap_or(PageView {
            fit: self.default_fit,
            zoom: self.default_zoom,
            scroll_x: 0,
            scroll_y: 0,
        })
    }

    pub fn page_view_mut(&mut self, page: usize) -> &mut PageView {
        let def_fit = self.default_fit;
        let def_zoom = self.default_zoom;
        self.views.entry(page).or_insert(PageView {
            fit: def_fit,
            zoom: def_zoom,
            scroll_x: 0,
            scroll_y: 0,
        })
    }

    pub fn cycle_fit(&mut self) {
        let targets = self.visible_pages();

        for page in targets {
            let view = self.page_view_mut(page);
            view.fit = match view.fit {
                ReaderFitMode::FitWidth => ReaderFitMode::FitPage,
                ReaderFitMode::FitPage => ReaderFitMode::FitWidth,
            };
        }
    }

    pub fn adjust_zoom(&mut self, factor: f32) {
        let new_zoom = (self.default_zoom * factor).clamp(0.1, 4.0);
        self.default_zoom = new_zoom;
        for view in self.views.values_mut() {
            view.zoom = new_zoom;
        }
        for page in self.visible_pages() {
            let view = self.page_view_mut(page);
            view.zoom = new_zoom;
        }
    }

    pub fn reset_view(&mut self) {
        self.default_zoom = 1.0;
        self.scroll_px = 0;
        self.snap_display();
        self.set_row_scroll_x(0);
        for view in self.views.values_mut() {
            view.zoom = 1.0;
            view.scroll_x = 0;
            view.scroll_y = 0;
        }
        for page in self.visible_pages() {
            let view = self.page_view_mut(page);
            view.zoom = 1.0;
            view.scroll_x = 0;
            view.scroll_y = 0;
        }
    }

    pub fn apply_zoom_all(&mut self) {
        let page = self.focused_page();
        let view = self.page_view(page);
        self.default_zoom = view.zoom;
        self.default_fit = view.fit;
        self.views.clear();
    }
}
