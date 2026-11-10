use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use kitty_pptx_display::{ImageId, PlacementMode};

pub struct SlideWidget {
    pub image_id: ImageId,
    pub mode: PlacementMode,
}

impl Widget for SlideWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Under Kitty's U=1 Unicode placeholder mode:
        // We fill the area cells with character U+10EEEE.
        // Foreground color encodes ImageId, Background color encodes (col, row).
        let fg = Color::Rgb(
            (self.image_id.0 % 256) as u8,
            ((self.image_id.0 >> 8) % 256) as u8,
            ((self.image_id.0 >> 16) % 256) as u8,
        );

        for r in 0..area.height {
            for c in 0..area.width {
                let x = area.x + c;
                let y = area.y + r;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    let bg = Color::Rgb((c % 256) as u8, (c / 256) as u8, (r % 256) as u8);
                    cell.set_char('\u{10EEEE}')
                        .set_fg(fg)
                        .set_bg(bg);
                }
            }
        }
    }
}
