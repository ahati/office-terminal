use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use kitty_pptx_core::{RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

/// Fallback terminal graphics driver using UTF-8 half-block characters `▀` (U+2580).
///
/// Each terminal character cell displays 2 vertical pixels:
/// - The upper half is colored via 24-bit ANSI foreground sequence `\x1b[38;2;R;G;Bm`.
/// - The lower half is colored via 24-bit ANSI background sequence `\x1b[48;2;R;G;Bm`.
pub struct HalfBlockDriver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
    staged: Arc<Mutex<HashMap<u32, RgbaSlide>>>,
}

impl HalfBlockDriver {
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

    /// Resizes an RgbaSlide to the exact pixel width and height needed.
    pub fn resize_slide(slide: &RgbaSlide, target_w: u32, target_h: u32) -> RgbaSlide {
        if slide.width == target_w && slide.height == target_h {
            return slide.clone();
        }
        if target_w == 0 || target_h == 0 {
            return RgbaSlide {
                width: target_w,
                height: target_h,
                pixels: Arc::new([]),
            };
        }

        let img = match image::RgbaImage::from_raw(slide.width, slide.height, slide.pixels.to_vec()) {
            Some(i) => i,
            None => return slide.clone(),
        };

        let resized = image::imageops::resize(
            &img,
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );

        RgbaSlide {
            width: target_w,
            height: target_h,
            pixels: Arc::from(resized.into_raw().into_boxed_slice()),
        }
    }

    /// Renders an RgbaSlide to ANSI half-block characters for the specified CellRect.
    pub fn render_halfblock_cells(slide: &RgbaSlide, rect: CellRect) -> String {
        let pixel_w = rect.cols as u32;
        let pixel_h = (rect.rows as u32) * 2;
        let resized = Self::resize_slide(slide, pixel_w, pixel_h);

        let mut out = String::with_capacity((rect.rows as usize) * (rect.cols as usize) * 45);

        for r in 0..rect.rows {
            // Position cursor at row, col
            out.push_str(&format!("\x1b[{};{}H", rect.row + 1 + r, rect.col + 1));

            let top_y = (r as usize) * 2;
            let bot_y = top_y + 1;

            let mut prev_fg = None;
            let mut prev_bg = None;

            for c in 0..rect.cols {
                let x = c as usize;

                let top_idx = (top_y * (pixel_w as usize) + x) * 4;
                let (tr, tg, tb) = if top_idx + 3 < resized.pixels.len() {
                    let a = resized.pixels[top_idx + 3] as u16;
                    (
                        ((resized.pixels[top_idx] as u16 * a) / 255) as u8,
                        ((resized.pixels[top_idx + 1] as u16 * a) / 255) as u8,
                        ((resized.pixels[top_idx + 2] as u16 * a) / 255) as u8,
                    )
                } else {
                    (0, 0, 0)
                };

                let bot_idx = (bot_y * (pixel_w as usize) + x) * 4;
                let (br, bg, bb) = if bot_idx + 3 < resized.pixels.len() {
                    let a = resized.pixels[bot_idx + 3] as u16;
                    (
                        ((resized.pixels[bot_idx] as u16 * a) / 255) as u8,
                        ((resized.pixels[bot_idx + 1] as u16 * a) / 255) as u8,
                        ((resized.pixels[bot_idx + 2] as u16 * a) / 255) as u8,
                    )
                } else {
                    (0, 0, 0)
                };

                // Optimize ANSI sequences by only emitting color changes when colors change
                if prev_fg != Some((tr, tg, tb)) {
                    out.push_str(&format!("\x1b[38;2;{};{};{}m", tr, tg, tb));
                    prev_fg = Some((tr, tg, tb));
                }
                if prev_bg != Some((br, bg, bb)) {
                    out.push_str(&format!("\x1b[48;2;{};{};{}m", br, bg, bb));
                    prev_bg = Some((br, bg, bb));
                }

                out.push('▀');
            }
            out.push_str("\x1b[0m");
        }

        out
    }

