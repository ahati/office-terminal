/// Detects genuine Kitty from environment variables.
pub fn kitty_from_env() -> bool {
    kitty_from_env_with(|key| std::env::var(key).ok())
}

/// Environment-variable-free core of [`kitty_from_env`], injectable for tests.
///
/// Alacritty is deliberately absent: upstream has no graphics protocol while
/// forks may implement Kitty graphics, so the active query must decide.
pub fn kitty_from_env_with<F>(lookup: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    if lookup("KITTY_WINDOW_ID").is_some() || lookup("KITTY_PID").is_some() {
        return true;
    }

    if let Some(term_program) = lookup("TERM_PROGRAM") {
        if term_program.eq_ignore_ascii_case("kitty") {
            return true;
        }
    }

    if let Some(term) = lookup("TERM") {
        if term.to_ascii_lowercase().contains("kitty") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(pairs: &[(&str, &str)]) -> bool {
        kitty_from_env_with(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn test_kitty_env_is_detected() {
        assert!(detect(&[("KITTY_WINDOW_ID", "1")]));
        assert!(detect(&[("KITTY_PID", "42")]));
        assert!(detect(&[("TERM", "xterm-kitty")]));
        assert!(detect(&[("TERM_PROGRAM", "kitty")]));
    }

    #[test]
    fn test_alacritty_is_left_to_the_active_query() {
        // Upstream Alacritty has no graphics protocol, but forks can implement
        // Kitty graphics. Environment detection must not reject them.
        assert!(!detect(&[("TERM_PROGRAM", "Alacritty")]));
        assert!(!detect(&[("TERM", "alacritty")]));
        assert!(!detect(&[("ALACRITTY_LOG", "/tmp/alacritty.log")]));
        assert!(!detect(&[("ALACRITTY_WINDOW_ID", "1")]));
    }

    #[test]
    fn test_other_terminals_are_not_kitty_from_env() {
        assert!(!detect(&[("TERM_PROGRAM", "iTerm.app")]));
        assert!(!detect(&[("FOOT_TERMINAL", "1")]));
        assert!(!detect(&[("TERM", "xterm-256color")]));
        assert!(!detect(&[]));
    }
}
