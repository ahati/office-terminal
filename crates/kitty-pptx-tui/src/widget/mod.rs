pub mod canvas;
pub mod layout;

use kitty_pptx_display::{ImageId, PlacementMode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

pub use canvas::{clear_letterbox, emit_direct_canvas};
pub use layout::{
    cell_rect_to_rect, compute_slide_rect, rect_to_cell_rect, ASPECT_16_9, ASPECT_4_3,
    DEFAULT_CELL_ASPECT_RATIO,
};

/// A Ratatui widget that displays a slide within a given `Rect` while preserving aspect ratio.
///
/// The widget is pure: it only reserves the canvas (letterbox clear + direct
/// canvas emit). The fitted cell rectangle it computes is recorded by the
/// calling view into [`crate::app::bindings::FrameBindings`], and the Kitty
/// driver places the actual image after the frame.
#[derive(Clone)]
pub struct SlideWidget {
    pub image_id: ImageId,
    pub mode: PlacementMode,
    pub aspect_ratio: f32,
    pub cell_aspect_ratio: f32,
    pub letterbox_bg: Color,
}

impl Default for SlideWidget {
    fn default() -> Self {
        Self {
            image_id: ImageId(1),
            mode: PlacementMode::Interactive,
            aspect_ratio: ASPECT_16_9,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
            letterbox_bg: Color::Black,
        }
    }
}

impl SlideWidget {
    pub fn new(image_id: ImageId, mode: PlacementMode) -> Self {
        Self {
            image_id,
            mode,
            ..Default::default()
        }
    }

    pub fn with_aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    pub fn with_cell_aspect_ratio(mut self, cell_aspect_ratio: f32) -> Self {
        self.cell_aspect_ratio = cell_aspect_ratio;
        self
    }

    pub fn with_letterbox_bg(mut self, color: Color) -> Self {
        self.letterbox_bg = color;
        self
    }
}

impl Widget for SlideWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &SlideWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let fitted = compute_slide_rect(area, self.aspect_ratio, self.cell_aspect_ratio);
        clear_letterbox(buf, area, fitted, self.letterbox_bg);
        emit_direct_canvas(buf, fitted, Color::Reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_widget_reserves_the_fitted_canvas() {
        let widget = SlideWidget::new(ImageId(2), PlacementMode::Interactive);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // The fitted area becomes a transparent graphics canvas (reset
        // cells), and the letterbox bars carry the widget background.
        let fitted = compute_slide_rect(area, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        let center = buf
            .cell((fitted.x + fitted.width / 2, fitted.y + fitted.height / 2))
            .expect("center lies inside the buffer");
        assert_eq!(center.symbol(), " ");
        assert_eq!(center.bg, Color::Reset);

        if fitted.y > 0 {
            let bar = buf.cell((0, 0)).expect("letterbox cell inside the buffer");
            assert_eq!(bar.bg, Color::Black);
        }
    }
}
