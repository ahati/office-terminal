use pptx_model::{MediaElement, Slide, SlideElement};
use std::process::{Command, Stdio};

/// Candidate external media player binaries supported by default.
pub const SUPPORTED_PLAYERS: &[&str] = &["mpv", "ffplay", "vlc"];

/// Find the first `MediaElement` on the given slide, if any.
pub fn find_slide_media(slide: &Slide) -> Option<&MediaElement> {
    slide.elements.iter().find_map(|el| match el {
        SlideElement::Media(m) => Some(m),
        _ => None,
    })
}

/// Returns true if the slide contains at least one `MediaElement`.
pub fn has_slide_media(slide: &Slide) -> bool {
    find_slide_media(slide).is_some()
}

/// Checks if a named binary is located in standard system `PATH` and executable.
pub fn is_executable_in_path(cmd: &str) -> bool {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(cmd);
            if candidate.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = candidate.metadata() {
                        if metadata.permissions().mode() & 0o111 != 0 {
                            return true;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Find the first available supported media player (`mpv`, `ffplay`, or `vlc`) in `PATH`.
pub fn find_available_player() -> Option<&'static str> {
    SUPPORTED_PLAYERS
        .iter()
        .copied()
        .find(|&player| is_executable_in_path(player))
}

/// Spawns an external player in a detached subprocess for the media file.
pub fn spawn_detached_player(player: &str, media_path: &str) -> std::io::Result<()> {
    Command::new(player)
        .arg(media_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// Attempt to play media present on the current slide.
///
/// Returns:
/// - `None` if the slide contains no media elements.
/// - `Some(status_message)` describing either the launched player or why playback could not begin.
pub fn try_play_current_slide_media(slide: &Slide) -> Option<String> {
    let media = find_slide_media(slide)?;
    let target = if !media.media_path.is_empty() {
        &media.media_path
    } else if !media.poster_path.is_empty() {
        &media.poster_path
    } else {
        return Some("Media element has no media path specified".to_string());
    };

    if let Some(player) = find_available_player() {
        match spawn_detached_player(player, target) {
            Ok(_) => Some(format!("Playing media with {}: {}", player, target)),
            Err(e) => Some(format!("Failed to launch {}: {}", player, e)),
        }
    } else {
        Some("No media player found (installed mpv, ffplay, or vlc required)".to_string())
    }
}

/// Attempt to play media with an explicit player command.
pub fn try_play_with_player(slide: &Slide, player: &str) -> Option<Result<String, String>> {
    let media = find_slide_media(slide)?;
    let target = if !media.media_path.is_empty() {
        &media.media_path
    } else if !media.poster_path.is_empty() {
        &media.poster_path
    } else {
        return Some(Err("Media element has no media path specified".to_string()));
    };

    match spawn_detached_player(player, target) {
        Ok(_) => Some(Ok(format!("Playing media with {}: {}", player, target))),
        Err(e) => Some(Err(format!("Failed to launch {}: {}", player, e))),
    }
}
