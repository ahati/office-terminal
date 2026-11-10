use kitty_pptx_core::DisplayError;
use crate::driver::*;
use crate::kitty::KittyDriver;

pub enum DetectedProtocol {
    Kitty,
    Sixel,
    Iterm2,
    HalfBlock,
}

pub struct CapabilityProbe;

impl CapabilityProbe {
    pub fn probe() -> Result<DetectedProtocol, DisplayError> {
        // 1. Check environment variables
        if std::env::var("KITTY_WINDOW_ID").is_ok() || std::env::var("KITTY_PID").is_ok() {
            return Ok(DetectedProtocol::Kitty);
        }

        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            if term_program.eq_ignore_ascii_case("ghostty") || term_program.eq_ignore_ascii_case("wezterm") {
                return Ok(DetectedProtocol::Kitty);
            }
            if term_program.eq_ignore_ascii_case("iterm.app") {
                return Ok(DetectedProtocol::Iterm2);
            }
        }

        // 2. Default to Kitty driver with half-block fallback capability
        Ok(DetectedProtocol::Kitty)
    }

    pub fn instantiate_driver(protocol: DetectedProtocol, mode: PlacementMode) -> Box<dyn TerminalGraphicsDriver> {
        match protocol {
            DetectedProtocol::Kitty => Box::new(KittyDriver::new(mode)),
            _ => Box::new(KittyDriver::new(mode)),
        }
    }
}
