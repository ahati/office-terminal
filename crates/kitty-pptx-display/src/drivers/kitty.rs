use std::io::Cursor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use kitty_pptx_core::{sanitize_hyperlink_url, RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

/// Driver for Kitty Graphics Protocol.
pub struct KittyDriver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
    use_placeholders: bool,
}

impl KittyDriver {
    pub fn new(placement_mode: PlacementMode) -> Self {
        Self {
            placement_mode,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
            use_placeholders: false,
        }
    }

    pub fn with_cell_aspect_ratio(mut self, ratio: f64) -> Self {
        self.cell_aspect_ratio = ratio;
        self
    }

    pub fn with_placeholder_mode(mut self, enabled: bool) -> Self {
        self.use_placeholders = enabled;
        self
    }

    pub fn cell_aspect_ratio(&self) -> f64 {
        self.cell_aspect_ratio
    }

    pub fn is_placeholder_mode(&self) -> bool {
        self.use_placeholders
    }

    /// Fits dimensions (cols, rows) preserving slide aspect ratio within the given cell limits.
    pub fn fit_slide(&self, bmp: &RgbaSlide, max_cols: u16, max_rows: u16) -> (u16, u16) {
        fit_aspect_ratio(bmp.width, bmp.height, max_cols, max_rows, self.cell_aspect_ratio)
    }

    /// Generates the Unicode placeholder terminal string for the specified image and rect.
    ///
    /// Each cell contains U+10EEEE, with:
    /// - 24-bit FG RGB encoding image ID: (id % 256, (id >> 8) % 256, (id >> 16) % 256)
    /// - 24-bit BG RGB encoding cell offset (col, row): (col % 256, col / 256, row % 256)
    pub fn generate_placeholder_string(id: ImageId, rect: CellRect) -> String {
        let mut out = String::with_capacity((rect.rows as usize) * (rect.cols as usize) * 40);
        let fg_r = (id.0 & 0xFF) as u8;
        let fg_g = ((id.0 >> 8) & 0xFF) as u8;
        let fg_b = ((id.0 >> 16) & 0xFF) as u8;

        for r in 0..rect.rows {
            // Position cursor at beginning of row
            out.push_str(&format!("\x1b[{};{}H", rect.row + 1 + r, rect.col + 1));
            // Set foreground color once for row
            out.push_str(&format!("\x1b[38;2;{};{};{}m", fg_r, fg_g, fg_b));

            for c in 0..rect.cols {
                let bg_r = (c & 0xFF) as u8;
                let bg_g = ((c >> 8) & 0xFF) as u8;
                let bg_b = (r & 0xFF) as u8;
                out.push_str(&format!("\x1b[48;2;{};{};{}m\u{10EEEE}", bg_r, bg_g, bg_b));
            }
            out.push_str("\x1b[0m");
        }

        out
    }

    /// Like [`Self::generate_placeholder_string`], but wraps placeholder cells
    /// covered by a hyperlink in OSC 8 sequences (PRD 10 §4.2).
    ///
    /// With no valid links the output is byte-identical to the plain generator.
    pub fn generate_placeholder_string_with_links(
        id: ImageId,
        rect: CellRect,
        links: &[TerminalHyperlink],
    ) -> String {
        let valid: Vec<(&TerminalHyperlink, String)> = links
            .iter()
            .filter_map(|link| {
                sanitize_hyperlink_url(&link.url)
                    .ok()
                    .map(|url| (link, url))
            })
            .collect();
        if valid.is_empty() {
            return Self::generate_placeholder_string(id, rect);
        }

        let mut out = String::with_capacity((rect.rows as usize) * (rect.cols as usize) * 64);
        let fg_r = (id.0 & 0xFF) as u8;
        let fg_g = ((id.0 >> 8) & 0xFF) as u8;
        let fg_b = ((id.0 >> 16) & 0xFF) as u8;

        for r in 0..rect.rows {
            out.push_str(&format!("\x1b[{};{}H", rect.row + 1 + r, rect.col + 1));
            out.push_str(&format!("\x1b[38;2;{};{};{}m", fg_r, fg_g, fg_b));

            let mut active: Option<(&TerminalHyperlink, &str)> = None;
            for c in 0..rect.cols {
                let absolute_col = rect.col.saturating_add(c);
                let absolute_row = rect.row.saturating_add(r);
                let cell_link = valid
                    .iter()
                    .rev()
                    .find(|(link, _)| link.rect.contains(absolute_col, absolute_row));

                let next = cell_link.map(|(link, url)| (*link, url.as_str()));
                let changed = match (active, next) {
                    (None, None) => false,
                    (Some((a, a_url)), Some((b, b_url))) => a.id != b.id || a_url != b_url,
                    _ => true,
                };
                if changed {
                    if active.is_some() {
                        out.push_str("\x1b]8;;\x1b\\");
                    }
                    if let Some((link, url)) = next {
                        out.push_str(&format!("\x1b]8;id=link_{};{}\x1b\\", link.id, url));
                    }
                    active = next;
                }

                let bg_r = (c & 0xFF) as u8;
                let bg_g = ((c >> 8) & 0xFF) as u8;
                let bg_b = (r & 0xFF) as u8;
                out.push_str(&format!("\x1b[48;2;{};{};{}m\u{10EEEE}", bg_r, bg_g, bg_b));
            }
            if active.is_some() {
                out.push_str("\x1b]8;;\x1b\\");
            }
            out.push_str("\x1b[0m");
        }

        out
    }

