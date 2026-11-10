use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use kitty_pptx_core::{RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

/// Driver for Sixel-capable terminals (e.g. Foot, mintty, mlterm, xterm -ti vt340).
pub struct SixelDriver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
    staged: Arc<Mutex<HashMap<u32, RgbaSlide>>>,
}

impl SixelDriver {
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

    /// Encodes RGBA pixels into a valid Sixel escape sequence string.
    pub fn encode_sixel(pixels: &[u8], width: u32, height: u32) -> String {
        if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
            return "\x1bPq\x1b\\".to_string();
        }

        // 1. Build palette (max 256 colors)
        let (palette, indexed_pixels) = Self::quantize_image(pixels, width, height);

        let mut out = String::with_capacity((width * height) as usize + 512);

        // Sixel DCS Introducer: \x1bPq followed by raster attributes
        // ("Pan;Pad;Width;Height) per DEC sixel / PRD 03b.
        out.push_str(&format!("\x1bPq\"1;1;{width};{height}"));

        // Define palette: #P;2;R;G;B where R, G, B are percentages 0..100
        for (i, &(r, g, b)) in palette.iter().enumerate() {
            let r_pct = ((r as u32 * 100) + 127) / 255;
            let g_pct = ((g as u32 * 100) + 127) / 255;
            let b_pct = ((b as u32 * 100) + 127) / 255;
            out.push_str(&format!("#{i};2;{r_pct};{g_pct};{b_pct}"));
        }

        let w = width as usize;
        let h = height as usize;

        // Process in horizontal bands of 6 vertical pixels
        for band_y in (0..h).step_by(6) {
            let band_height = (h - band_y).min(6);

            // Find all unique color indices present in this band
            let mut colors_in_band = HashSet::new();
            for dy in 0..band_height {
                let row_offset = (band_y + dy) * w;
                for x in 0..w {
                    colors_in_band.insert(indexed_pixels[row_offset + x]);
                }
            }

            let mut sorted_colors: Vec<u8> = colors_in_band.into_iter().collect();
            sorted_colors.sort_unstable();

            let num_colors = sorted_colors.len();
            for (color_idx, &color) in sorted_colors.iter().enumerate() {
                // Select color
                out.push_str(&format!("#{color}"));

                // Build run-length encoded characters for this color layer
                let mut current_char = 0u8;
                let mut repeat_count = 0usize;

                for x in 0..w {
                    let mut sixel_val = 0u8;
                    for dy in 0..band_height {
                        let idx = (band_y + dy) * w + x;
                        if indexed_pixels[idx] == color {
                            sixel_val |= 1 << dy;
                        }
                    }
                    let ch = 0x3F + sixel_val; // Sixel character

                    if repeat_count == 0 {
                        current_char = ch;
                        repeat_count = 1;
                    } else if ch == current_char {
                        repeat_count += 1;
                    } else {
                        Self::write_sixel_run(&mut out, current_char, repeat_count);
                        current_char = ch;
                        repeat_count = 1;
                    }
                }

                if repeat_count > 0 {
                    Self::write_sixel_run(&mut out, current_char, repeat_count);
                }

                // Carriage return ($) to redraw current band with next color, unless this is the last color
                if color_idx + 1 < num_colors {
                    out.push('$');
                }
            }

            // Newline (-) to move to next 6-pixel band
            out.push('-');
        }

