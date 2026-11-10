pub mod layout;
pub mod placeholder;

use std::sync::{Arc, Mutex};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use kitty_pptx_display::{CellRect, ImageId, PlacementMode};

pub use layout::{
    cell_rect_to_rect, compute_slide_rect, rect_to_cell_rect, ASPECT_16_9, ASPECT_4_3,
    DEFAULT_CELL_ASPECT_RATIO,
};
pub use placeholder::{
    clear_letterbox, emit_direct_canvas, emit_placeholder_grid, encode_grid_coords_color,
    encode_image_id_color, KITTY_PLACEHOLDER_CHAR,
};

/// Rendering strategy used by `SlideWidget`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideRenderMode {
    /// Fill cells with Unicode placeholder U+10EEEE, encoding ImageId and cell coordinates.
    Placeholder,
    /// Clear cells and record screen `CellRect` for terminal graphics driver direct positioning.
    DirectPositioning,
}

/// A Ratatui widget that displays a slide within a given `Rect` while preserving aspect ratio.
#[derive(Clone)]
pub struct SlideWidget {
    pub image_id: ImageId,
    pub mode: PlacementMode,
    pub aspect_ratio: f32,
    pub cell_aspect_ratio: f32,
    pub render_mode: SlideRenderMode,
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
            render_mode: SlideRenderMode::Placeholder,
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

    pub fn placeholder(image_id: ImageId) -> Self {
        Self {
            image_id,
            render_mode: SlideRenderMode::Placeholder,
            ..Default::default()
        }
    }

    pub fn direct(image_id: ImageId, sink: Arc<Mutex<Option<CellRect>>>) -> Self {
        Self {
            image_id,
            render_mode: SlideRenderMode::DirectPositioning,
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

    pub fn with_render_mode(mut self, render_mode: SlideRenderMode) -> Self {
        self.render_mode = render_mode;
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

        // Letterbox / pillarbox regions outside fitted area
        clear_letterbox(buf, area, fitted, self.letterbox_bg);

        // Record the placement rect regardless of render mode: the terminal
        // graphics driver needs it both for direct placement and for the
        // virtual placement backing Kitty placeholder cells.
        if let Some(sink) = &self.direct_rect {
            if let Ok(mut lock) = sink.lock() {
                *lock = Some(rect_to_cell_rect(fitted));
            }
        }

        match self.render_mode {
            SlideRenderMode::Placeholder => {
                emit_placeholder_grid(buf, fitted, self.image_id);
            }
            SlideRenderMode::DirectPositioning => {
                emit_direct_canvas(buf, fitted, Color::Reset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_widget_placeholder_render() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        let widget = SlideWidget::new(ImageId(4), PlacementMode::Interactive);
        widget.render(Rect::new(0, 0, 80, 24), &mut buf);

        // Verify letterboxing / grid
        let fitted = compute_slide_rect(Rect::new(0, 0, 80, 24), ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO);
        let center_cell = buf.cell((fitted.x + fitted.width / 2, fitted.y + fitted.height / 2)).unwrap();
        assert_eq!(center_cell.symbol(), KITTY_PLACEHOLDER_CHAR.to_string());
        assert_eq!(center_cell.fg, encode_image_id_color(ImageId(4)));
    }

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
