use std::io::Cursor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use kitty_pptx_core::{sanitize_hyperlink_url, RgbaSlide, DisplayError};
use crate::driver::*;
use crate::tty::global_tty;

/// Graphics-protocol flavor implemented by the host terminal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KittyFlavor {
    /// Genuine kitty: PNG transmission (`f=100`) and Unicode placeholders.
    Full,
    /// Partial implementations (WezTerm, Ghostty, Konsole, ...) that speak the
    /// Kitty graphics protocol but cannot be trusted with PNG payloads and/or
    /// Unicode placeholders. Raw RGBA (`f=32`) is universally supported.
    Compat,
}

/// Wire format used to transmit image data over the Kitty graphics protocol.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransmissionFormat {
    /// Uncompressed 32-bit RGBA (`f=32`), base64 encoded.
    RawRgba,
    /// PNG payload (`f=100`), base64 encoded.
    Png,
}

impl TransmissionFormat {
    /// Kitty `f=` key value for this format.
    pub fn kitty_code(self) -> u32 {
        match self {
            Self::RawRgba => 32,
            Self::Png => 100,
        }
    }
}

impl KittyFlavor {
    /// Transmission format used for this flavor.
    pub fn transmission_format(self) -> TransmissionFormat {
        match self {
            // Genuine kitty decodes PNG, which hugely reduces the payload size.
            Self::Full => TransmissionFormat::Png,
            // Raw RGBA avoids depending on PNG support in partial implementations.
            Self::Compat => TransmissionFormat::RawRgba,
        }
    }

