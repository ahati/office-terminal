use std::io::Cursor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use kitty_pptx_core::{RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

pub struct KittyDriver {
    placement_mode: PlacementMode,
}

impl KittyDriver {
    pub fn new(placement_mode: PlacementMode) -> Self {
        Self { placement_mode }
    }
}

impl TerminalGraphicsDriver for KittyDriver {
    fn name(&self) -> &'static str {
        "kitty"
    }

    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            placeholders: true,
            terminal_side_scaling: true,
            resident_ids: 8,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        // 1. Encode PNG in idle time
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        
        image::write_buffer_with_format(
            &mut cursor,
            &bmp.pixels,
            bmp.width,
            bmp.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ).map_err(|e| DisplayError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        // 2. Base64 encode
        let b64 = BASE64.encode(&png_bytes);

        // 3. Slice into 4096-byte chunks
        let tty = global_tty();
        let chunk_size = 4096;
        let total_len = b64.len();

        let mut offset = 0;
        let mut is_first = true;

        while offset < total_len {
            let end = (offset + chunk_size).min(total_len);
            let chunk = &b64[offset..end];
            let more = if end < total_len { 1 } else { 0 };

            let seq = if is_first {
                is_first = false;
                format!(
                    "\x1b_Ga=t,f=100,i={},s={},v={},U=1,q=2,m={};{}\x1b\\",
                    id.0, bmp.width, bmp.height, more, chunk
                )
            } else {
                format!("\x1b_Gm={};{}\x1b\\", more, chunk)
            };

            tty.write_seq(&seq).map_err(DisplayError::Io)?;
            offset = end;
        }

        Ok(())
    }

    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        let tty = global_tty();
        match p {
            Placement::Cells(rect) => {
                let seq = format!(
                    "\x1b[{};{}H\x1b_Ga=p,i={},c={},r={},q=2\x1b\\",
                    rect.row + 1, rect.col + 1, id.0, rect.cols, rect.rows
                );
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
            Placement::ScrollbackBlock => {
                let seq = format!("\x1b_Ga=p,i={},q=2\x1b\\\n", id.0);
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
        }
        Ok(())
    }

    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        self.present(id, p)
    }

    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let seq = format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", id.0);
        tty.write_seq(&seq).map_err(DisplayError::Io)
    }

    fn release_all(&mut self) -> Result<(), DisplayError> {
        let tty = global_tty();
        // Lowercase d=a deletes all images
        tty.write_seq("\x1b_Ga=d,d=a,q=2\x1b\\").map_err(DisplayError::Io)
    }
}