    /// Renders an RgbaSlide to a standalone ANSI string of cols x rows cells.
    pub fn render_halfblock_to_ansi(slide: &RgbaSlide, target_cols: u16, target_rows: u16) -> String {
        let rect = CellRect {
            row: 0,
            col: 0,
            rows: target_rows,
            cols: target_cols,
        };
        Self::render_halfblock_cells(slide, rect)
    }

    /// Renders an RgbaSlide as a scrollback block with newline delimiters.
    pub fn render_scrollback_block(slide: &RgbaSlide, cols: u16, rows: u16) -> String {
        let pixel_w = cols as u32;
        let pixel_h = (rows as u32) * 2;
        let resized = Self::resize_slide(slide, pixel_w, pixel_h);

        let mut out = String::with_capacity((rows as usize) * (cols as usize) * 45);

        for r in 0..rows {
            let top_y = (r as usize) * 2;
            let bot_y = top_y + 1;
            let mut prev_fg = None;
            let mut prev_bg = None;

            for c in 0..cols {
                let x = c as usize;
                let top_idx = (top_y * (pixel_w as usize) + x) * 4;
                let (tr, tg, tb) = if top_idx + 3 < resized.pixels.len() {
                    let a = resized.pixels[top_idx + 3] as u16;
                    (
                        ((resized.pixels[top_idx] as u16 * a) / 255) as u8,
                        ((resized.pixels[top_idx + 1] as u16 * a) / 255) as u8,
                        ((resized.pixels[top_idx + 2] as u16 * a) / 255) as u8,
                    )
                } else {
                    (0, 0, 0)
                };

                let bot_idx = (bot_y * (pixel_w as usize) + x) * 4;
                let (br, bg, bb) = if bot_idx + 3 < resized.pixels.len() {
                    let a = resized.pixels[bot_idx + 3] as u16;
                    (
                        ((resized.pixels[bot_idx] as u16 * a) / 255) as u8,
                        ((resized.pixels[bot_idx + 1] as u16 * a) / 255) as u8,
                        ((resized.pixels[bot_idx + 2] as u16 * a) / 255) as u8,
                    )
                } else {
                    (0, 0, 0)
                };

                if prev_fg != Some((tr, tg, tb)) {
                    out.push_str(&format!("\x1b[38;2;{};{};{}m", tr, tg, tb));
                    prev_fg = Some((tr, tg, tb));
                }
                if prev_bg != Some((br, bg, bb)) {
                    out.push_str(&format!("\x1b[48;2;{};{};{}m", br, bg, bb));
                    prev_bg = Some((br, bg, bb));
                }

                out.push('▀');
            }
            out.push_str("\x1b[0m\r\n");
        }

        out
    }
}

impl TerminalGraphicsDriver for HalfBlockDriver {
    fn name(&self) -> &'static str {
        "halfblock"
    }

    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: true,
            placeholders: false,
            terminal_side_scaling: false,
            resident_ids: 16,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let mut map = self.staged.lock().unwrap();
        map.insert(id.0, bmp.clone());
        Ok(())
    }

    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        let slide = {
            let map = self.staged.lock().unwrap();
            map.get(&id.0).cloned().ok_or(DisplayError::NotStaged(id.0))?
        };

        let tty = global_tty();

        match p {
            Placement::Cells(rect) | Placement::VirtualPlaceholder(rect) => {
                let seq = Self::render_halfblock_cells(&slide, rect);
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
            Placement::ScrollbackBlock => {
                let (cols, rows) = self.fit_slide(&slide, 80, 24);
                let seq = Self::render_scrollback_block(&slide, cols, rows);
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

impl StageSink for HalfBlockDriver {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError> {
        let mut map = self.staged.lock().unwrap();
        map.insert(id.0, bmp.clone());
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
