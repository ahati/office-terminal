use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn fill_rect(buf: &mut Buffer, rect: Rect, bg_color: Color) {
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ').set_bg(bg_color);
            }
        }
    }
}

/// Clears areas in `total_area` that fall outside `fitted_rect` (letterbox / pillarbox bars).
pub fn clear_letterbox(buf: &mut Buffer, total_area: Rect, fitted_rect: Rect, bg_color: Color) {
    let fitted = fitted_rect.intersection(total_area);
    if fitted.is_empty() {
        fill_rect(buf, total_area, bg_color);
        return;
    }
    // Top bar
    if fitted.top() > total_area.top() {
        fill_rect(
            buf,
            Rect::new(
                total_area.x,
                total_area.top(),
                total_area.width,
                fitted.top() - total_area.top(),
            ),
            bg_color,
        );
    }
    // Bottom bar
    if total_area.bottom() > fitted.bottom() {
        fill_rect(
            buf,
            Rect::new(
                total_area.x,
                fitted.bottom(),
                total_area.width,
                total_area.bottom() - fitted.bottom(),
            ),
            bg_color,
        );
    }
    // Left bar (between fitted.top and fitted.bottom)
    if fitted.left() > total_area.left() {
        fill_rect(
            buf,
            Rect::new(
                total_area.x,
                fitted.top(),
                fitted.left() - total_area.left(),
                fitted.height,
            ),
            bg_color,
        );
    }
    // Right bar (between fitted.top and fitted.bottom)
    if total_area.right() > fitted.right() {
        fill_rect(
            buf,
            Rect::new(
                fitted.right(),
                fitted.top(),
                total_area.right() - fitted.right(),
                fitted.height,
            ),
            bg_color,
        );
    }
}

/// Prepares a clear canvas for direct terminal positioning mode.
pub fn emit_direct_canvas(buf: &mut Buffer, rect: Rect, bg_color: Color) {
    fill_rect(buf, rect, bg_color);
}
