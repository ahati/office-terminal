use std::sync::{Arc, Mutex};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use kitty_pptx_display::{ImageId, PlacementMode};
use kitty_pptx_tui::widget::layout::{
    compute_slide_rect, rect_to_cell_rect, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO,
};
use kitty_pptx_tui::widget::placeholder::{
    encode_grid_coords_color, encode_image_id_color, KITTY_PLACEHOLDER_CHAR,
};
use kitty_pptx_tui::widget::{SlideRenderMode, SlideWidget};

#[test]
fn test_widget_unicode_placeholder_rendering() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    let widget = SlideWidget::placeholder(ImageId(5))
        .with_letterbox_bg(Color::Black);

    widget.render(Rect::new(0, 0, 80, 24), &mut buf);

    let fitted = compute_slide_rect(
        Rect::new(0, 0, 80, 24),
        ASPECT_16_9,
        DEFAULT_CELL_ASPECT_RATIO,
    );

    // Verify cell inside fitted rect has U+10EEEE and proper colors
    let cell_inside = buf.cell((fitted.x + 2, fitted.y + 2)).unwrap();
    assert_eq!(cell_inside.symbol(), KITTY_PLACEHOLDER_CHAR.to_string());
    assert_eq!(cell_inside.fg, encode_image_id_color(ImageId(5)));
    assert_eq!(cell_inside.bg, encode_grid_coords_color(2, 2));

    // Verify letterbox outside fitted rect is cleared with black
    if fitted.y > 0 {
        let cell_letterbox = buf.cell((0, 0)).unwrap();
        assert_eq!(cell_letterbox.symbol(), " ");
        assert_eq!(cell_letterbox.bg, Color::Black);
    }
}

#[test]
fn test_widget_direct_positioning_mode() {
    let sink = Arc::new(Mutex::new(None));
    let widget = SlideWidget::new(ImageId(10), PlacementMode::Interactive)
        .with_render_mode(SlideRenderMode::DirectPositioning)
        .with_direct_sink(sink.clone());

    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 40));
    widget.render(Rect::new(0, 0, 100, 40), &mut buf);

    let recorded = sink.lock().unwrap().expect("CellRect must be recorded");
    let expected_fitted = compute_slide_rect(
        Rect::new(0, 0, 100, 40),
        ASPECT_16_9,
        DEFAULT_CELL_ASPECT_RATIO,
    );
    assert_eq!(recorded, rect_to_cell_rect(expected_fitted));

    // In direct mode, fitted canvas should NOT contain U+10EEEE placeholders
    let center_cell = buf.cell((expected_fitted.x + expected_fitted.width / 2, expected_fitted.y + expected_fitted.height / 2)).unwrap();
    assert_eq!(center_cell.symbol(), " ");
}

#[test]
fn test_widget_zero_dimensions() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
    let widget = SlideWidget::placeholder(ImageId(1));

    // Zero width
    (&widget).render(Rect::new(0, 0, 0, 10), &mut buf);
    // Zero height
    (&widget).render(Rect::new(0, 0, 10, 0), &mut buf);
}
