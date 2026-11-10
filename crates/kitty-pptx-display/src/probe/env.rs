#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetectedProtocol {
    /// Genuine kitty: full graphics protocol incl. PNG payloads and Unicode placeholders.
    Kitty,
    /// Partial kitty-protocol implementations (WezTerm, Ghostty, ...). Raw RGBA
    /// transmission only; Unicode placeholders must not be used.
    KittyCompat,
    Sixel,
    Iterm2,
    HalfBlock,
}

impl DetectedProtocol {
    /// Whether this protocol supports the Kitty graphics escape sequences.
    pub fn is_kitty_family(self) -> bool {
        matches!(self, Self::Kitty | Self::KittyCompat)
    }
}

/// Detects terminal protocol based purely on environment variables.
pub fn detect_protocol_from_env() -> Option<DetectedProtocol> {
    detect_protocol_with(|key| std::env::var(key).ok())
}

/// Env-var-free core of [`detect_protocol_from_env`], injectable for tests.
pub fn detect_protocol_with<F>(lookup: F) -> Option<DetectedProtocol>
where
    F: Fn(&str) -> Option<String>,
{
    // 1. Genuine kitty (sets KITTY_WINDOW_ID / KITTY_PID unconditionally)
    if lookup("KITTY_WINDOW_ID").is_some() || lookup("KITTY_PID").is_some() {
        return Some(DetectedProtocol::Kitty);
    }

    // 2. Ghostty implements the graphics protocol but has no Unicode placeholders.
    if lookup("GHOSTTY_RESOURCES_DIR").is_some() {
        return Some(DetectedProtocol::KittyCompat);
    }

    // 3. WezTerm implements the graphics protocol, but not Unicode placeholders.
    if lookup("WEZTERM_PANE").is_some() {
        return Some(DetectedProtocol::KittyCompat);
    }

    // 4. Foot
    if lookup("FOOT_TERMINAL").is_some() {
        return Some(DetectedProtocol::Sixel);
    }

    // 5. mintty
    if lookup("MINTTY_SHORTCUT").is_some() {
        return Some(DetectedProtocol::Sixel);
    }

    // 6. Contour: APC parser but no Kitty graphics implementation; it supports sixel.
    if lookup("CONTOUR_VERSION").is_some() {
        return Some(DetectedProtocol::Sixel);
    }

    // 7. Check TERM_PROGRAM
    if let Some(term_prog) = lookup("TERM_PROGRAM") {
        let prog = term_prog.to_ascii_lowercase();
        if prog == "kitty" {
            return Some(DetectedProtocol::Kitty);
        }
        if prog == "ghostty" || prog == "wezterm" {
            return Some(DetectedProtocol::KittyCompat);
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
    if let Some(lc_term) = lookup("LC_TERMINAL") {
        if lc_term.eq_ignore_ascii_case("iterm2") {
            return Some(DetectedProtocol::Iterm2);
        }
    }

    // 9. Check TERMINAL_EMULATOR
    if let Some(emulator) = lookup("TERMINAL_EMULATOR") {
        if emulator.eq_ignore_ascii_case("contour") {
            return Some(DetectedProtocol::Sixel);
        }
    }

    // 10. Check Alacritty specific env vars
    if lookup("ALACRITTY_LOG").is_some() || lookup("ALACRITTY_WINDOW_ID").is_some() {
        return Some(DetectedProtocol::HalfBlock);
    }

    // 11. Check TERM variable
    if let Some(term) = lookup("TERM") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(pairs: &[(&str, &str)]) -> Option<DetectedProtocol> {
        detect_protocol_with(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn test_kitty_env_is_full_flavor() {
        assert_eq!(detect(&[("KITTY_WINDOW_ID", "1")]), Some(DetectedProtocol::Kitty));
        assert_eq!(detect(&[("KITTY_PID", "42")]), Some(DetectedProtocol::Kitty));
        assert_eq!(detect(&[("TERM", "xterm-kitty")]), Some(DetectedProtocol::Kitty));
        assert_eq!(detect(&[("TERM_PROGRAM", "kitty")]), Some(DetectedProtocol::Kitty));
    }

    #[test]
    fn test_wezterm_and_ghostty_are_compat_flavor() {
        assert_eq!(detect(&[("WEZTERM_PANE", "0")]), Some(DetectedProtocol::KittyCompat));
        assert_eq!(detect(&[("TERM_PROGRAM", "WezTerm")]), Some(DetectedProtocol::KittyCompat));
        assert_eq!(
            detect(&[("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")]),
            Some(DetectedProtocol::KittyCompat)
        );
        assert_eq!(detect(&[("TERM_PROGRAM", "ghostty")]), Some(DetectedProtocol::KittyCompat));
    }

    #[test]
    fn test_contour_is_sixel_not_kitty() {
        assert_eq!(detect(&[("CONTOUR_VERSION", "0.3")]), Some(DetectedProtocol::Sixel));
        assert_eq!(
            detect(&[("TERMINAL_EMULATOR", "contour")]),
            Some(DetectedProtocol::Sixel)
        );
    }

    #[test]
    fn test_other_terminals() {
        assert_eq!(detect(&[("TERM_PROGRAM", "iTerm.app")]), Some(DetectedProtocol::Iterm2));
        assert_eq!(detect(&[("LC_TERMINAL", "iTerm2")]), Some(DetectedProtocol::Iterm2));
        assert_eq!(detect(&[("FOOT_TERMINAL", "1")]), Some(DetectedProtocol::Sixel));
        assert_eq!(detect(&[("MINTTY_SHORTCUT", "1")]), Some(DetectedProtocol::Sixel));
        assert_eq!(
            detect(&[("ALACRITTY_WINDOW_ID", "1")]),
            Some(DetectedProtocol::HalfBlock)
        );
        assert_eq!(detect(&[]), None);
    }

    #[test]
    fn test_kitty_family_helper() {
        assert!(DetectedProtocol::Kitty.is_kitty_family());
        assert!(DetectedProtocol::KittyCompat.is_kitty_family());
        assert!(!DetectedProtocol::Sixel.is_kitty_family());
    }
}
