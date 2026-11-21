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
    /// [`ReaderState::scroll_px`] via [`ReaderState::advance_display`]
    /// (implemented in [`crate::app::reader::anim`]).
    pub(crate) displayed_scroll_px: Arc<AtomicU32>,
    /// Effective column count of the last drawn frame. Layout output lives
    /// in [`crate::app::bindings::FrameBindings`]; the run loop's publisher
    /// routes it here so input-side queries (`visible_pages`, row steps)
    /// observe the geometry that is actually on screen.
    num_columns: usize,
    pub focused_slot: usize,
    pub row_scroll_x: Arc<AtomicI64>,
    pub max_row_scroll_x: Arc<AtomicI64>,
    /// Per-page fit/zoom/scroll cache; default-filled lazily on first access
    /// (accessors implemented in [`crate::app::reader::views`]).
    pub(crate) views: HashMap<usize, PageView>,
    total_pages: usize,
    page_sizes: Vec<(f32, f32)>,
    /// Dominant page size in points (post-rotation): the majority size of
    /// the document, the basis of the stable column grid (PRD 14a §6.4).
    dominant_pt: (f32, f32),
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
            dominant_pt: dominant_page_pt(&page_sizes),
            page_sizes,
        }
    }

    pub fn total_pages(&self) -> usize {
        self.total_pages
    }

    pub fn page_size_pt(&self, page: usize) -> (f32, f32) {
        self.page_sizes.get(page).copied().unwrap_or((612.0, 792.0))
    }

    /// The document's dominant page size in points: the most common
    /// `(width, height)` bucket (0.1 pt granularity; ties go to the
    /// earliest page). Uniform documents report their own size; mixed
    /// documents report the majority, which becomes the column-grid basis
    /// so outlier (landscape) pages pair with their neighbors at identical
    /// positions row after row (PRD 14a §6.4).
    pub fn dominant_page_pt(&self) -> (f32, f32) {
        self.dominant_pt
    }

    /// Width in device pixels of one column box on the stable grid: the
    /// dominant page's fit width at the document zoom level.
    pub fn column_box_w(&self, viewport_h_px: f32) -> f32 {
        let (w, h) = self.dominant_pt;
        let ar = (w / h).max(0.1);
        (viewport_h_px * ar * self.default_zoom).max(10.0)
    }

    // Page views: `page_view` / `page_view_mut` / `cycle_fit` /
    // `adjust_zoom` / `reset_view` / `apply_zoom_all` live in
    // `crate::app::reader::views`.

    // ─── Smoothed scroll display ───────────────────────────────────────

    // Implemented in `crate::app::reader::anim` (`displayed_scroll_px`,
    // `shift_displayed_scroll_px`, `snap_display`, `advance_display`,
    // `is_animating`).

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

    /// Raster-width classes for reader pages (device-pixel aware).
    ///
    /// A page raster only has to out-resolve the pane that shows it, so the
    /// target snaps the pane's zoomed device width (× [`READER_TARGET_QUALITY`])
    /// up this ladder. Coarse classes keep resize jitter from re-rendering.
    /// The 1280 px floor keeps 12 pt text at ~25 px — comfortably above the
    /// rasterizer's 6 px font floor — and 3840 px covers full-screen panes on
    /// 4K-retina terminals.
    pub const READER_TARGET_CLASSES: &[u32] = &[1280, 1920, 2560, 3840];

    /// Supersampling factor over the pane's device width before class
    /// snapping: enough headroom for kitty's downscale to stay crisp without
    /// paying for the old fixed 2560 px on small panes.
    pub const READER_TARGET_QUALITY: f32 = 1.5;

    /// Zoomed pane width in device pixels for `page` under the current
    /// layout — the input basis for [`ReaderState::page_render_target`].
    pub fn pane_width_px(
        &self,
        page: usize,
        viewport_w_px: f32,
        viewport_h_px: f32,
        cell_w_px: f32,
    ) -> f32 {
        let slot_w = self.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
        let (base_w, _) = self.pane_fit(page, viewport_h_px, slot_w);
        base_w * self.page_view(page).zoom
    }

    /// Raster target for `page`, sized to the pane it is shown in.
    ///
    /// `pane_w_px` is the pane's zoomed device-pixel width (from the pane
    /// layout, or [`ReaderState::pane_width_px`] on the event side); the
    /// target snaps it up the [`Self::READER_TARGET_CLASSES`] ladder, so a
    /// 600 px pane renders at 1280 px instead of the old fixed 2560 px —
    /// ~4× faster rasterization and ~4× less wire payload per page, which is
    /// what kept boundary transmits landing mid-glide (the page-transition
    /// flicker). Shared by the pane layout and the prefetch planner so staged
    /// rasters always match placement lookups, including mixed-size
    /// documents.
    pub fn page_render_target(&self, page: usize, pane_w_px: f32) -> RenderTargetSpec {
        let (pt_w, pt_h) = self.page_size_pt(page);
        let ar = (pt_w / pt_h).max(0.1);
        let wanted = (pane_w_px.max(1.0) * Self::READER_TARGET_QUALITY).round() as u32;
        let width_px = Self::READER_TARGET_CLASSES
            .iter()
            .copied()
            .find(|&class| class >= wanted)
            .unwrap_or(Self::READER_TARGET_CLASSES[Self::READER_TARGET_CLASSES.len() - 1]);
        // Aspect-true height in the rasterizer's supported range; the lower
        // bound keeps the original 2560→1024 ratio (2:5) at every class so
        // wide pages never distort.
        let height_px = ((width_px as f32 / ar).round() as u32)
            .clamp((width_px * 2 / 5).max(1), 8192);
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

    /// Fit-slot width in device pixels for the current row (PRD 14a §6.1/§6.4).
    ///
    /// Single-column rows fit the viewport. Multi-column rows pick the slot
    /// by fit mode: `FitWidth` fills the viewport-divided slot; `FitPage`
    /// contains every page in the dominant page's **column box**, so a
    /// document's rows all share one stable grid — a landscape page beside
    /// a portrait lead lands at exactly the columns the portrait pages use,
    /// and the row peeking in from below during a scroll is the row the
    /// anchor will flip to.
    pub fn slot_width(&self, viewport_w_px: f32, viewport_h_px: f32, cell_w_px: f32) -> f32 {
        self.slot_width_for(viewport_w_px, viewport_h_px, cell_w_px, self.effective_columns())
    }

    /// [`Self::slot_width`] for an explicit column count — the render path
    /// passes its freshly planned count so the slot always matches the
    /// frame's own layout (the stored count trails by one publish).
    pub fn slot_width_for(
        &self,
        viewport_w_px: f32,
        viewport_h_px: f32,
        cell_w_px: f32,
        cols: usize,
    ) -> f32 {
        let gutter_px = 2.0 * cell_w_px;
        if cols <= 1 {
            return viewport_w_px;
        }
        let fit_slot = ((viewport_w_px - (cols - 1) as f32 * gutter_px) / cols as f32).max(10.0);
        match self.page_view(self.anchor).fit {
            ReaderFitMode::FitWidth => fit_slot,
            ReaderFitMode::FitPage => self.column_box_w(viewport_h_px).min(fit_slot),
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

/// Majority page size of a document: the most common `(w, h)` bucket at
/// 0.1 pt granularity, ties resolved to the earliest page. Empty documents
/// fall back to letter.
fn dominant_page_pt(sizes: &[(f32, f32)]) -> (f32, f32) {
    if sizes.is_empty() {
        return (612.0, 792.0);
    }
    /// One 0.1 pt size bucket: `(key, count, first index, size)`.
    struct Bucket {
        key: (i64, i64),
        count: usize,
        first: usize,
        size: (f32, f32),
    }
    let bucket = |(w, h): (f32, f32)| ((w * 10.0).round() as i64, (h * 10.0).round() as i64);
    let mut acc: Vec<Bucket> = Vec::new();
    for (i, &size) in sizes.iter().enumerate() {
        let key = bucket(size);
        match acc.iter_mut().find(|b| b.key == key) {
            Some(entry) => entry.count += 1,
            None => acc.push(Bucket { key, count: 1, first: i, size }),
        }
    }
    // Highest count wins; ties go to the earliest first occurrence.
    acc.sort_by(|a, b| b.count.cmp(&a.count).then(a.first.cmp(&b.first)));
    acc[0].size
}
