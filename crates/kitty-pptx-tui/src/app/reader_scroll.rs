//! Scrolling calculations and spatial offsets for Reader mode.

use std::sync::atomic::Ordering;
use crate::app::reader_state::ReaderState;
use crate::app::reader_types::{ReaderFitMode, ReaderLayout};

pub const VERTICAL_GUTTER_ROWS: u16 = 1;

impl ReaderState {
    /// Compute maximum height in pixels among visible pages in the primary row.
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
        let slot_w = self.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
        let mut max_h = 0.0f32;
        for &page in &vis {
            let (_, base_h) = self.pane_fit(page, viewport_h_px, slot_w);
            let h = base_h * self.page_view(page).zoom;
            if h > max_h {
                max_h = h;
            }
        }
        max_h.max(10.0)
    }

    /// Compute total row stride in pixels, including vertical inter-row gutter spacing.
    pub fn current_row_stride_px(
        &self,
        viewport_w_px: f32,
        viewport_h_px: f32,
        cell_w_px: f32,
        cell_h_px: f32,
    ) -> f32 {
        let slot_w = self.slot_width(viewport_w_px, viewport_h_px, cell_w_px);
        self.row_stride_px(viewport_h_px, slot_w, cell_w_px, cell_h_px)
    }

    /// Vertical column continuous scroll with anchor advance on row boundary.
    /// Returns `true` if position or anchor moved.
    pub fn scroll_column(&mut self, delta_px: i64, row_stride_px: f32, base_h_px: f32) -> bool {
        let (anchor_changed, moved) = self.scroll_column_detailed(delta_px, row_stride_px, base_h_px);
        anchor_changed || moved
    }

    /// Detailed vertical scroll returning `(anchor_changed, offset_moved)`.
    pub fn scroll_column_detailed(
        &mut self,
        delta_px: i64,
        row_stride_px: f32,
        base_h_px: f32,
    ) -> (bool, bool) {
        if self.total_pages() == 0 || row_stride_px <= 0.0 {
            return (false, false);
        }
        let prev_anchor = self.anchor;
        let prev_scroll_px = self.scroll_px;

        self.scroll_px += delta_px;
        let step = self.effective_columns();

        // Advance anchor when scrolled past bottom of current row. The
        // displayed scroll wraps by the same stride so the on-screen content
        // position stays continuous across the anchor transition (the
        // smoothed value otherwise lags one stride behind and the view would
        // flash backward a full row).
        while self.scroll_px >= row_stride_px as i64 {
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
            if next_anchor < self.total_pages() {
                self.anchor = next_anchor;
                self.scroll_px -= row_stride_px as i64;
                self.shift_displayed_scroll_px(-row_stride_px);
            } else {
                self.scroll_px = (base_h_px as i64).saturating_sub(1);
                break;
            }
        }

        // Clamp at document end so Row 0 remains visible on the last row
        let has_next = match self.layout {
            ReaderLayout::Single => self.anchor + 1 < self.total_pages(),
            ReaderLayout::Spread => {
                let next = if self.cover_alone && self.anchor == 0 {
                    1
                } else {
                    self.anchor + step
                };
                next < self.total_pages()
            }
            ReaderLayout::Horizontal => self.anchor + 1 < self.total_pages(),
        };
        if !has_next && self.scroll_px >= base_h_px as i64 {
            self.scroll_px = (base_h_px as i64).saturating_sub(1);
        }

        // Retreat anchor when scrolled past top of current row (same
        // stride lockstep as the advance wrap above).
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
                self.scroll_px += row_stride_px as i64;
                self.shift_displayed_scroll_px(row_stride_px);
            } else {
                self.scroll_px = 0;
                break;
            }
        }

        let anchor_changed = self.anchor != prev_anchor;
        let moved = anchor_changed || self.scroll_px != prev_scroll_px;
        (anchor_changed, moved)
    }

    /// Continuous horizontal slide with wrap across page boundaries.
    /// Returns `(anchor_changed, offset_moved)`.
    pub fn scroll_horizontal_continuous(&mut self, delta_px: i64, page_w_px: f32) -> (bool, bool) {
        if self.total_pages() == 0 || page_w_px <= 0.0 {
            return (false, false);
        }
        let prev_anchor = self.anchor;
        let prev_scroll_px = self.scroll_px;

        self.scroll_px += delta_px;

        // Anchor wraps shift the displayed scroll by the same page width so
        // the slide stays continuous across page boundaries.
        while self.scroll_px >= page_w_px as i64 {
            if self.anchor + 1 < self.total_pages() {
                self.anchor += 1;
                self.scroll_px -= page_w_px as i64;
                self.shift_displayed_scroll_px(-page_w_px);
            } else {
                self.scroll_px = (page_w_px as i64).saturating_sub(1);
                break;
            }
        }

        while self.scroll_px < 0 {
            if self.anchor > 0 {
                self.anchor -= 1;
                self.scroll_px += page_w_px as i64;
                self.shift_displayed_scroll_px(page_w_px);
            } else {
                self.scroll_px = 0;
                break;
            }
        }

        let anchor_changed = self.anchor != prev_anchor;
        let moved = anchor_changed || self.scroll_px != prev_scroll_px;
        (anchor_changed, moved)
    }

    /// Slide the entire spread row horizontally when zoomed in past viewport.
    pub fn scroll_row_horizontal(&self, delta_px: i64) -> bool {
        let prev = self.row_scroll_x.load(Ordering::Relaxed);
        let max_x = self.max_row_scroll_x.load(Ordering::Relaxed);
        if max_x <= 0 {
            return false;
        }
        let next = (prev + delta_px).clamp(0, max_x);
        self.row_scroll_x.store(next, Ordering::Relaxed);
        next != prev
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
        if max_scroll_x == 0 {
            return false;
        }
        let prev = view.scroll_x;
        let next_x = ((view.scroll_x as i32 + delta_px).max(0) as u32).min(max_scroll_x);
        view.scroll_x = next_x;
        view.scroll_x != prev
    }
}
