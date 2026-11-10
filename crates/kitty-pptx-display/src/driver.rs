use kitty_pptx_core::{sanitize_hyperlink_url, DisplayError, HyperlinkTarget, RgbaSlide, SlideLink};

pub use crate::geometry::*;
pub use crate::drivers::{
    HalfBlockDriver, Iterm2Driver, KittyDriver, KittyFlavor, SixelDriver, TransmissionFormat,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32);

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
    if slide_width_emu <= 0
        || slide_height_emu <= 0
        || placement.cols == 0
        || placement.rows == 0
    {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    Cells(CellRect),
    ScrollbackBlock,
    VirtualPlaceholder(CellRect),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacementMode {
    Interactive,
    Scrollback,
}

#[derive(Clone, Debug)]
pub struct DriverCaps {
    pub alpha: bool,
    pub placeholders: bool,
    pub terminal_side_scaling: bool,
    pub resident_ids: u32,
}

pub trait TerminalGraphicsDriver: Send {
    fn name(&self) -> &'static str;
    fn caps(&self) -> DriverCaps;
    fn placement_mode(&self) -> PlacementMode;
    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError>;
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    /// Present an image, wrapping linked cell regions in OSC 8 hyperlinks.
    /// Drivers without terminal hyperlink support simply ignore `links`.
    fn present_with_hyperlinks(
        &mut self,
        id: ImageId,
        p: Placement,
        links: &[TerminalHyperlink],
    ) -> Result<(), DisplayError> {
        let _ = links;
        self.present(id, p)
    }
    /// Whether Unicode placeholder rendering is active for this driver.
    ///
    /// Callers must only fill the screen with U+10EEEE cells when this returns
    /// `true`; otherwise placeholder cells render as literal text on terminals
    /// that do not implement the Kitty Unicode placeholder extension.
    fn supports_placeholders(&self) -> bool {
        self.caps().placeholders
    }
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError>;
    fn release_all(&mut self) -> Result<(), DisplayError>;
}

pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32;
}
