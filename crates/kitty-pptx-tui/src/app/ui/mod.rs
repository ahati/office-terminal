pub mod normal;
pub mod presenter;
pub mod help;
pub mod blank;
pub mod sorter;
pub mod comments;
pub mod warnings;
pub mod search;
pub mod link_picker;

use std::sync::{Arc, Mutex};
use ratatui::Frame;
use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::widget::{SlideRenderMode, SlideWidget};
use kitty_pptx_display::{CellRect, ImageId, PlacementMode};

pub use blank::render_blank_screen;
pub use comments::render_comments_drawer;
pub use help::render_help_modal;
pub use normal::render_normal;
pub use presenter::render_presenter;
pub use search::render_search_modal;
pub use sorter::render_sorter;
pub use warnings::render_warning_drawer;
pub use link_picker::render_link_picker;

/// Builds the widget for the main slide canvas, honoring the desired render mode.
pub(crate) fn current_slide_widget(state: &PresentationState) -> SlideWidget {
    slide_widget(state, state.current_image_id(), state.placement_sink())
}

/// Builds the widget for the presenter next-slide preview.
pub(crate) fn preview_slide_widget(state: &PresentationState, image_id: ImageId) -> SlideWidget {
    slide_widget(state, image_id, state.preview_placement_sink())
}

fn slide_widget(
    state: &PresentationState,
    image_id: ImageId,
    sink: Arc<Mutex<Option<CellRect>>>,
) -> SlideWidget {
    SlideWidget::new(image_id, PlacementMode::Interactive)
        .with_aspect_ratio(state.aspect_ratio())
        .with_render_mode(state.slide_render_mode())
        .with_direct_sink(sink)
}

/// Thumbnails are only rendered as Kitty placeholders; on direct-positioning
/// drivers they are left blank rather than emitting a bogus colour grid.
pub(crate) fn thumbnails_supported(state: &PresentationState) -> bool {
    state.slide_render_mode() == SlideRenderMode::Placeholder
}

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
        AppMode::LinkPickerModal => render_link_picker(state, f, area),
        _ => {}
    }
}
