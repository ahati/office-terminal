//! Zoom → view policy for the interactive presenter.
//!
//! The zoom factor lives on [`PresentationState`] (the `NavState` contract);
//! this module owns what a zoom *step* does: **the placement changes and the
//! resolution follows only when the placement outruns the raster**. The
//! zoom-aware placement itself is pure geometry
//! ([`crate::widget::zoom_fit`]), so a zoom step is just a redraw: the same
//! staged bitmap is re-placed at the new cell rect / crop window, which
//! Kitty replaces atomically — no re-render, no blank window.
//!
//! Resolution policy (resolution follows placement):
//!
//! * The base raster already carries a [`CANVAS_SUPERSAMPLE`]× headroom over
//!   the physical screen, so every ladder rung up to that factor is satisfied
//!   by the existing bitmap; the terminal scales client-side.
//! * Zoom **out** never shrinks the raster: a smaller bitmap would upscale
//!   into the same cells and pixelate, while the base raster downscales for
//!   free.
//! * Zoom **in** past the headroom bumps the raster (`render_scale`), capped
//!   by [`MAX_TARGET_DIMENSION_PX`] / [`MAX_TARGET_PIXELS`] so the pixmap
//!   allocation stays bounded (`base_width * zoom` used to be uncapped and
//!   reached ~455 MB RGBA at zoom 4 on a large terminal).
//! * Nothing is invalidated or released: the raster cache is keyed by
//!   `(slide, target)`, so a bumped target simply renders alongside the old
//!   entries (the budget evicts them) and the slide syncer's swap discipline
//!   replaces the bitmap on screen without a blank frame.
//!
//! Split out of `app.rs` so the target math (EMU aspect → pixel target →
//! clamped bounds) is readable on its own and `app.rs` stays a facade over
//! the event loop.

use crate::app::PresentationApp;

/// Raster supersampling baked into the base target (`screen_w = cols * cell_w
/// * 2.0`): zoom rungs up to this factor are covered by the base raster.
pub(crate) const CANVAS_SUPERSAMPLE: f64 = 2.0;
/// Hard ceiling for any canvas raster dimension, keeping the pixmap
/// allocation bounded.
const MAX_TARGET_DIMENSION_PX: u32 = 8192;
const MAX_TARGET_PIXELS: u64 = 64_000_000;

impl PresentationApp {
    /// Zoom by an explicit factor. Returns whether the factor changed.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if !self.state.set_scale(scale) {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Step to the next / previous ladder rung.
    pub fn zoom_in(&mut self) -> bool {
        if !self.state.nav_mut().zoom_in() {
            return false;
        }
        self.apply_scale();
        true
    }

    pub fn zoom_out(&mut self) -> bool {
        if !self.state.nav_mut().zoom_out() {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Fit the slide width / whole slide to the current terminal grid.
    pub fn fit_width(&mut self) -> bool {
        let (cols, _) = self.state.layout_size();
        if !self.state.fit_width(cols as f32) {
            return false;
        }
        self.apply_scale();
        true
    }

    pub fn fit_page(&mut self) -> bool {
        let (cols, rows) = self.state.layout_size();
        if !self.state.fit_page(cols as f32, rows as f32) {
            return false;
        }
        self.apply_scale();
        true
    }

    /// Apply a zoom step: recompute the (possibly unchanged) target and
    /// prefetch only when the resolution actually moved.
    ///
    /// Deliberately *not* destructive: no cache invalidation, no
    /// `release_all`. The visible bitmap stays placed while the new geometry
    /// takes effect (a pure re-placement for rungs within the supersample
    /// headroom), and a genuinely new raster arrives through the slide
    /// syncer's present-then-release swap. This is the same discipline a
    /// terminal resize gets: the screen is never blanked to change geometry.
    pub(crate) fn apply_scale(&mut self) {
        let (cols, rows) = self.state.layout_size();
        let before = self.current_target;
        self.recompute_target(cols, rows);
        if self.current_target != before {
            self.trigger_prefetch();
        }
    }

    /// Base target for the terminal grid; resolution follows placement.
    pub(crate) fn recompute_target(&mut self, cols: u16, rows: u16) {
        // Reader pages render at the device-pixel class of the pane showing
        // them (see `ReaderState::page_render_target`): panes then stage
        // sub-rectangles of that bitmap instead of re-rendering per viewport.
        if let Some(reader) = self.state.reader_state() {
            let (viewport_w_px, viewport_h_px, cell_w_px, _) =
                crate::app::reader_types::reader_viewport_metrics(cols, rows);
            let page = self.state.current_slide();
            let pane_w = reader.pane_width_px(page, viewport_w_px, viewport_h_px, cell_w_px);
            self.current_target = reader.page_render_target(page, pane_w);
            return;
        }

        // Pixel target based on live terminal dimensions and slide aspect ratio.
        let (slide_width, slide_height) = self.state.slide_dimensions();
        let slide_aspect = slide_width as f64 / slide_height.max(1) as f64;
        let metrics = kitty_pptx_display::cell_pixel_size_cached();
        let cell_w = metrics.width_px.max(8.0);
        let cell_h = metrics.height_px.max(16.0);
        let screen_w = (cols as f64 * cell_w * 2.0).round() as u32;
        let screen_h = (rows as f64 * cell_h * 2.0).round() as u32;
        let max_width = screen_w.clamp(1920, 3840);
        let max_height = screen_h.clamp(1080, 2160);

        let base_width = max_width
            .min((max_height as f64 * slide_aspect).round() as u32)
            .max(640);
        let zoom = f64::from(self.state.scale().max(0.01));
        // See the module docs: rungs inside the supersample headroom keep the
        // base raster; only zoom-in past it bumps the resolution, capped.
        let render_scale = (zoom / CANVAS_SUPERSAMPLE).max(1.0);
        let mut width_px =
            ((base_width as f64 * render_scale).round() as u32).clamp(1, MAX_TARGET_DIMENSION_PX);
        let mut height_px = ((width_px as f64 / slide_aspect).round().max(1.0) as u32)
            .clamp(1, MAX_TARGET_DIMENSION_PX);
        let pixels = u64::from(width_px) * u64::from(height_px);
        if pixels > MAX_TARGET_PIXELS {
            let shrink = MAX_TARGET_PIXELS as f64 / pixels as f64;
            width_px = ((width_px as f64 * shrink.sqrt()).round() as u32).max(1);
            height_px = ((height_px as f64 * shrink.sqrt()).round() as u32).max(1);
        }
        self.current_target = kitty_pptx_core::RenderTargetSpec {
            width_px,
            height_px,
        };
    }
}
