pub mod blank;
pub mod comments;
pub mod find;
pub mod help;
pub mod link_picker;
pub mod normal;
pub mod presenter;
pub mod reader;
pub mod reader_layout;
pub mod search;
pub mod sorter;
pub mod warnings;

use crate::app::mode::AppMode;
use crate::app::state::PresentationState;
use crate::widget::SlideWidget;
use kitty_pptx_display::{CellRect, ImageId, PlacementMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::sync::{Arc, Mutex};

pub use blank::render_blank_screen;
pub use comments::render_comments_drawer;
pub use find::render_find_modal;
pub use help::render_help_modal;
pub use link_picker::render_link_picker;
pub use normal::render_normal;
pub use presenter::render_presenter;
pub use reader::render_reader;
pub use search::render_search_modal;
pub use sorter::render_sorter;
pub use warnings::render_warning_drawer;

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
        .with_direct_sink(sink)
}

/// Draws the degraded-render note over a slide canvas whose background render
/// failed. Without it the canvas is silently blank and reads as a broken
/// projector; the note names the slide and points at the warning drawer.
pub(crate) fn render_render_failure_note(state: &PresentationState, f: &mut Frame, area: Rect) {
    let Some((slide, message)) = state.render_failure() else {
        return;
    };
    if slide != state.current_slide() {
        return;
    }
    let badge_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Red)
        .add_modifier(Modifier::BOLD);
    let detail_style = Style::default().fg(Color::Red);
    let first_line = message.lines().next().unwrap_or("render failed");
    let note = Paragraph::new(vec![
        Line::from(Span::styled(" RENDER FAILED ", badge_style)),
        Line::from(Span::styled(
            format!(" slide {} could not be rendered", slide + 1),
            detail_style,
        )),
        Line::from(Span::styled(format!(" {first_line}"), detail_style)),
        Line::from(Span::styled(
            " see warnings for details",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(Block::default().borders(Borders::NONE));
    let height = 4.min(area.height);
    let row = area.y + (area.height.saturating_sub(height)) / 2;
    f.render_widget(note, Rect::new(area.x, row, area.width, height));
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
    let base_mode = state.base_mode();

    match base_mode {
        AppMode::Fullscreen => render_normal(state, f, area),
        AppMode::PresenterConsole => render_presenter(state, f, area),
        AppMode::Sorter => render_sorter(state, f, area),
        AppMode::Reader => render_reader(state, f, area),
        _ => render_normal(state, f, area),
    }

    // Render active overlay on top of base views
    match state.mode() {
        AppMode::HelpModal => render_help_modal(f, area),
        AppMode::CommentsDrawer => render_comments_drawer(state, f, area),
        AppMode::WarningDrawer => render_warning_drawer(state, f, area),
        AppMode::TocSearchModal => render_search_modal(state, f, area),
        AppMode::FindModal => render_find_modal(state, f, area),
        AppMode::LinkPickerModal => render_link_picker(state, f, area),
        _ => {}
    }
}
