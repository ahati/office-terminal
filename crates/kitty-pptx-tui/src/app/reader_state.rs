use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub use crate::app::reader_types::*;

/// Full interactive state model for Reader mode (PRD 14a §4).
#[derive(Clone, Debug)]
pub struct ReaderState {
    pub anchor: usize,
    pub layout: ReaderLayout,
    pub default_fit: ReaderFitMode,
    pub default_zoom: f32,
    pub focus: Slot,
    pub sync: bool,
    pub cover_alone: bool,
    pub scroll_px: i64,
    pub num_columns: Arc<AtomicUsize>,
    pub focused_slot: usize,
    pub row_scroll_x: Arc<AtomicI64>,
    pub max_row_scroll_x: Arc<AtomicI64>,
    views: HashMap<usize, PageView>,
    total_pages: usize,
    page_sizes: Vec<(f32, f32)>,
    placements: Arc<Mutex<Vec<ReaderSlotPlacement>>>,
}

impl ReaderState {
    pub fn new(total_pages: usize, page_sizes: Vec<(f32, f32)>) -> Self {
        Self {
            anchor: 0,
            layout: ReaderLayout::Spread,
            default_fit: ReaderFitMode::FitPage,
            default_zoom: 1.0,
            focus: Slot::Left,
            sync: true,
            cover_alone: false,
            scroll_px: 0,
            num_columns: Arc::new(AtomicUsize::new(2)),
            focused_slot: 0,
            row_scroll_x: Arc::new(AtomicI64::new(0)),
            max_row_scroll_x: Arc::new(AtomicI64::new(0)),
            views: HashMap::new(),
            total_pages,
            page_sizes,
            placements: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_shared_placements(
        mut self,
        placements: Arc<Mutex<Vec<ReaderSlotPlacement>>>,
    ) -> Self {
        self.placements = placements;
        self
    }

    pub fn total_pages(&self) -> usize {
        self.total_pages
    }

    pub fn page_size_pt(&self, page: usize) -> (f32, f32) {
        self.page_sizes.get(page).copied().unwrap_or((612.0, 792.0))
    }

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

    pub fn set_num_columns(&self, cols: usize) {
        self.num_columns.store(cols.max(1), Ordering::Relaxed);
    }

    pub fn num_columns(&self) -> usize {
        self.num_columns.load(Ordering::Relaxed)
    }

    pub fn effective_columns(&self) -> usize {
        match self.layout {
            ReaderLayout::Single => 1,
            ReaderLayout::Spread => self.num_columns(),
            ReaderLayout::Horizontal => self.num_columns(),
        }
    }

    pub fn row_scroll_x(&self) -> i64 {
        self.row_scroll_x.load(Ordering::Relaxed)
    }

    pub fn set_row_scroll_x(&self, val: i64) {
        let max_x = self.max_row_scroll_x.load(Ordering::Relaxed);
        let clamped = val.clamp(0, max_x.max(0));
        self.row_scroll_x.store(clamped, Ordering::Relaxed);
    }

    pub fn max_row_scroll_x(&self) -> i64 {
        self.max_row_scroll_x.load(Ordering::Relaxed)
    }

    pub fn set_max_row_scroll_x(&self, val: i64) {
        self.max_row_scroll_x.store(val.max(0), Ordering::Relaxed);
        let cur = self.row_scroll_x.load(Ordering::Relaxed);
        if cur > val.max(0) {
            self.row_scroll_x.store(val.max(0), Ordering::Relaxed);
        }
    }

    pub fn visible_pages(&self) -> Vec<usize> {
        if self.total_pages == 0 {
            return Vec::new();
        }
        match self.layout {
            ReaderLayout::Single => vec![self.anchor.min(self.total_pages - 1)],
            ReaderLayout::Spread => {
                let cols = self.effective_columns();
                if self.cover_alone && self.anchor == 0 {
                    vec![0]
                } else {
                    let mut pages = Vec::with_capacity(cols);
                    for i in 0..cols {
                        if self.anchor + i < self.total_pages {
                            pages.push(self.anchor + i);
                        }
                    }
                    if pages.is_empty() {
                        vec![self.anchor.min(self.total_pages - 1)]
                    } else {
                        pages
                    }
                }
            }
            ReaderLayout::Horizontal => {
                let cols = self.effective_columns();
                let mut pages = Vec::with_capacity(cols + 1);
                for i in 0..=cols {
                    if self.anchor + i < self.total_pages {
                        pages.push(self.anchor + i);
                    }
                }
                if pages.is_empty() {
                    vec![self.anchor.min(self.total_pages - 1)]
                } else {
                    pages
                }
            }
        }
    }

    pub fn focused_page(&self) -> usize {
        let vis = self.visible_pages();
        if vis.is_empty() {
            return 0;
        }
        if self.focused_slot < vis.len() {
            vis[self.focused_slot]
        } else {
            match self.focus {
                Slot::Left => vis[0],
                Slot::Right => *vis.get(1).unwrap_or(&vis[0]),
            }
        }
    }

    pub fn slot_width(&self, viewport_w_px: f32, cell_w_px: f32) -> f32 {
        let gutter_cols = 2u16;
        let gutter_px = (gutter_cols as f32) * cell_w_px;
        let cols = self.effective_columns();
        if cols > 1 {
            let total_gutters = (cols - 1) as f32 * gutter_px;
            ((viewport_w_px - total_gutters) / cols as f32).max(10.0)
        } else {
            viewport_w_px
        }
    }

    pub fn toggle_focus(&mut self) {
        let vis = self.visible_pages();
        if !vis.is_empty() {
            self.focused_slot = (self.focused_slot + 1) % vis.len();
            self.focus = if self.focused_slot.is_multiple_of(2) {
                Slot::Left
            } else {
                Slot::Right
            };
        }
    }

    pub fn set_sync(&mut self, sync: bool) {
        self.sync = sync;
    }

    pub fn cycle_fit(&mut self) {
        let targets = if self.sync {
            self.visible_pages()
        } else {
            vec![self.focused_page()]
        };

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
        if self.sync {
            for view in self.views.values_mut() {
                view.zoom = new_zoom;
            }
            for page in self.visible_pages() {
                let view = self.page_view_mut(page);
                view.zoom = new_zoom;
            }
        } else {
            let page = self.focused_page();
            let view = self.page_view_mut(page);
            view.zoom = (view.zoom * factor).clamp(0.1, 4.0);
        }
    }

    pub fn reset_view(&mut self) {
        self.default_zoom = 1.0;
        self.scroll_px = 0;
        self.set_row_scroll_x(0);
        if self.sync {
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
        } else {
            let page = self.focused_page();
            let def_zoom = self.default_zoom;
            let def_fit = self.default_fit;
            let view = self.page_view_mut(page);
            view.zoom = def_zoom;
            view.fit = def_fit;
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

    pub fn toggle_layout(&mut self) {
        self.layout = match self.layout {
            ReaderLayout::Single => ReaderLayout::Spread,
            ReaderLayout::Spread => ReaderLayout::Single,
            ReaderLayout::Horizontal => ReaderLayout::Spread,
        };
        self.scroll_px = 0;
        self.set_row_scroll_x(0);
        self.set_max_row_scroll_x(0);
    }

    pub fn toggle_horizontal(&mut self) {
        self.layout = if self.layout == ReaderLayout::Horizontal {
            ReaderLayout::Spread
        } else {
            ReaderLayout::Horizontal
        };
        self.scroll_px = 0;
        self.set_row_scroll_x(0);
        self.set_max_row_scroll_x(0);
    }

    pub fn toggle_cover_alone(&mut self) {
        self.cover_alone = !self.cover_alone;
        self.scroll_px = 0;
        self.set_row_scroll_x(0);
        self.set_max_row_scroll_x(0);
    }

    pub fn next_row(&mut self) {
        self.scroll_px = 0;
        if self.total_pages == 0 {
            return;
        }
        let step = self.effective_columns();
        match self.layout {
            ReaderLayout::Single => {
                self.anchor = (self.anchor + 1).min(self.total_pages - 1);
            }
            ReaderLayout::Spread => {
                if self.cover_alone && self.anchor == 0 {
                    self.anchor = 1.min(self.total_pages - 1);
                } else {
                    self.anchor = (self.anchor + step).min(self.total_pages.saturating_sub(1));
                }
            }
            ReaderLayout::Horizontal => {
                self.anchor = (self.anchor + 1).min(self.total_pages - 1);
            }
        }
    }

    pub fn prev_row(&mut self) {
        self.scroll_px = 0;
        let step = self.effective_columns();
        match self.layout {
            ReaderLayout::Single => {
                self.anchor = self.anchor.saturating_sub(1);
            }
            ReaderLayout::Spread => {
                if self.cover_alone && self.anchor <= step {
                    self.anchor = 0;
                } else {
                    self.anchor = self.anchor.saturating_sub(step);
                }
            }
            ReaderLayout::Horizontal => {
                self.anchor = self.anchor.saturating_sub(1);
            }
        }
    }

    pub fn jump_to_page(&mut self, page_0based: usize) {
        self.scroll_px = 0;
        self.set_row_scroll_x(0);
        if self.total_pages == 0 {
            return;
        }
        let clamped = page_0based.min(self.total_pages - 1);
        if self.layout == ReaderLayout::Spread {
            if self.cover_alone && clamped > 0 {
                self.anchor = if !clamped.is_multiple_of(2) { clamped } else { clamped - 1 };
                self.focus = if !clamped.is_multiple_of(2) { Slot::Left } else { Slot::Right };
            } else if !self.cover_alone {
                self.anchor = (clamped / 2) * 2;
                self.focus = if clamped.is_multiple_of(2) { Slot::Left } else { Slot::Right };
            } else {
                self.anchor = clamped;
                self.focus = Slot::Left;
            }
            self.focused_slot = clamped.saturating_sub(self.anchor);
        } else {
            self.anchor = clamped;
            self.focus = Slot::Left;
            self.focused_slot = 0;
        }
    }

    pub fn placements(&self) -> Vec<ReaderSlotPlacement> {
        self.placements.lock().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn set_placements(&mut self, placements: Vec<ReaderSlotPlacement>) {
        if let Ok(mut lock) = self.placements.lock() {
            *lock = placements;
        }
    }

    pub fn status_line(&self, filename: &str) -> String {
        let placements = self.placements();
        let primary_placements: Vec<&ReaderSlotPlacement> =
            placements.iter().filter(|p| p.is_primary).collect();
        let source_placements = if primary_placements.is_empty() {
            placements.iter().collect::<Vec<_>>()
        } else {
            primary_placements
        };
        let mut active_pages: Vec<usize> = source_placements.iter().map(|p| p.page).collect();
        active_pages.sort_unstable();
        active_pages.dedup();

        let page_label = if active_pages.len() > 1 {
            format!(
                "spread {}-{}",
                active_pages[0] + 1,
                active_pages[active_pages.len() - 1] + 1
            )
        } else if let Some(&p) = active_pages.first() {
            format!("p.{}", p + 1)
        } else {
            let vis = self.visible_pages();
            if vis.len() > 1 {
                format!("spread {}-{}", vis[0] + 1, vis[1] + 1)
            } else if !vis.is_empty() {
                format!("p.{}", vis[0] + 1)
            } else {
                "p.0".to_string()
            }
        };

        let focused_page = self.focused_page();
        let view = self.page_view(focused_page);
        let fit_label = match view.fit {
            ReaderFitMode::FitWidth => "width",
            ReaderFitMode::FitPage => "page",
        };
        let zoom_pct = (view.zoom * 100.0).round() as u32;
        let focus_side = match self.focus {
            Slot::Left => "L",
            Slot::Right => "R",
        };
        let sync_status = if self.sync { "on" } else { "off" };

        let cover_str = if self.layout == ReaderLayout::Spread && self.cover_alone {
            " · cover:on"
        } else {
            ""
        };

        format!(
            "{} · {}/{} · fit:{} {}% · focus:{} · sync:{}{} · q:help",
            filename, page_label, self.total_pages, fit_label, zoom_pct, focus_side, sync_status, cover_str
        )
    }
}
