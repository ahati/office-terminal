//! Pre-built transmission payloads for the Kitty graphics protocol.
//!
//! Raster workers encode a slide's PNG + base64 wire format off the UI
//! thread; the interactive path then hands the result to
//! [`TerminalGraphicsDriver::transmit_prebuilt`] so presenting a ready slide
//! costs exactly one buffered terminal write.

use office_terminal_core::{DisplayError, RgbaSlide, TransmitEncoder, TransmitPayload};

use crate::drivers::kitty::encode_transmit_payload;

/// [`TransmitEncoder`] implementation for the Kitty graphics protocol:
/// PNG (`f=100`) + base64, matching `KittyDriver::stage`.
///
/// Injected into the core render pipeline so background workers pre-encode
/// prefetch results; the UI thread only writes the prebuilt bytes.
pub struct KittyTransmitEncoder;

impl TransmitEncoder for KittyTransmitEncoder {
    fn encode(&self, bitmap: &RgbaSlide) -> Result<TransmitPayload, DisplayError> {
        encode_transmit_payload(bitmap)
    }
}