        // Sixel String Terminator: \x1b\
        out.push_str("\x1b\\");
        out
    }

    fn write_sixel_run(out: &mut String, ch: u8, count: usize) {
        if count == 0 {
            return;
        }
        if count <= 3 {
            for _ in 0..count {
                out.push(ch as char);
            }
        } else {
            out.push_str(&format!("!{}{}", count, ch as char));
        }
    }

    /// Quantizes RGBA image into a palette of up to 256 RGB colors and an indexed pixel buffer.
    fn quantize_image(pixels: &[u8], width: u32, height: u32) -> (Vec<(u8, u8, u8)>, Vec<u8>) {
        let total_pixels = (width * height) as usize;
        let mut distinct_colors = HashMap::new();
        let mut color_list = Vec::new();

        // Sample / scan colors (with alpha blending against black background)
        for i in 0..total_pixels {
            let offset = i * 4;
            let a = pixels[offset + 3] as u32;
            let r = ((pixels[offset] as u32 * a) / 255) as u8;
            let g = ((pixels[offset + 1] as u32 * a) / 255) as u8;
            let b = ((pixels[offset + 2] as u32 * a) / 255) as u8;
            let color = (r, g, b);

            if !distinct_colors.contains_key(&color) && distinct_colors.len() < 256 {
                let idx = distinct_colors.len() as u8;
                distinct_colors.insert(color, idx);
                color_list.push(color);
            }
        }

        // If <= 256 unique colors, we have an exact palette!
        if distinct_colors.len() <= 256 {
            let mut indexed = Vec::with_capacity(total_pixels);
            for i in 0..total_pixels {
                let offset = i * 4;
                let a = pixels[offset + 3] as u32;
                let r = ((pixels[offset] as u32 * a) / 255) as u8;
                let g = ((pixels[offset + 1] as u32 * a) / 255) as u8;
                let b = ((pixels[offset + 2] as u32 * a) / 255) as u8;
                let color = (r, g, b);
                if let Some(&idx) = distinct_colors.get(&color) {
                    indexed.push(idx);
                } else {
                    // Match to closest in color_list
                    let closest = Self::find_closest_color(color, &color_list);
                    indexed.push(closest);
                }
            }
            return (color_list, indexed);
        }

        // Otherwise generate a standard 6x6x6 color cube (216 colors) + 24 grayscale ramp = 240 colors
        let mut palette = Vec::with_capacity(256);
        for r_step in 0..6 {
            for g_step in 0..6 {
                for b_step in 0..6 {
                    let r = ((r_step * 255) / 5) as u8;
                    let g = ((g_step * 255) / 5) as u8;
                    let b = ((b_step * 255) / 5) as u8;
                    palette.push((r, g, b));
                }
            }
        }
        for gray_step in 0..24 {
            let val = ((gray_step * 255) / 23) as u8;
            palette.push((val, val, val));
        }

        let mut indexed = Vec::with_capacity(total_pixels);
        for i in 0..total_pixels {
            let offset = i * 4;
            let a = pixels[offset + 3] as u32;
            let r = ((pixels[offset] as u32 * a) / 255) as u8;
            let g = ((pixels[offset + 1] as u32 * a) / 255) as u8;
            let b = ((pixels[offset + 2] as u32 * a) / 255) as u8;

            // Direct mapping to 6x6x6 cube
            let r_i = ((r as u32 * 5 + 127) / 255).min(5) as u8;
            let g_i = ((g as u32 * 5 + 127) / 255).min(5) as u8;
            let b_i = ((b as u32 * 5 + 127) / 255).min(5) as u8;
            let idx = r_i * 36 + g_i * 6 + b_i;
            indexed.push(idx);
        }

        (palette, indexed)
    }

    fn find_closest_color(color: (u8, u8, u8), palette: &[(u8, u8, u8)]) -> u8 {
        let mut min_dist = u64::MAX;
        let mut best_idx = 0u8;

        for (i, &p) in palette.iter().enumerate() {
            let dr = color.0 as i64 - p.0 as i64;
            let dg = color.1 as i64 - p.1 as i64;
            let db = color.2 as i64 - p.2 as i64;
            let dist = (dr * dr + dg * dg + db * db) as u64;
            if dist < min_dist {
                min_dist = dist;
                best_idx = i as u8;
                if dist == 0 {
                    break;
                }
            }
        }

        best_idx
    }
}

impl TerminalGraphicsDriver for SixelDriver {
    fn name(&self) -> &'static str {
        "sixel"
    }

    fn caps(&self) -> DriverCaps {
        DriverCaps {
            alpha: false,
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
                // Typical cell pixel width: ~8-10px, cell pixel height: ~16-20px
                let target_w = (rect.cols as u32) * 10;
                let target_h = (rect.rows as u32) * 20;
                let resized = Self::resize_slide(&slide, target_w, target_h);
                let sixel = Self::encode_sixel(&resized.pixels, resized.width, resized.height);

                let seq = format!("\x1b[{};{}H{}", rect.row + 1, rect.col + 1, sixel);
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
            Placement::ScrollbackBlock => {
                let (cols, rows) = self.fit_slide(&slide, 80, 24);
                let target_w = (cols as u32) * 10;
                let target_h = (rows as u32) * 20;
                let resized = Self::resize_slide(&slide, target_w, target_h);
                let sixel = Self::encode_sixel(&resized.pixels, resized.width, resized.height);
                let seq = format!("{}\n", sixel);
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

impl StageSink for SixelDriver {
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
