//! Pre-encoded terminal transmission payloads (see PRD 03a staging policy).
//!
//! Rasterization workers encode a page's terminal payload (e.g. PNG + base64
//! for the Kitty driver) immediately after rasterizing, so the interactive
//! syncer never pays encode cost on the UI thread.

use std::sync::Arc;

use crate::error::DisplayError;

use super::RgbaSlide;

/// A pre-encoded, driver-specific transmission payload for one raster.
#[derive(Clone, Debug)]
pub struct TransmitPayload {
    /// Base64-encoded payload bytes, ready for chunked transmission.
    pub base64: Arc<str>,
    /// Encoded byte length of the base64 payload (bytes on the wire).
    pub base64_len: usize,
}

/// Encodes [`RgbaSlide`] rasters into driver transmission payloads.
///
/// Implemented by the display layer (e.g. PNG + base64 for the Kitty
/// graphics protocol); injected into the render pipeline so background
/// workers can pre-encode prefetch results off the UI thread.
pub trait TransmitEncoder: Send + Sync {
    fn encode(&self, bitmap: &RgbaSlide) -> Result<TransmitPayload, DisplayError>;
}
