pub mod normal;
pub mod presenter;
pub mod help;
pub mod blank;

use ratatui::Frame;
use crate::app::state::{AppMode, PresentationState};

pub use blank::render_blank_screen;
pub use help::render_help_modal;
pub use normal::render_normal;
pub use presenter::render_presenter;

/// Render the presentation UI based on the current state.
pub fn render_app(state: &PresentationState, f: &mut Frame) {
    let area = f.area();

    // Blank screen overrides normal display
    if state.is_blank() {
        render_blank_screen(state.blank_mode(), f, area);
        return;
    }

    match state.mode() {
        AppMode::Fullscreen => {
            render_normal(state, f, area);
        }
        AppMode::PresenterConsole => {
            render_presenter(state, f, area);
        }
        AppMode::HelpModal => {
            // Render underlying fullscreen view, then overlay the help popup modal
            render_normal(state, f, area);
            render_help_modal(f, area);
        }
    }
}
