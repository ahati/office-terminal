use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use kitty_pptx_display::ImageId;

/// Kitty Unicode placeholder character (U+10EEEE).
/// Kitty replaces this character with the designated sub-image cell.
pub const KITTY_PLACEHOLDER_CHAR: char = '\u{10EEEE}';

/// Encodes the `ImageId` into an RGB foreground color for the Kitty placeholder protocol.
/// - Red: id & 0xFF
/// - Green: (id >> 8) & 0xFF
/// - Blue: (id >> 16) & 0xFF
pub fn encode_image_id_color(id: ImageId) -> Color {
    Color::Rgb(
        (id.0 % 256) as u8,
        ((id.0 >> 8) % 256) as u8,
        ((id.0 >> 16) % 256) as u8,
    )
}

/// Encodes the zero-based grid coordinate (column, row) inside the placeholder rectangle
/// into an RGB background color for the Kitty placeholder protocol.
/// - Red: col & 0xFF
/// - Green: (col >> 8) & 0xFF
/// - Blue: row & 0xFF
pub fn encode_grid_coords_color(col: u16, row: u16) -> Color {
    Color::Rgb(
        (col % 256) as u8,
        ((col / 256) % 256) as u8,
        (row % 256) as u8,
    )
}

/// Emits the Kitty Unicode placeholder grid into the given buffer across `rect`.
pub fn emit_placeholder_grid(buf: &mut Buffer, rect: Rect, image_id: ImageId) {
    let fg = encode_image_id_color(image_id);

    for r in 0..rect.height {
        for c in 0..rect.width {
            let x = rect.x + c;
            let y = rect.y + r;
            if let Some(cell) = buf.cell_mut((x, y)) {
                let bg = encode_grid_coords_color(c, r);
                cell.set_char(KITTY_PLACEHOLDER_CHAR)
                    .set_fg(fg)
                    .set_bg(bg);
            }
        }
    }
}

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
                    cell.set_char(' ')
                        .set_bg(bg_color);
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
                cell.set_char(' ')
                    .set_bg(bg_color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_image_id_color() {
        let id1 = ImageId(1);
        assert_eq!(encode_image_id_color(id1), Color::Rgb(1, 0, 0));

        let id2 = ImageId(258); // 256 + 2 => (2, 1, 0)
        assert_eq!(encode_image_id_color(id2), Color::Rgb(2, 1, 0));

        let id3 = ImageId(0x123456);
        assert_eq!(encode_image_id_color(id3), Color::Rgb(0x56, 0x34, 0x12));
    }

    #[test]
    fn test_encode_grid_coords_color() {
        let c = encode_grid_coords_color(5, 10);
        assert_eq!(c, Color::Rgb(5, 0, 10));

        let c2 = encode_grid_coords_color(300, 15); // 300 = 256 + 44
        assert_eq!(c2, Color::Rgb(44, 1, 15));
    }

    #[test]
    fn test_emit_placeholder_grid() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 10));
        let rect = Rect::new(2, 2, 3, 2);
        let id = ImageId(7);
        emit_placeholder_grid(&mut buf, rect, id);

        for r in 0..2 {
            for c in 0..3 {
                let cell = buf.cell((2 + c, 2 + r)).unwrap();
                assert_eq!(cell.symbol(), KITTY_PLACEHOLDER_CHAR.to_string());
                assert_eq!(cell.fg, Color::Rgb(7, 0, 0));
                assert_eq!(cell.bg, Color::Rgb(c as u8, 0, r as u8));
            }
        }
    }
}
