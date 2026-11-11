use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

/// Clears areas in `total_area` that fall outside `fitted_rect` (letterbox / pillarbox bars).
pub fn clear_letterbox(buf: &mut Buffer, total_area: Rect, fitted_rect: Rect, bg_color: Color) {
    for y in total_area.top()..total_area.bottom() {
        for x in total_area.left()..total_area.right() {
            let inside_fitted = x >= fitted_rect.left()
                && x < fitted_rect.right()
                && y >= fitted_rect.top()
                && y < fitted_rect.bottom();

            if !inside_fitted {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ').set_bg(bg_color);
                }
            }
        }
    }
}

/// Prepares a clear canvas for direct terminal positioning mode.
pub fn emit_direct_canvas(buf: &mut Buffer, rect: Rect, bg_color: Color) {
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ').set_bg(bg_color);
            }
        }
    }
}
