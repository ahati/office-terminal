use std::time::Duration;
use kitty_pptx_core::DisplayError;
use crate::driver::*;
use crate::drivers::{KittyDriver, HalfBlockDriver, SixelDriver, Iterm2Driver};

pub mod env;
pub mod query;

pub use env::{detect_protocol_from_env, detect_protocol_with, DetectedProtocol};
pub use query::{query_terminal_device, parse_da1_has_sixel};

pub struct CapabilityProbe;

impl CapabilityProbe {
    /// Probes the current terminal environment and hardware capabilities.
    ///
    /// Priority:
    /// 1. Environment variables (KITTY_WINDOW_ID, TERM_PROGRAM, etc.)
    /// 2. Active terminal escape sequence query (Kitty query & DA1) with 50ms timeout.
    /// 3. Fallback to `DetectedProtocol::HalfBlock`.
    pub fn probe() -> Result<DetectedProtocol, DisplayError> {
        Self::probe_with_timeout(Duration::from_millis(50))
    }

    /// Probes environment variables only.
    pub fn probe_from_env() -> Option<DetectedProtocol> {
        detect_protocol_from_env()
    }

    /// Probes with a custom timeout for terminal queries.
    pub fn probe_with_timeout(timeout: Duration) -> Result<DetectedProtocol, DisplayError> {
        // 1. Check environment variables
        if let Some(proto) = detect_protocol_from_env() {
            return Ok(proto);
        }

        // 2. Query terminal device via escape sequences
        if let Some(proto) = query_terminal_device(timeout) {
            return Ok(proto);
        }

        // 3. Fallback to universal HalfBlock
        Ok(DetectedProtocol::HalfBlock)
    }

    /// Instantiates the appropriate graphics driver for the detected protocol.
    ///
    /// Unicode placeholders are only enabled for genuine kitty in interactive
    /// mode; partial protocol implementations use raw RGBA + direct placement.
    pub fn instantiate_driver(
        protocol: DetectedProtocol,
        mode: PlacementMode,
    ) -> Box<dyn TerminalGraphicsDriver> {
        match protocol {
            DetectedProtocol::Kitty => {
                let driver = KittyDriver::new_full(mode);
                if mode == PlacementMode::Interactive {
                    Box::new(driver.with_placeholder_mode(true))
                } else {
                    Box::new(driver)
                }
            }
            DetectedProtocol::KittyCompat => {
                Box::new(KittyDriver::new_compat(mode))
            }
            DetectedProtocol::Sixel => Box::new(SixelDriver::new(mode)),
            DetectedProtocol::Iterm2 => Box::new(Iterm2Driver::new(mode)),
            DetectedProtocol::HalfBlock => Box::new(HalfBlockDriver::new(mode)),
        }
    }
}
