use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use kitty_pptx_core::{RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

/// Driver for terminals supporting the iTerm2 inline images protocol (e.g. iTerm2, WezTerm).
pub struct Iterm2Driver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
    staged: Arc<Mutex<HashMap<u32, (RgbaSlide, String)>>>, // id -> (slide, base64_png)
}

impl Iterm2Driver {
    pub fn new(placement_mode: PlacementMode) -> Self {
        Self {
            placement_mode,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
            staged: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_cell_aspect_ratio(mut self, ratio: f64) -> Self {
        self.cell_aspect_ratio = ratio;
        self
    }

    pub fn cell_aspect_ratio(&self) -> f64 {
        self.cell_aspect_ratio
    }

    /// Fits dimensions (cols, rows) preserving slide aspect ratio.
    pub fn fit_slide(&self, bmp: &RgbaSlide, max_cols: u16, max_rows: u16) -> (u16, u16) {
        fit_aspect_ratio(bmp.width, bmp.height, max_cols, max_rows, self.cell_aspect_ratio)
    }

    /// Encodes an RgbaSlide to a base64-encoded PNG string.
    pub fn encode_png_b64(slide: &RgbaSlide) -> Result<String, DisplayError> {
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);

        image::write_buffer_with_format(
            &mut cursor,
            &slide.pixels,
            slide.width,
            slide.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ).map_err(|e| DisplayError::Io(std::io::Error::other(e.to_string())))?;

        Ok(BASE64.encode(&png_bytes))
    }

    /// Formats an iTerm2 inline image escape sequence.
    pub fn format_iterm2_sequence(b64: &str, cols: Option<u16>, rows: Option<u16>) -> String {
        let size_params = match (cols, rows) {
            (Some(c), Some(r)) => format!(";width={c};height={r}"),
            (Some(c), None) => format!(";width={c}"),
            (None, Some(r)) => format!(";height={r}"),
            (None, None) => ";width=auto;height=auto".to_string(),
        };

        format!("\x1b]1337;File=inline=1{size_params}:{b64}\x07")
    }
}

impl TerminalGraphicsDriver for Iterm2Driver {
    fn name(&self) -> &'static str {
        "iterm2"
    }

    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            placeholders: false,
            terminal_side_scaling: true,
            resident_ids: 16,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let b64 = Self::encode_png_b64(bmp)?;
        let mut map = self.staged.lock().unwrap();
        map.insert(id.0, (bmp.clone(), b64));
        Ok(())
    }

    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        let (_slide, b64) = {
            let map = self.staged.lock().unwrap();
            map.get(&id.0).cloned().ok_or(DisplayError::NotStaged(id.0))?
        };

        let tty = global_tty();

        match p {
            Placement::Cells(rect) | Placement::VirtualPlaceholder(rect) => {
                let seq = format!(
                    "\x1b[{};{}H{}",
                    rect.row + 1,
                    rect.col + 1,
                    Self::format_iterm2_sequence(&b64, Some(rect.cols), Some(rect.rows))
                );
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
            Placement::ScrollbackBlock => {
                let seq = format!("{}\n", Self::format_iterm2_sequence(&b64, None, None));
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
        }

        Ok(())
    }

    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        self.present(id, p)
    }

    fn release(&mut self, id: ImageId) -> Result<(), DisplayError> {
        let mut map = self.staged.lock().unwrap();
        map.remove(&id.0);
        Ok(())
    }

    fn release_all(&mut self) -> Result<(), DisplayError> {
        let mut map = self.staged.lock().unwrap();
        map.clear();
        Ok(())
    }
}

impl StageSink for Iterm2Driver {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError> {
        let b64 = Self::encode_png_b64(bmp)?;
        let mut map = self.staged.lock().unwrap();
        map.insert(id.0, (bmp.clone(), b64));
        Ok(())
    }

    fn release(&self, id: ImageId) -> Result<(), DisplayError> {
        let mut map = self.staged.lock().unwrap();
        map.remove(&id.0);
        Ok(())
    }

    fn resident_capacity(&self) -> u32 {
        16
    }
}
