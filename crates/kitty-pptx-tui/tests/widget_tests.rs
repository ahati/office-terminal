use kitty_pptx_display::{ImageId, PlacementMode};
use kitty_pptx_tui::widget::layout::{
    compute_slide_rect, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO,
};
use kitty_pptx_tui::widget::SlideWidget;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

#[test]
fn test_widget_fits_the_slide_and_reserves_the_canvas() {
    let widget =
        SlideWidget::new(ImageId(10), PlacementMode::Interactive).with_aspect_ratio(ASPECT_16_9);

    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 40));
    widget.render(Rect::new(0, 0, 100, 40), &mut buf);

    let expected_fitted = compute_slide_rect(
        Rect::new(0, 0, 100, 40),
        ASPECT_16_9,
        DEFAULT_CELL_ASPECT_RATIO,
    );

    // The fitted area becomes a transparent graphics canvas.
    let center_cell = buf
        .cell((
            expected_fitted.x + expected_fitted.width / 2,
            expected_fitted.y + expected_fitted.height / 2,
        ))
        .unwrap();
    assert_eq!(center_cell.symbol(), " ");
    assert_eq!(center_cell.bg, ratatui::style::Color::Reset);

    // Cells outside the fitted area are letterboxed with the widget's
    // background instead of keeping stale buffer content.
    if expected_fitted.y > 0 {
        let bar = buf.cell((0, 0)).unwrap();
        assert_eq!(bar.bg, ratatui::style::Color::Black);
    }
}

#[test]
fn test_widget_zero_dimensions() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
    let widget = SlideWidget::new(ImageId(1), PlacementMode::Interactive);

    (&widget).render(Rect::new(0, 0, 0, 10), &mut buf);
    (&widget).render(Rect::new(0, 0, 10, 0), &mut buf);
}
