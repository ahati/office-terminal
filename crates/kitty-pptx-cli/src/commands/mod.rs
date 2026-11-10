use std::path::PathBuf;
use kitty_pptx_core::Source;

pub mod export;
pub mod info;
pub mod dump;
pub mod view;
pub mod present;
pub mod completions;

pub use export::run_export;
pub use info::run_info;
pub use dump::run_dump;
pub use view::run_view;
pub use present::run_interactive;
pub use completions::*;

/// Maps `"-"` to `Source::Stdin`, and any other path to `Source::File(p)`.
pub fn source_from_path(p: PathBuf) -> Source {
    if p.as_os_str() == "-" {
        Source::Stdin
    } else {
        Source::File(p)
    }
}
