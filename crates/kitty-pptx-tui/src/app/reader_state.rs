use std::collections::HashMap;

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
    pub num_columns: usize,
    pub focused_slot: usize,
    views: HashMap<usize, PageView>,
    total_pages: usize,
    page_sizes: Vec<(f32, f32)>,
    placements: Vec<ReaderSlotPlacement>,
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
            num_columns: 2,
            focused_slot: 0,
            views: HashMap::new(),
            total_pages,
            page_sizes,
            placements: Vec::new(),
        }
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

    pub fn effective_columns(&self) -> usize {
        match self.layout {
            ReaderLayout::Single => 1,
            ReaderLayout::Spread => self.num_columns.max(1),
            ReaderLayout::Horizontal => self.num_columns.max(1),
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

    pub fn current_row_height_px(
        &self,
        viewport_w_px: f32,
        viewport_h_px: f32,
        cell_w_px: f32,
    ) -> f32 {
        let vis = self.visible_pages();
        if vis.is_empty() {
            return viewport_h_px;
        }
        let slot_w = self.slot_width(viewport_w_px, cell_w_px);
        let mut max_h = 0.0f32;
        for &page in &vis {
            let (pt_w, pt_h) = self.page_size_pt(page);
            let ar = (pt_w / pt_h).max(0.1);
            let view = self.page_view(page);
            let base_h = match view.fit {
                ReaderFitMode::FitWidth => slot_w / ar,
                ReaderFitMode::FitPage => viewport_h_px.min(slot_w / ar),
            };
            let h = base_h * view.zoom;
            if h > max_h {
                max_h = h;
            }
        }
        max_h.max(10.0)
    }

    pub fn scroll_column(&mut self, delta_px: i64, row_h_px: f32) -> bool {
        if self.total_pages == 0 || row_h_px <= 0.0 {
            return false;
        }
        let prev_anchor = self.anchor;
        let prev_scroll_px = self.scroll_px;

        self.scroll_px += delta_px;
        let step = self.effective_columns();

        // If scrolled past bottom of current row, advance anchor
        while self.scroll_px >= row_h_px as i64 {
            let next_anchor = match self.layout {
                ReaderLayout::Single => self.anchor + 1,
                ReaderLayout::Spread => {
                    if self.cover_alone && self.anchor == 0 {
                        1
                    } else {
                        self.anchor + step
                    }
                }
                ReaderLayout::Horizontal => self.anchor + 1,
            };
            if next_anchor < self.total_pages {
                self.anchor = next_anchor;
                self.scroll_px -= row_h_px as i64;
            } else {
                self.scroll_px = (row_h_px as i64).saturating_sub(1);
                break;
            }
        }

        // If scrolled past top of current row, retreat anchor
        while self.scroll_px < 0 {
            if self.anchor > 0 {
                let prev = match self.layout {
                    ReaderLayout::Single => self.anchor.saturating_sub(1),
                    ReaderLayout::Spread => {
                        if self.cover_alone && self.anchor <= step {
                            0
                        } else {
                            self.anchor.saturating_sub(step)
                        }
                    }
                    ReaderLayout::Horizontal => self.anchor.saturating_sub(1),
                };
                self.anchor = prev;
                self.scroll_px += row_h_px as i64;
            } else {
                self.scroll_px = 0;
                break;
            }
        }

        self.anchor != prev_anchor || self.scroll_px != prev_scroll_px
    }

    pub fn scroll_horizontal_continuous(&mut self, delta_px: i64, page_w_px: f32) -> bool {
        if self.total_pages == 0 || page_w_px <= 0.0 {
            return false;
        }
        let prev_anchor = self.anchor;
        let prev_scroll_px = self.scroll_px;

        self.scroll_px += delta_px;

        while self.scroll_px >= page_w_px as i64 {
            if self.anchor + 1 < self.total_pages {
                self.anchor += 1;
                self.scroll_px -= page_w_px as i64;
            } else {
                self.scroll_px = (page_w_px as i64).saturating_sub(1);
                break;
            }
        }

        while self.scroll_px < 0 {
            if self.anchor > 0 {
                self.anchor -= 1;
                self.scroll_px += page_w_px as i64;
            } else {
                self.scroll_px = 0;
                break;
            }
        }

        self.anchor != prev_anchor || self.scroll_px != prev_scroll_px
    }

    pub fn scroll_vertical(&mut self, delta_px: i32) -> bool {
        self.scroll_vertical_bounded(delta_px, u32::MAX)
    }

    pub fn scroll_vertical_bounded(&mut self, delta_px: i32, max_scroll_y: u32) -> bool {
        let targets = if self.sync {
            self.visible_pages()
        } else {
            vec![self.focused_page()]
        };

        let mut moved = false;
        for page in targets {
            let view = self.page_view_mut(page);
            if view.fit == ReaderFitMode::FitPage {
                continue;
            }
            let prev = view.scroll_y;
            let next_y = ((view.scroll_y as i32 + delta_px).max(0) as u32).min(max_scroll_y);
            view.scroll_y = next_y;
            if view.scroll_y != prev {
                moved = true;
            }
        }
        moved
    }

    pub fn scroll_horizontal(&mut self, delta_px: i32) -> bool {
        self.scroll_horizontal_bounded(delta_px, u32::MAX)
    }

    pub fn scroll_horizontal_bounded(&mut self, delta_px: i32, max_scroll_x: u32) -> bool {
        let page = self.focused_page();
        let view = self.page_view_mut(page);
        if view.fit == ReaderFitMode::FitPage || max_scroll_x == 0 {
            view.scroll_x = 0;
            return false;
        }
        let prev = view.scroll_x;
        let next_x = ((view.scroll_x as i32 + delta_px).max(0) as u32).min(max_scroll_x);
        view.scroll_x = next_x;
        view.scroll_x != prev
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
    }

    pub fn toggle_horizontal(&mut self) {
        self.layout = if self.layout == ReaderLayout::Horizontal {
            ReaderLayout::Spread
        } else {
            ReaderLayout::Horizontal
        };
        self.scroll_px = 0;
    }

    pub fn toggle_cover_alone(&mut self) {
        self.cover_alone = !self.cover_alone;
        self.scroll_px = 0;
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

    pub fn placements(&self) -> &[ReaderSlotPlacement] {
        &self.placements
    }

    pub fn set_placements(&mut self, placements: Vec<ReaderSlotPlacement>) {
        self.placements = placements;
    }

    pub fn status_line(&self, filename: &str) -> String {
        let mut active_pages: Vec<usize> = self.placements.iter().map(|p| p.page).collect();
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
