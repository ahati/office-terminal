use kitty_pptx_core::{
    sanitize_hyperlink_url, DisplayError, HyperlinkTarget, RgbaSlide, SlideLink,
};

pub use crate::drivers::KittyDriver;
pub use crate::geometry::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32);

// ─── Resident image-id pool ────────────────────────────────────────────────
// The interactive TUI partitions the terminal's image namespace into disjoint
// ranges so that releasing one class of images can never delete another:
// current slide `1..=8`, presenter preview `9..=16`, sorter thumbnails `17..`.

/// First image id used by the full-screen current-slide slot.
pub const CURRENT_IMAGE_ID_BASE: u32 = 1;
/// Number of rotating image ids for the current slide.
pub const CURRENT_IMAGE_ID_SLOTS: u32 = 8;
/// First image id used by the presenter next-slide preview.
pub const PREVIEW_IMAGE_ID_BASE: u32 = CURRENT_IMAGE_ID_BASE + CURRENT_IMAGE_ID_SLOTS;
/// Number of rotating image ids for the presenter preview.
pub const PREVIEW_IMAGE_ID_SLOTS: u32 = 8;
/// First image id available to slide-sorter thumbnail cards.
pub const THUMBNAIL_IMAGE_ID_BASE: u32 = PREVIEW_IMAGE_ID_BASE + PREVIEW_IMAGE_ID_SLOTS;
/// Maximum number of sorter thumbnails that may be resident at the same time.
///
/// Cards reuse the canvas raster, so a resident card costs as much as a slide
/// at the terminal's canvas target (roughly 1.4 MB at an 800x450 canvas, ~8 MB
/// at 1920x1080). Sixteen cards matches common 4x3/5x3 grids while keeping the
/// grid inside the documented terminal-memory budget.
pub const MAX_THUMBNAIL_IMAGES: u32 = 16;
/// Total number of resident image ids the interactive path can keep alive.
pub const RESIDENT_IMAGE_IDS: u32 = THUMBNAIL_IMAGE_ID_BASE + MAX_THUMBNAIL_IMAGES - 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

impl CellRect {
    pub fn contains(&self, col: u16, row: u16) -> bool {
        col >= self.col
            && col < self.col.saturating_add(self.cols)
            && row >= self.row
            && row < self.row.saturating_add(self.rows)
    }
}

/// A clickable terminal-cell region emitting an OSC 8 hyperlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalHyperlink {
    /// Stable hotkey/OSC-8 `id` parameter.
    pub id: u32,
    pub url: String,
    pub rect: CellRect,
}

impl TerminalHyperlink {
    pub fn new(id: u32, url: impl Into<String>, rect: CellRect) -> Self {
        Self {
            id,
            url: url.into(),
            rect,
        }
    }
}

/// Map slide-space hyperlinks to terminal cells inside a placed slide rect.
///
/// Only validated external URIs are returned; internal jumps and presentation
/// actions are handled by the TUI instead of the terminal protocol.
pub fn map_slide_links_to_cells(
    links: &[SlideLink],
    slide_width_emu: i64,
    slide_height_emu: i64,
    placement: CellRect,
    first_id: u32,
) -> Vec<TerminalHyperlink> {
    if slide_width_emu <= 0 || slide_height_emu <= 0 || placement.cols == 0 || placement.rows == 0 {
        return Vec::new();
    }

    let width = slide_width_emu as f32;
    let height = slide_height_emu as f32;
    let max_col = placement.cols - 1;
    let max_row = placement.rows - 1;
    let mut mapped = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let HyperlinkTarget::ExternalUri(raw_url) = &link.target else {
            continue;
        };
        let Ok(url) = sanitize_hyperlink_url(raw_url) else {
            continue;
        };
        let bounds = link.bounds_emu;
        let x_start = bounds.x.clamp(0.0, width);
        let x_end = bounds.right().clamp(0.0, width);
        let y_start = bounds.y.clamp(0.0, height);
        let y_end = bounds.bottom().clamp(0.0, height);
        if x_end <= x_start || y_end <= y_start {
            continue;
        }

        let col_start = ((x_start / width) * placement.cols as f32).floor() as u16;
        let col_end = (((x_end / width) * placement.cols as f32).floor() as u16).min(max_col);
        let row_start = ((y_start / height) * placement.rows as f32).floor() as u16;
        let row_end = (((y_end / height) * placement.rows as f32).floor() as u16).min(max_row);
        if col_end < col_start || row_end < row_start {
            continue;
        }

        mapped.push(TerminalHyperlink {
            id: first_id.saturating_add(index as u32),
            url,
            rect: CellRect {
                row: placement.row.saturating_add(row_start),
                col: placement.col.saturating_add(col_start),
                rows: row_end - row_start + 1,
                cols: col_end - col_start + 1,
            },
        });
    }
    mapped
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    Cells(CellRect),
    CellsSubrect { rect: CellRect, src: PixelRect },
    ScrollbackBlock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacementMode {
    Interactive,
    Scrollback,
}

#[derive(Clone, Debug)]
pub struct DriverCaps {
    pub alpha: bool,
    pub terminal_side_scaling: bool,
    /// How many [`ImageId`] slots the terminal can keep decoded at once.
    pub resident_ids: u32,
}

pub trait TerminalGraphicsDriver: Send {
    fn name(&self) -> &'static str;
    fn caps(&self) -> DriverCaps;
    fn placement_mode(&self) -> PlacementMode;
    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError>;
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError>;
    fn release_all(&mut self) -> Result<(), DisplayError>;
}

pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32;
}
