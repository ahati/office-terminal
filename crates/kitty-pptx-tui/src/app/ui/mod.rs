pub mod normal;
pub mod presenter;
pub mod help;
pub mod blank;
pub mod sorter;
pub mod comments;
pub mod warnings;
pub mod search;

use ratatui::Frame;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;

pub use blank::render_blank_screen;
pub use comments::render_comments_drawer;
pub use help::render_help_modal;
pub use normal::render_normal;
pub use presenter::render_presenter;
pub use search::render_search_modal;
pub use sorter::render_sorter;
pub use warnings::render_warning_drawer;

/// Render the presentation UI based on the current state.
pub fn render_app(state: &PresentationState, f: &mut Frame) {
    let area = f.area();

    // Blank screen overrides normal display
    if state.is_blank() {
        render_blank_screen(state.blank_mode(), f, area);
        return;
    }

    // Determine the base view to render
    let base_mode = if state.mode().is_base_mode() {
        state.mode()
    } else if state.previous_mode().is_base_mode() {
        state.previous_mode()
    } else {
        AppMode::Fullscreen
    };

    match base_mode {
        AppMode::Fullscreen => render_normal(state, f, area),
        AppMode::PresenterConsole => render_presenter(state, f, area),
        AppMode::Sorter => render_sorter(state, f, area),
        _ => render_normal(state, f, area),
    }

    // Render active overlay on top of base views
    match state.mode() {
        AppMode::HelpModal => render_help_modal(f, area),
        AppMode::CommentsDrawer => render_comments_drawer(state, f, area),
        AppMode::WarningDrawer => render_warning_drawer(state, f, area),
        AppMode::TocSearchModal => render_search_modal(state, f, area),
        _ => {}
    }
}
