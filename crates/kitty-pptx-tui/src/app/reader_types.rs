use kitty_pptx_core::RenderTargetSpec;
use kitty_pptx_display::{
    CellRect, ImageId, PixelRect, READER_IMAGE_ID_BASE, READER_IMAGE_ID_SLOTS,
};

/// Stable terminal image id for reader page `page`.
///
/// Ids derive from the page index (not row parity or slot position), so a pane
/// that moves between the primary and next row — or slides between columns —
/// keeps its id and the graphics syncer re-PLACEs it instead of re-encoding
/// and re-transmitting the raster. Pages reuse ids modulo
/// [`READER_IMAGE_ID_SLOTS`] once more panes than the pool are on screen.
pub fn page_image_id(page: usize) -> ImageId {
    ImageId(READER_IMAGE_ID_BASE + ((page as u32) % READER_IMAGE_ID_SLOTS))
}

/// Reader viewport metrics with the canonical cell-size floors.
///
/// Single source of truth for every reader-path layout and scroll calculation:
/// the cached cell probe (one ioctl per resize instead of one per frame) and
/// the `4×8 px` minimum cell floors stay consistent across modules.
/// Returns `(viewport_w_px, viewport_h_px, cell_w_px, cell_h_px)` where the
/// viewport height excludes the one-row status line.
pub fn reader_viewport_metrics(cols: u16, rows: u16) -> (f32, f32, f32, f32) {
    let metrics = kitty_pptx_display::cell_pixel_size_cached();
    let cell_w_px = metrics.width_px.max(4.0) as f32;
    let cell_h_px = metrics.height_px.max(8.0) as f32;
    let viewport_w_px = (cols as f32) * cell_w_px;
    let viewport_h_px = (rows.saturating_sub(1) as f32) * cell_h_px;
    (viewport_w_px, viewport_h_px, cell_w_px, cell_h_px)
}

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
    pub is_primary: bool,
}
