use kitty_pptx_core::{RgbaSlide, DisplayError};

pub use crate::geometry::*;
pub use crate::drivers::{KittyDriver, HalfBlockDriver, SixelDriver, Iterm2Driver};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
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
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError>;
    fn release_all(&mut self) -> Result<(), DisplayError>;
}

pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32;
}
