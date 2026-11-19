use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;

use kitty_pptx_core::RenderTargetSpec;

use crate::app::reader_scroll::VERTICAL_GUTTER_ROWS;

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
    /// Vertical scroll target in pixels (anchor advancement keys off this).
    pub scroll_px: i64,
    /// Scroll offset currently shown on screen (f32 bits in an atomic so the
    /// immutable render path can advance it once per frame). Converges toward
    /// [`ReaderState::scroll_px`] via [`ReaderState::advance_display`].
    displayed_scroll_px: Arc<AtomicU32>,
    /// Effective column count of the last drawn frame. Layout output lives
    /// in [`crate::app::bindings::FrameBindings`]; the run loop's publisher
    /// routes it here so input-side queries (`visible_pages`, row steps)
    /// observe the geometry that is actually on screen.
    num_columns: usize,
    pub focused_slot: usize,
    pub row_scroll_x: Arc<AtomicI64>,
    pub max_row_scroll_x: Arc<AtomicI64>,
    views: HashMap<usize, PageView>,
    total_pages: usize,
    page_sizes: Vec<(f32, f32)>,
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
            displayed_scroll_px: Arc::new(AtomicU32::new(0.0_f32.to_bits())),
            num_columns: 2,
            focused_slot: 0,
            row_scroll_x: Arc::new(AtomicI64::new(0)),
            max_row_scroll_x: Arc::new(AtomicI64::new(0)),
            views: HashMap::new(),
            total_pages,
            page_sizes,
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

    // ─── Smoothed scroll display ───────────────────────────────────────

    /// Scroll offset currently shown on screen. The smoothing target is
    /// [`ReaderState::scroll_px`].
    pub fn displayed_scroll_px(&self) -> f32 {
        f32::from_bits(self.displayed_scroll_px.load(Ordering::Relaxed))
    }

    fn set_displayed_scroll_px(&self, value: f32) {
        self.displayed_scroll_px
            .store(value.to_bits(), Ordering::Relaxed);
    }

    /// Shifts the displayed scroll by the same amount the target wrapped when
    /// the anchor advanced, keeping the on-screen content position continuous
    /// across row transitions (see `reader_scroll`).
    pub(crate) fn shift_displayed_scroll_px(&self, delta_px: f32) {
        self.set_displayed_scroll_px(self.displayed_scroll_px() + delta_px);
    }

    /// Snaps the displayed scroll onto its target (discrete navigation:
    /// layout toggles, row jumps and page jumps must not glide).
    fn snap_display(&mut self) {
        self.set_displayed_scroll_px(self.scroll_px as f32);
    }

    /// Advances the displayed scroll toward its target with an exponential
    /// approach (~35% of the remaining distance per frame, snapping below
    /// 1 px). Called once per frame by the reader render path; the value
    /// lives behind an atomic because the renderer only sees `&self`.
    /// Returns whether the displayed value moved this frame.
    pub fn advance_display(&self) -> bool {
        let target = self.scroll_px as f32;
        let displayed = self.displayed_scroll_px();
        let delta = target - displayed;
        if delta.abs() < 1.0 {
            if displayed == target {
                return false;
            }
            self.set_displayed_scroll_px(target);
            return true;
        }
        self.set_displayed_scroll_px(displayed + delta * 0.35);
        true
    }

    /// Whether the displayed scroll is still converging toward its target
    /// (the event loop should keep ticking while this is true).
    pub fn is_animating(&self) -> bool {
        (self.displayed_scroll_px() - self.scroll_px as f32).abs() >= 1.0
    }

    // ─── Shared pane geometry (PRD 14a §6) ─────────────────────────────

    /// Base pane size in pixels for `page` under its current fit mode, before
    /// zoom is applied. Returns `(width_px, height_px)`.
    ///
    /// Single source of truth for the FitWidth/FitPage fit formula shared by
    /// the row renderer (`ui::reader`), pane metrics (`ui::reader_layout`)
    /// and the scroll math (`reader_scroll`, `reader_events`).
    pub fn pane_fit(&self, page: usize, viewport_h_px: f32, slot_w_px: f32) -> (f32, f32) {
        let (pt_w, pt_h) = self.page_size_pt(page);
        let ar = (pt_w / pt_h).max(0.1);
        match self.page_view(page).fit {
            ReaderFitMode::FitWidth => (slot_w_px, slot_w_px / ar),
            ReaderFitMode::FitPage => {
                let h = viewport_h_px.min(slot_w_px / ar);
                (h * ar, h)
            }
        }
    }

    /// Canonical raster target for `page`: 2560 px wide, aspect-true height
    /// clamped to the rasterizer's supported range.
    ///
    /// Shared by the pane layout and the prefetch planner so staged rasters
    /// always match placement lookups, including mixed-size documents.
    pub fn page_render_target(&self, page: usize) -> RenderTargetSpec {
        let (pt_w, pt_h) = self.page_size_pt(page);
        let ar = (pt_w / pt_h).max(0.1);
        let width_px = 2560u32;
        let height_px = ((width_px as f32 / ar).round() as u32).clamp(1024, 8192);
        RenderTargetSpec {
            width_px,
            height_px,
        }
    }

    /// Total stride in pixels of the current row, including the inter-row
    /// vertical gutter.
    ///
    /// Unlike [`ReaderState::current_row_stride_px`] this takes the slot
    /// width directly, so callers that already computed it (the row
    /// renderer) do not re-derive it. `cell_w_px` is accepted for signature
    /// symmetry with [`ReaderState::scroll_step_px`].
    pub fn row_stride_px(
        &self,
        viewport_h_px: f32,
        slot_w_px: f32,
        _cell_w_px: f32,
        cell_h_px: f32,
    ) -> f32 {
        let mut max_h = 0.0f32;
        for page in self.visible_pages() {
            let (_, base_h) = self.pane_fit(page, viewport_h_px, slot_w_px);
            let h = base_h * self.page_view(page).zoom;
            if h > max_h {
                max_h = h;
            }
        }
        max_h.max(10.0) + (VERTICAL_GUTTER_ROWS as f32) * cell_h_px
    }

    /// One-eighth of the current row stride, floored at 24 px so every
    /// continuous-scroll key press moves a visible distance.
    pub fn scroll_step_px(
        &self,
        viewport_h_px: f32,
        slot_w_px: f32,
        cell_w_px: f32,
        cell_h_px: f32,
    ) -> i64 {
        let stride = self.row_stride_px(viewport_h_px, slot_w_px, cell_w_px, cell_h_px);
        ((stride / 8.0).round() as i64).max(24)
    }

    /// Records the effective column count of the last drawn frame.
    ///
    /// Only the run loop's frame publisher calls this: the layout output
    /// arrives via [`crate::app::bindings::FrameBindings::reader_columns`],
    /// never through the render path.
    pub(crate) fn set_num_columns(&mut self, cols: usize) {
        self.num_columns = cols.max(1);
    }

    pub fn num_columns(&self) -> usize {
        self.num_columns
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
        self.snap_display();
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
        self.snap_display();
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
        self.snap_display();
        self.set_row_scroll_x(0);
        self.set_max_row_scroll_x(0);
    }

    pub fn toggle_cover_alone(&mut self) {
        self.cover_alone = !self.cover_alone;
        self.scroll_px = 0;
        self.snap_display();
        self.set_row_scroll_x(0);
        self.set_max_row_scroll_x(0);
    }

    pub fn next_row(&mut self) {
        self.scroll_px = 0;
        self.snap_display();
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
        self.snap_display();
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
        self.snap_display();
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

    /// Status line for the reader viewport.
    ///
    /// `placements` and `columns` are the frame's layout output (recorded
    /// into [`crate::app::bindings::FrameBindings`] by the reader view); the
    /// label reports the primary row so preview rows never leak into it.
    pub fn status_line(
        &self,
        filename: &str,
        placements: &[ReaderSlotPlacement],
        columns: usize,
    ) -> String {
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
            if columns > 1 && vis.len() > 1 {
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
