use std::time::Duration;

use office_terminal_core::DisplayError;

use crate::driver::{PlacementMode, TerminalGraphicsDriver};
use crate::drivers::KittyDriver;

pub mod env;
pub mod geometry;
pub mod query;

pub struct CapabilityProbe;

impl CapabilityProbe {
    /// Verifies that the terminal implements the Kitty graphics protocol.
    ///
    /// There is intentionally no fallback: a terminal that does not answer the
    /// Kitty query is unsupported and startup must fail.
    pub fn probe() -> Result<(), DisplayError> {
        Self::probe_with_timeout(Duration::from_millis(50))
    }

    /// Probes with a custom timeout.
    ///
    /// `KITTY_PPTX_PROTOCOL=kitty` (or `kitty-full`) bypasses the probe for
    /// debugging. Any other value is an error.
    pub fn probe_with_timeout(timeout: Duration) -> Result<(), DisplayError> {
        if let Ok(raw) = std::env::var("KITTY_PPTX_PROTOCOL") {
            let value = raw.trim().to_ascii_lowercase();
            if value == "kitty" || value == "kitty-full" {
                return Ok(());
            }
            return Err(DisplayError::NoCapableTerminal);
        }

        if env::kitty_from_env() || query::query_kitty_graphics(timeout) {
            Ok(())
        } else {
            Err(DisplayError::NoCapableTerminal)
        }
    }

    /// Creates the Kitty driver after a successful probe.
    pub fn instantiate_driver(mode: PlacementMode) -> Box<dyn TerminalGraphicsDriver> {
        Box::new(KittyDriver::new(mode))
    }

    /// Probes once and returns the Kitty driver.
    pub fn probe_driver(
        mode: PlacementMode,
    ) -> Result<Box<dyn TerminalGraphicsDriver>, DisplayError> {
        Self::probe()?;
        Ok(Self::instantiate_driver(mode))
    }
}
