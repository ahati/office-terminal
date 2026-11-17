use kitty_pptx_core::RenderTargetSpec;
use kitty_pptx_display::{CellRect, ImageId, PixelRect};

/// Layout model for displaying document pages in reader mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderLayout {
    Single,
    Spread,
    Horizontal,
}

/// Slot identifier in two-page spread layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Left,
    Right,
}

/// Scaling basis for document page fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderFitMode {
    FitWidth,
    FitPage,
}

/// Per-page viewport view state and spatial offsets.
#[derive(Clone, Debug, PartialEq)]
pub struct PageView {
    pub fit: ReaderFitMode,
    pub zoom: f32,
    pub scroll_x: u32,
    pub scroll_y: u32,
}

impl Default for PageView {
    fn default() -> Self {
        Self {
            fit: ReaderFitMode::FitPage,
            zoom: 1.0,
            scroll_x: 0,
            scroll_y: 0,
        }
    }
}

/// A staged placement for a visible reader pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderSlotPlacement {
    pub page: usize,
    pub id: ImageId,
    pub cell_rect: CellRect,
    pub pixel_src: PixelRect,
    pub page_target: RenderTargetSpec,
    pub max_scroll_x: u32,
}
