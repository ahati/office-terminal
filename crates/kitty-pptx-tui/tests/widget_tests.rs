use kitty_pptx_display::{ImageId, PlacementMode};
use kitty_pptx_tui::widget::layout::{
    compute_slide_rect, rect_to_cell_rect, ASPECT_16_9, DEFAULT_CELL_ASPECT_RATIO,
};
use kitty_pptx_tui::widget::SlideWidget;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use std::sync::{Arc, Mutex};

#[test]
fn test_widget_direct_positioning_records_rect() {
    let sink = Arc::new(Mutex::new(None));
    let widget =
        SlideWidget::new(ImageId(10), PlacementMode::Interactive).with_direct_sink(sink.clone());

    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 40));
    widget.render(Rect::new(0, 0, 100, 40), &mut buf);

    let recorded = sink.lock().unwrap().expect("CellRect must be recorded");
    let expected_fitted = compute_slide_rect(
        Rect::new(0, 0, 100, 40),
        ASPECT_16_9,
        DEFAULT_CELL_ASPECT_RATIO,
    );
    assert_eq!(recorded, rect_to_cell_rect(expected_fitted));

    let center_cell = buf
        .cell((
            expected_fitted.x + expected_fitted.width / 2,
            expected_fitted.y + expected_fitted.height / 2,
        ))
        .unwrap();
    assert_eq!(center_cell.symbol(), " ");
}

#[test]
fn test_widget_zero_dimensions() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
    let widget = SlideWidget::new(ImageId(1), PlacementMode::Interactive);

    (&widget).render(Rect::new(0, 0, 0, 10), &mut buf);
    (&widget).render(Rect::new(0, 0, 10, 0), &mut buf);
}
