use kitty_pptx_core::{
    sanitize_hyperlink_url, DisplayError, HyperlinkTarget, RgbaSlide, SlideLink, TransmitPayload,
};

pub use crate::drivers::KittyDriver;
pub use crate::geometry::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32);

// ─── Resident image-id pool ────────────────────────────────────────────────
// The interactive TUI partitions the terminal's image namespace into disjoint
// ranges so that releasing one class of images can never delete another:
// current slide `1..=8`, presenter preview `9..=16`, sorter thumbnails
// `17..=80`, document-reader pages `81..=92`.

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
/// Cards downscale the canvas raster to thumbnail dimensions (~480x360),
/// reducing memory to ~500 KB per card. Sixty-four cards comfortably fits
/// large decks within the terminal image quota without premature eviction.
pub const MAX_THUMBNAIL_IMAGES: u32 = 64;
/// First image id used by document-reader page panes.
///
/// Reader mode is exclusive with the slide/sorter views, but its ids stay in
/// a disjoint range so releasing one class of images can never delete
/// another and the documented partition remains machine-checked.
pub const READER_IMAGE_ID_BASE: u32 = THUMBNAIL_IMAGE_ID_BASE + MAX_THUMBNAIL_IMAGES;
/// Number of reader page ids (two spread rows of up to six panes).
pub const READER_IMAGE_ID_SLOTS: u32 = 12;
/// Default terminal graphics buffer memory quota (320 MB in standard Kitty).
pub const DEFAULT_TERMINAL_IMAGE_QUOTA_BYTES: usize = 320 * 1024 * 1024;

/// Returns the effective terminal graphics image quota in bytes,
/// overridable via the `KITTY_PPTX_TERMINAL_QUOTA` environment variable.
pub fn terminal_image_quota_bytes() -> usize {
    std::env::var("KITTY_PPTX_TERMINAL_QUOTA")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(DEFAULT_TERMINAL_IMAGE_QUOTA_BYTES)
}

/// Total number of resident image ids the interactive path can keep alive.
pub const RESIDENT_IMAGE_IDS: u32 = READER_IMAGE_ID_BASE + READER_IMAGE_ID_SLOTS - 1;

// Machine-checked partition: the reader range starts after the last sorter
// thumbnail id, and the resident pool covers the reader range end-to-end.
const _: () = assert!(READER_IMAGE_ID_BASE >= THUMBNAIL_IMAGE_ID_BASE + MAX_THUMBNAIL_IMAGES);
const _: () = assert!(RESIDENT_IMAGE_IDS >= READER_IMAGE_ID_BASE + READER_IMAGE_ID_SLOTS - 1);

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
///
/// Test-only surface: no production caller; exercised by driver tests.
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
///
/// Test-only surface: no production caller; exercised by driver tests.
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

/// Test-only surface: no production reader today; queried by driver mocks/tests.
#[derive(Clone, Debug)]
pub struct DriverCaps {
    pub alpha: bool,
    pub terminal_side_scaling: bool,
    /// How many [`ImageId`] slots the terminal can keep decoded at once.
    pub resident_ids: u32,
}

pub trait TerminalGraphicsDriver: Send {
    /// Test-only surface: returned by every driver but read only in tests.
    fn name(&self) -> &'static str;
    fn caps(&self) -> DriverCaps;
    /// Test-only surface: queried only by driver tests, not the presenter.
    fn placement_mode(&self) -> PlacementMode;
    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError>;
    /// Transmits an already-encoded payload without re-encoding on this thread.
    ///
    /// The payload is produced off-thread by a `kitty_pptx_core::TransmitEncoder`
    /// (raster workers pre-encoding PNG + base64), so the UI thread only
    /// pays the wire write. Drivers without a prebuilt-capable path keep this
    /// default, which reports the capability gap as an I/O error instead of
    /// silently re-encoding.
    fn transmit_prebuilt(
        &mut self,
        _id: ImageId,
        _width: u32,
        _height: u32,
        _payload: &TransmitPayload,
    ) -> Result<(), DisplayError> {
        Err(DisplayError::Io(std::io::Error::other(
            "prebuilt transmit unsupported by this driver",
        )))
    }
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError>;
    fn release_all(&mut self) -> Result<(), DisplayError>;
}

/// Test-only surface: only the driver implements it; no production caller.
pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32;
}