    /// Formats Kitty transmission chunk sequences for image transmission.
    pub fn format_transmission_chunks(id: ImageId, width: u32, height: u32, b64_payload: &str) -> Vec<String> {
        let chunk_size = 4096;
        let total_len = b64_payload.len();
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut is_first = true;

        if total_len == 0 {
            chunks.push(format!("\x1b_Ga=t,f=100,i={},s={},v={},q=2,m=0;\x1b\\", id.0, width, height));
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

    fn present_impl(
        &mut self,
        id: ImageId,
        p: Placement,
        links: &[TerminalHyperlink],
    ) -> Result<(), DisplayError> {
        let tty = global_tty();
        match p {
            Placement::Cells(rect) => {
                if self.use_placeholders {
                    let seq = Self::generate_placeholder_string_with_links(id, rect, links);
                    tty.write_seq(&seq).map_err(DisplayError::Io)?;
                } else {
                    let seq = format!(
                        "\x1b[{};{}H\x1b_Ga=p,i={},c={},r={},q=2\x1b\\",
                        rect.row + 1, rect.col + 1, id.0, rect.cols, rect.rows
                    );
                    tty.write_seq(&seq).map_err(DisplayError::Io)?;
                }
            }
            Placement::VirtualPlaceholder(rect) => {
                let seq = Self::generate_placeholder_string_with_links(id, rect, links);
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
            Placement::ScrollbackBlock => {
                let seq = format!("\x1b_Ga=p,i={},q=2\x1b\\\n", id.0);
                tty.write_seq(&seq).map_err(DisplayError::Io)?;
            }
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
            placeholders: true,
            terminal_side_scaling: true,
            resident_ids: 8,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        // 1. Encode PNG
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);

        image::write_buffer_with_format(
            &mut cursor,
            &bmp.pixels,
            bmp.width,
            bmp.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ).map_err(|e| DisplayError::Io(std::io::Error::other(e.to_string())))?;

        // 2. Base64 encode
        let b64 = BASE64.encode(&png_bytes);

        // 3. Slice into 4096-byte chunks and transmit to TTY
        let tty = global_tty();
        let chunks = Self::format_transmission_chunks(id, bmp.width, bmp.height, &b64);
        for seq in chunks {
            tty.write_seq(&seq).map_err(DisplayError::Io)?;
        }

        Ok(())
    }

    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError> {
        self.present_impl(id, p, &[])
    }

    fn present_with_hyperlinks(
        &mut self,
        id: ImageId,
        p: Placement,
        links: &[TerminalHyperlink],
    ) -> Result<(), DisplayError> {
        self.present_impl(id, p, links)
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

impl StageSink for KittyDriver {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError> {
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);

        image::write_buffer_with_format(
            &mut cursor,
            &bmp.pixels,
            bmp.width,
            bmp.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ).map_err(|e| DisplayError::Io(std::io::Error::other(e.to_string())))?;

        let b64 = BASE64.encode(&png_bytes);
        let tty = global_tty();
        let chunks = Self::format_transmission_chunks(id, bmp.width, bmp.height, &b64);
        for seq in chunks {
            tty.write_seq(&seq).map_err(DisplayError::Io)?;
        }
        Ok(())
    }

    fn release(&self, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let seq = format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", id.0);
        tty.write_seq(&seq).map_err(DisplayError::Io)
    }

    fn resident_capacity(&self) -> u32 {
        8
    }
}
