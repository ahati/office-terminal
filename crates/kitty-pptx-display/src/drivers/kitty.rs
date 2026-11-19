use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use kitty_pptx_core::{DisplayError, RgbaSlide, TransmitPayload};

use crate::driver::*;
use crate::tty::global_tty;

/// Z-index that draws a placement under the terminal's cell-background layer.
///
/// Kitty stacks a negative-z image below text but above cell backgrounds for
/// `INT32_MIN/2 <= z < 0`. `z=-1` therefore painted slides *over* the opaque
/// fill of TUI overlays on Kitty and Ghostty: the overlay text stayed visible
/// (glyphs sit above the image) but its background vanished. Values strictly
/// below `INT32_MIN/2` are drawn under non-default cell backgrounds.
///
/// The Alacritty fork does not implement that threshold — it draws every
/// negative z-index behind the combined cell pass — so this value keeps the
/// same overlay behaviour on Kitty, Ghostty and the fork.
pub const BEHIND_CELL_BACKGROUNDS_Z: i32 = i32::MIN / 2 - 1;

// Kitty's below-background band starts at INT32_MIN/2; a placement at or above
// it would cover overlay fills again.
const _: () = assert!(BEHIND_CELL_BACKGROUNDS_Z < i32::MIN / 2);

// ─── PNG + base64 transmission encoding ─────────────────────────────────────

/// Encodes a raster as PNG with the transmission settings used by
/// [`KittyDriver::stage`]: `CompressionType::Fast` + `FilterType::NoFilter`
/// favour encode latency over payload size — transmission is bound by pty
/// write throughput, not by terminal-side decompression cost.
fn encode_png(bmp: &RgbaSlide) -> Result<Vec<u8>, DisplayError> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;

    let mut png_bytes = Vec::new();
    let encoder = PngEncoder::new_with_quality(
        &mut png_bytes,
        CompressionType::Fast,
        FilterType::NoFilter,
    );

    encoder
        .write_image(
            &bmp.pixels,
            bmp.width,
            bmp.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| DisplayError::Io(std::io::Error::other(e.to_string())))?;

    Ok(png_bytes)
}

/// Encodes a raster into a pre-built Kitty transmission payload: PNG (`f=100`)
/// bytes wrapped in base64, exactly matching `KittyDriver::stage`.
///
/// Raster workers run this off the UI thread (via [`crate::KittyTransmitEncoder`])
/// and hand the result to
/// [`TerminalGraphicsDriver::transmit_prebuilt`], so the UI thread never pays
/// the PNG + base64 encode cost for a ready slide.
pub fn encode_transmit_payload(bmp: &RgbaSlide) -> Result<TransmitPayload, DisplayError> {
    let png = encode_png(bmp)?;
    let b64 = BASE64.encode(&png);
    Ok(TransmitPayload {
        base64: b64.as_str().into(),
        base64_len: b64.len(),
    })
}

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

    /// Transmits one bitmap as a single coalesced write (fallback path).
    ///
    /// Kitty requires the base64 payload to be split into ≤4 KB chunks with
    /// `m=0`/`m=1` framing, but the framing lives in the escape sequences, not
    /// in individual `write(2)` calls. Concatenating every chunk into one blob
    /// keeps the protocol framing intact while replacing dozens of
    /// write+flush syscalls per image with exactly one (PRD 03a §6: pty writes
    /// block on the terminal reader, so syscall count is latency).
    ///
    /// This path still pays the PNG + base64 encode inline; the prebuilt
    /// pipeline avoids it via [`TerminalGraphicsDriver::transmit_prebuilt`] +
    /// [`crate::KittyTransmitEncoder`].
    fn transmit(&self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError> {
        let payload = encode_transmit_payload(bmp)?;
        self.write_prebuilt(id, bmp.width, bmp.height, &payload.base64)
    }

    /// Joins Kitty's ≤4 KB `m=0`/`m=1` chunk framing for a pre-encoded base64
    /// payload into one coalesced blob and writes it with a single buffered
    /// write: one lock acquisition, one `write_all`, one flush per image.
    ///
    /// The framing is identical to [`Self::format_transmission_chunks`]; the
    /// base64 text is sliced as-is and never re-encoded.
    fn write_prebuilt(
        &self,
        id: ImageId,
        width: u32,
        height: u32,
        b64: &str,
    ) -> Result<(), DisplayError> {
        let tty = global_tty();
        let chunks = Self::format_transmission_chunks(id, width, height, b64);
        let mut blob = String::with_capacity(b64.len() + chunks.len() * 48);
        for seq in &chunks {
            blob.push_str(seq);
        }
        tty.write_seq_batch(&[&blob]).map_err(DisplayError::Io)
    }

    /// Escape sequences that present `id` for the given [`Placement`].
    ///
    /// A fixed placement id (`p=1`) replaces the previous placement instead of
    /// accumulating copies on resize or mode switches.
    pub fn present_sequences(id: ImageId, placement: Placement) -> Vec<String> {
        match placement {
            Placement::Cells(rect) => {
                // The image must sit below non-default cell backgrounds, not
                // just below text: on Kitty and Ghostty `z=-1` is above the
                // background layer, so an overlay's opaque fill disappears
                // behind the slide and only its glyphs remain visible. Cells
                // in the slide area carry the default background, letting the
                // image show through.
                vec![format!(
                    "\x1b[{};{}H\x1b_Ga=p,i={},p=1,c={},r={},z={},q=2\x1b\\",
                    rect.row + 1,
                    rect.col + 1,
                    id.0,
                    rect.cols,
                    rect.rows,
                    BEHIND_CELL_BACKGROUNDS_Z
                )]
            }
            Placement::CellsSubrect { rect, src } => {
                vec![format!(
                    "\x1b[{};{}H\x1b_Ga=p,i={},p=1,x={},y={},w={},h={},c={},r={},z={},C=1,q=2\x1b\\",
                    rect.row + 1,
                    rect.col + 1,
                    id.0,
                    src.x,
                    src.y,
                    src.width,
                    src.height,
                    rect.cols,
                    rect.rows,
                    BEHIND_CELL_BACKGROUNDS_Z
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

    fn transmit_prebuilt(
        &mut self,
        id: ImageId,
        width: u32,
        height: u32,
        payload: &TransmitPayload,
    ) -> Result<(), DisplayError> {
        self.write_prebuilt(id, width, height, &payload.base64)
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
