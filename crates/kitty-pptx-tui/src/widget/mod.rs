pub mod canvas;
pub mod layout;

use kitty_pptx_display::{CellRect, ImageId, PlacementMode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use std::sync::{Arc, Mutex};

pub use canvas::{clear_letterbox, emit_direct_canvas};
pub use layout::{
    cell_rect_to_rect, compute_slide_rect, rect_to_cell_rect, ASPECT_16_9, ASPECT_4_3,
    DEFAULT_CELL_ASPECT_RATIO,
};

/// A Ratatui widget that displays a slide within a given `Rect` while preserving aspect ratio.
///
/// The widget only reserves the canvas and records its cell rectangle. The Kitty
/// driver places the actual image after the frame.
#[derive(Clone)]
pub struct SlideWidget {
    pub image_id: ImageId,
    pub mode: PlacementMode,
    pub aspect_ratio: f32,
    pub cell_aspect_ratio: f32,
    pub direct_rect: Option<Arc<Mutex<Option<CellRect>>>>,
    pub letterbox_bg: Color,
}

impl Default for SlideWidget {
    fn default() -> Self {
        Self {
            image_id: ImageId(1),
            mode: PlacementMode::Interactive,
            aspect_ratio: ASPECT_16_9,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
            direct_rect: None,
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

    pub fn direct(image_id: ImageId, sink: Arc<Mutex<Option<CellRect>>>) -> Self {
        Self {
            image_id,
            direct_rect: Some(sink),
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

    pub fn with_direct_sink(mut self, sink: Arc<Mutex<Option<CellRect>>>) -> Self {
        self.direct_rect = Some(sink);
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

        if let Some(sink) = &self.direct_rect {
            if let Ok(mut lock) = sink.lock() {
                *lock = Some(rect_to_cell_rect(fitted));
            }
        }

        emit_direct_canvas(buf, fitted, Color::Reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_widget_direct_positioning() {
        let sink = Arc::new(Mutex::new(None));
        let widget = SlideWidget::direct(ImageId(2), sink.clone());
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        widget.render(Rect::new(0, 0, 80, 24), &mut buf);

        let recorded = sink.lock().unwrap().expect("CellRect must be recorded");
        let expected = rect_to_cell_rect(compute_slide_rect(
            Rect::new(0, 0, 80, 24),
            ASPECT_16_9,
            DEFAULT_CELL_ASPECT_RATIO,
        ));
        assert_eq!(recorded, expected);
    }
}
