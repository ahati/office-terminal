pub mod driver;
pub mod geometry;
pub mod tty;
pub mod drivers;
pub mod probe;

// Aliases for submodule-level imports
pub mod kitty {
    pub use crate::drivers::kitty::*;
}
pub mod halfblock {
    pub use crate::drivers::halfblock::*;
}
pub mod sixel {
    pub use crate::drivers::sixel::*;
}
pub mod iterm2 {
    pub use crate::drivers::iterm2::*;
}

pub use driver::*;
pub use tty::*;
pub use drivers::*;
pub use probe::*;
