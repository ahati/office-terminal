use office_terminal_core::Source;
use std::path::PathBuf;

pub mod completions;
pub mod dump;
pub mod dump_ast;
pub mod export;
pub mod info;
pub mod present;
pub mod view;

pub use completions::*;
pub use dump::run_dump;
pub use export::run_export;
pub use info::run_info;
pub use present::run_interactive;
pub use view::run_view;

/// Maps `"-"` to `Source::Stdin`, and any other path to `Source::File(p)`.
pub fn source_from_path(p: PathBuf) -> Source {
    if p.as_os_str() == "-" {
        Source::Stdin
    } else {
        Source::File(p)
    }
}
