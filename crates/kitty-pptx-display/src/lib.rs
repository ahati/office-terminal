//! Kitty graphics protocol driver and terminal cell mapping.
#![forbid(unsafe_code)]
pub mod driver;
pub mod drivers;
pub mod geometry;
pub mod probe;
pub mod transmit;
pub mod tty;

// Alias for submodule-level imports
pub mod kitty {
    pub use crate::drivers::kitty::*;
}

pub use driver::*;
pub use drivers::*;
pub use probe::*;
pub use transmit::*;
pub use tty::*;