    /// Whether the terminal can be trusted with U+10EEEE placeholder cells.
    pub fn supports_placeholders(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Driver for Kitty Graphics Protocol.
pub struct KittyDriver {
    placement_mode: PlacementMode,
    cell_aspect_ratio: f64,
    use_placeholders: bool,
    flavor: KittyFlavor,
}

impl KittyDriver {
    /// Creates a driver with the conservative `Compat` flavor: raw RGBA
    /// transmission and placeholders disabled. Safe for every terminal that
    /// answers the Kitty graphics query.
    pub fn new(placement_mode: PlacementMode) -> Self {
        Self {
            placement_mode,
            cell_aspect_ratio: DEFAULT_CELL_ASPECT_RATIO,
            use_placeholders: false,
            flavor: KittyFlavor::Compat,
        }
    }

    /// Creates a driver for genuine kitty: PNG transmission and optional
    /// Unicode placeholders (still opt-in via [`Self::with_placeholder_mode`]).
    pub fn new_full(placement_mode: PlacementMode) -> Self {
        Self::new(placement_mode).with_flavor(KittyFlavor::Full)
    }

    /// Creates a driver for a terminal that only partially implements the
    /// protocol (raw RGBA transmission, placeholders disabled).
    pub fn new_compat(placement_mode: PlacementMode) -> Self {
        Self::new(placement_mode)
    }

    pub fn with_flavor(mut self, flavor: KittyFlavor) -> Self {
        self.flavor = flavor;
        self
    }

    pub fn flavor(&self) -> KittyFlavor {
        self.flavor
    }

    /// Transmission format that [`TerminalGraphicsDriver::stage`] will use.
    pub fn transmission_format(&self) -> TransmissionFormat {
        self.flavor.transmission_format()
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

    /// Placeholder rendering is only active on genuine kitty with the mode
    /// explicitly enabled; every other terminal gets direct placement.
    pub fn placeholders_active(&self) -> bool {
        self.use_placeholders && self.flavor.supports_placeholders()
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

    /// Formats Kitty transmission chunk sequences for PNG (`f=100`) payloads.
    pub fn format_transmission_chunks(id: ImageId, width: u32, height: u32, b64_payload: &str) -> Vec<String> {
        Self::format_chunks(id, width, height, b64_payload, TransmissionFormat::Png)
    }

    /// Formats Kitty transmission chunk sequences for raw RGBA (`f=32`) payloads.
    pub fn format_transmission_chunks_raw(id: ImageId, width: u32, height: u32, b64_payload: &str) -> Vec<String> {
        Self::format_chunks(id, width, height, b64_payload, TransmissionFormat::RawRgba)
    }

    fn format_chunks(
        id: ImageId,
        width: u32,
        height: u32,
        b64_payload: &str,
        format: TransmissionFormat,
    ) -> Vec<String> {
        let chunk_size = 4096;
        let total_len = b64_payload.len();
        let f = format.kitty_code();
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut is_first = true;

        if total_len == 0 {
            chunks.push(format!("\x1b_Ga=t,f={},i={},s={},v={},q=2,m=0;\x1b\\", f, id.0, width, height));
            return chunks;
        }

        while offset < total_len {
            let end = (offset + chunk_size).min(total_len);
            let chunk = &b64_payload[offset..end];
            let more = if end < total_len { 1 } else { 0 };

            let seq = if is_first {
                is_first = false;
                format!(
                    "\x1b_Ga=t,f={},i={},s={},v={},q=2,m={};{}\x1b\\",
                    f, id.0, width, height, more, chunk
                )
            } else {
                format!("\x1b_Gm={};{}\x1b\\", more, chunk)
            };

            chunks.push(seq);
            offset = end;
        }

        chunks
    }

    /// Encodes an RGBA slide as PNG bytes.
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

    /// Transmits `bmp` to the terminal using the flavor's wire format.
    fn transmit(&self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let tty = global_tty();
        let format = self.transmission_format();
        let chunks = match format {
            TransmissionFormat::RawRgba => {
                let expected = (bmp.width as usize)
                    .saturating_mul(bmp.height as usize)
                    .saturating_mul(4);
                if bmp.pixels.len() != expected {
                    return Err(DisplayError::Io(std::io::Error::other(
                        "RGBA slide buffer does not match its dimensions",
                    )));
                }
                let b64 = BASE64.encode(&bmp.pixels);
                Self::format_transmission_chunks_raw(id, bmp.width, bmp.height, &b64)
            }
            TransmissionFormat::Png => {
                let png = Self::encode_png(bmp)?;
                let b64 = BASE64.encode(&png);
                Self::format_transmission_chunks(id, bmp.width, bmp.height, &b64)
            }
        };

        for seq in chunks {
            tty.write_seq(&seq).map_err(DisplayError::Io)?;
        }
        Ok(())
    }

    /// Escape sequences that present `id` for the given [`Placement`].
    ///
    /// A fixed placement id (`p=1`) is used so re-placing the same image with
    /// a new rect replaces the previous placement instead of accumulating
    /// copies on resize or mode switches.
    ///
    /// Kept pure so the flavor-dependent wire format can be asserted in tests.
    pub fn present_sequences(
        &self,
        id: ImageId,
        placement: Placement,
        links: &[TerminalHyperlink],
    ) -> Vec<String> {
        match placement {
            Placement::Cells(rect) | Placement::VirtualPlaceholder(rect) => {
                if self.placeholders_active() {
                    vec![
                        format!(
                            "\x1b_Ga=p,U=1,i={},p=1,c={},r={},z=-1,q=2\x1b\\",
                            id.0, rect.cols, rect.rows
                        ),
                        Self::generate_placeholder_string_with_links(id, rect, links),
                    ]
                } else {
                    // z=-1 keeps the image behind text so TUI overlays
                    // (help modal, drawers) composite above it. Cells in the
                    // slide area carry the default background, letting the
                    // image show through; opaque overlay cells mask it.
                    vec![format!(
                        "\x1b[{};{}H\x1b_Ga=p,i={},p=1,c={},r={},z=-1,q=2\x1b\\",
                        rect.row + 1,
                        rect.col + 1,
                        id.0,
                        rect.cols,
                        rect.rows
                    )]
                }
            }
            Placement::ScrollbackBlock => {
                vec![format!("\x1b_Ga=p,i={},q=2\x1b\\\n", id.0)]
            }
        }
    }

    fn present_impl(
        &mut self,
        id: ImageId,
        p: Placement,
        links: &[TerminalHyperlink],
    ) -> Result<(), DisplayError> {
        let tty = global_tty();
        for seq in self.present_sequences(id, p, links) {
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
            placeholders: self.placeholders_active(),
            terminal_side_scaling: true,
            resident_ids: 8,
        }
    }

    fn placement_mode(&self) -> PlacementMode {
        self.placement_mode
    }

    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        self.transmit(bmp, id)
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

    fn supports_placeholders(&self) -> bool {
        self.placeholders_active()
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
        self.transmit(bmp, id)
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
