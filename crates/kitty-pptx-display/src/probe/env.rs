#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetectedProtocol {
    Kitty,
    Sixel,
    Iterm2,
    HalfBlock,
}

/// Detects terminal protocol based purely on environment variables.
pub fn detect_protocol_from_env() -> Option<DetectedProtocol> {
    // 1. Kitty
    if std::env::var("KITTY_WINDOW_ID").is_ok() || std::env::var("KITTY_PID").is_ok() {
        return Some(DetectedProtocol::Kitty);
    }

    // 2. Ghostty
    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
        return Some(DetectedProtocol::Kitty);
    }

    // 3. WezTerm
    if std::env::var("WEZTERM_PANE").is_ok() {
        return Some(DetectedProtocol::Kitty);
    }

    // 4. Foot
    if std::env::var("FOOT_TERMINAL").is_ok() {
        return Some(DetectedProtocol::Sixel);
    }

    // 5. mintty
    if std::env::var("MINTTY_SHORTCUT").is_ok() {
        return Some(DetectedProtocol::Sixel);
    }

    // 6. Contour
    if std::env::var("CONTOUR_VERSION").is_ok() {
        return Some(DetectedProtocol::Kitty);
    }

    // 7. Check TERM_PROGRAM
    if let Ok(term_prog) = std::env::var("TERM_PROGRAM") {
        let prog = term_prog.to_ascii_lowercase();
        if prog == "ghostty" || prog == "wezterm" {
            return Some(DetectedProtocol::Kitty);
        }
        if prog == "iterm.app" {
            return Some(DetectedProtocol::Iterm2);
        }
        if prog == "alacritty" {
            return Some(DetectedProtocol::HalfBlock);
        }
        if prog == "mintty" {
            return Some(DetectedProtocol::Sixel);
        }
    }

    // 8. Check LC_TERMINAL
    if let Ok(lc_term) = std::env::var("LC_TERMINAL") {
        if lc_term.eq_ignore_ascii_case("iterm2") {
            return Some(DetectedProtocol::Iterm2);
        }
    }

    // 9. Check TERMINAL_EMULATOR
    if let Ok(emulator) = std::env::var("TERMINAL_EMULATOR") {
        if emulator.eq_ignore_ascii_case("contour") {
            return Some(DetectedProtocol::Kitty);
        }
    }

    // 10. Check Alacritty specific env vars
    if std::env::var("ALACRITTY_LOG").is_ok() || std::env::var("ALACRITTY_WINDOW_ID").is_ok() {
        return Some(DetectedProtocol::HalfBlock);
    }

    // 11. Check TERM variable
    if let Ok(term) = std::env::var("TERM") {
        let t = term.to_ascii_lowercase();
        if t.contains("kitty") {
            return Some(DetectedProtocol::Kitty);
        }
        if t.contains("foot") || t.contains("mlterm") || t.contains("sixel") {
            return Some(DetectedProtocol::Sixel);
        }
        if t.contains("alacritty") {
            return Some(DetectedProtocol::HalfBlock);
        }
    }

    None
}
