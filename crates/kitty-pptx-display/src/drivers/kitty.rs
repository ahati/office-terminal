use std::io::Cursor;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use kitty_pptx_core::{DisplayError, RgbaSlide};

use crate::driver::*;
use crate::tty::global_tty;

/// Driver for the Kitty Graphics Protocol.
///
/// Images are transmitted as PNG (`f=100`) and presented with direct cursor
/// placement (`a=p`). Unicode placeholder cells are deliberately not used:
/// they require a second, cell-level protocol and are not needed for the
/// full-resolution slide path.
pub struct KittyDriver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
}

impl KittyDriver {
    pub fn new(placement_mode: PlacementMode) -> Self {
        Self {
            placement_mode,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
        }
    }

    pub fn with_cell_aspect_ratio(mut self, ratio: f64) -> Self {
        self.cell_aspect_ratio = ratio;
        self
    }

    pub fn cell_aspect_ratio(&self) -> f64 {
        self.cell_aspect_ratio
    }

    /// Fits dimensions (cols, rows) preserving slide aspect ratio within the given cell limits.
    pub fn fit_slide(&self, bmp: &RgbaSlide, max_cols: u16, max_rows: u16) -> (u16, u16) {
        fit_aspect_ratio(
            bmp.width,
            bmp.height,
            max_cols,
            max_rows,
            self.cell_aspect_ratio,
        )
    }

    /// Formats Kitty transmission chunks for PNG (`f=100`) payloads.
    pub fn format_transmission_chunks(
        id: ImageId,
        width: u32,
        height: u32,
        b64_payload: &str,
    ) -> Vec<String> {
        let chunk_size = 4096;
        let total_len = b64_payload.len();
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut is_first = true;

        if total_len == 0 {
            chunks.push(format!(
                "\x1b_Ga=t,f=100,i={},s={},v={},q=2,m=0;\x1b\\",
                id.0, width, height
            ));
            return chunks;
        }

        while offset < total_len {
            let end = (offset + chunk_size).min(total_len);
            let chunk = &b64_payload[offset..end];
            let more = if end < total_len { 1 } else { 0 };

            let seq = if is_first {
                is_first = false;
                format!(
                    "\x1b_Ga=t,f=100,i={},s={},v={},q=2,m={};{}\x1b\\",
                    id.0, width, height, more, chunk
                )
            } else {
                format!("\x1b_Gm={};{}\x1b\\", more, chunk)
            };

            chunks.push(seq);
            offset = end;
        }

        chunks
    }

    fn encode_png(bmp: &RgbaSlide) -> Result<Vec<u8>, DisplayError> {
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);

        image::write_buffer_with_format(
            &mut cursor,
            &bmp.pixels,
            bmp.width,
            bmp.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| DisplayError::Io(std::io::Error::other(e.to_string())))?;

        Ok(png_bytes)
    }

    fn transmit(&self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let png = Self::encode_png(bmp)?;
        let b64 = BASE64.encode(&png);
        for seq in Self::format_transmission_chunks(id, bmp.width, bmp.height, &b64) {
            tty.write_seq(&seq).map_err(DisplayError::Io)?;
        }
        Ok(())
    }

    /// Escape sequences that present `id` for the given [`Placement`].
    ///
    /// A fixed placement id (`p=1`) replaces the previous placement instead of
    /// accumulating copies on resize or mode switches.
    pub fn present_sequences(id: ImageId, placement: Placement) -> Vec<String> {
        match placement {
            Placement::Cells(rect) => {
                // z=-1 keeps the image behind text so TUI overlays (help modal,
                // drawers) composite above it. Cells in the slide area carry the
                // default background, letting the image show through.
                vec![format!(
                    "\x1b[{};{}H\x1b_Ga=p,i={},p=1,c={},r={},z=-1,q=2\x1b\\",
                    rect.row + 1,
                    rect.col + 1,
                    id.0,
                    rect.cols,
                    rect.rows
                )]
            }
            Placement::CellsSubrect { rect, src } => {
                vec![format!(
                    "\x1b[{};{}H\x1b_Ga=p,i={},p=1,x={},y={},w={},h={},c={},r={},z=-1,C=1,q=2\x1b\\",
                    rect.row + 1,
                    rect.col + 1,
                    id.0,
                    src.x,
                    src.y,
                    src.width,
                    src.height,
                    rect.cols,
                    rect.rows
                )]
            }
            Placement::ScrollbackBlock => {
                vec![format!("\x1b_Ga=p,i={},q=2\x1b\\\n", id.0)]
            }
        }
    }

    fn present_impl(&mut self, id: ImageId, placement: Placement) -> Result<(), DisplayError> {
        let tty = global_tty();
        for seq in Self::present_sequences(id, placement) {
            tty.write_seq(&seq).map_err(DisplayError::Io)?;
        }
        Ok(())
    }
}

impl TerminalGraphicsDriver for KittyDriver {
    fn name(&self) -> &'static str {
        "kitty"
    }

    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            terminal_side_scaling: true,
            resident_ids: RESIDENT_IMAGE_IDS,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        self.transmit(bmp, id)
    }

    fn present(&mut self, id: ImageId, placement: Placement) -> Result<(), DisplayError> {
        self.present_impl(id, placement)
    }

    fn reposition(&mut self, id: ImageId, placement: Placement) -> Result<(), DisplayError> {
        self.present(id, placement)
    }

    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let seq = format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", id.0);
        tty.write_seq(&seq).map_err(DisplayError::Io)
    }

    fn release_all(&mut self) -> Result<(), DisplayError> {
        let tty = global_tty();
        tty.write_seq("\x1b_Ga=d,d=a,q=2\x1b\\")
            .map_err(DisplayError::Io)
    }
}

impl StageSink for KittyDriver {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError> {
        self.transmit(bmp, id)
    }

    fn release(&self, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let seq = format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", id.0);
        tty.write_seq(&seq).map_err(DisplayError::Io)
    }

    fn resident_capacity(&self) -> u32 {
        RESIDENT_IMAGE_IDS
    }
}
