//! Zoom → render-target policy for the interactive presenter.
//!
//! The zoom factor lives on [`PresentationState`] (the `NavState` contract);
//! this module owns what a zoom *step* does to the presentation: recompute the
//! pixel target from the latched terminal grid, invalidate the rasters that
//! were rendered for the old one, re-stage the visible graphics and schedule
//! the prefetch. Same for a terminal resize, which is a pixel-target change
//! with an unchanged zoom factor.
//!
//! Split out of `app.rs` so the target math (EMU aspect → pixel target →
//! clamped bounds) is readable on its own and `app.rs` stays a facade over the
//! event loop.

use kitty_pptx_core::InvalidateCause;

use crate::app::PresentationApp;

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

    /// Recompute the render target from the latched layout and zoom factor
    /// without changing the terminal grid. Used after a zoom step.
    pub(crate) fn apply_scale(&mut self) {
        let (cols, rows) = self.state.layout_size();
        self.recompute_target(cols, rows);
        self.pipeline.invalidate(InvalidateCause::TargetChanged);
        self.restage_graphics();
        self.trigger_prefetch();
    }

    /// Base target for the terminal grid, multiplied by the latched zoom.
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
        let metrics = kitty_pptx_display::probe_cell_pixel_size();
        let cell_w = metrics.width_px.max(8.0);
        let cell_h = metrics.height_px.max(16.0);
        let screen_w = (cols as f64 * cell_w * 2.0).round() as u32;
        let screen_h = (rows as f64 * cell_h * 2.0).round() as u32;
        let max_width = screen_w.clamp(1920, 3840);
        let max_height = screen_h.clamp(1080, 2160);

        let base_width = max_width
            .min((max_height as f64 * slide_aspect).round() as u32)
            .max(640);
        let scale = self.state.scale().max(0.01);
        let width_px = ((base_width as f32 * scale).round() as u32).max(1);
        let height_px = ((width_px as f64 / slide_aspect).round().max(1.0)) as u32;
        self.current_target = kitty_pptx_core::RenderTargetSpec {
            width_px,
            height_px,
        };
    }
}
