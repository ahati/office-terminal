use std::io::{self, Write, IsTerminal};
use std::time::Duration;
use crate::probe::env::DetectedProtocol;

/// Queries the connected terminal device via escape sequences with a strict timeout.
///
/// Sends Kitty Graphics Protocol test sequence and DA1 (Primary Device Attributes).
/// If stdin is not a TTY or query times out, returns None without blocking.
pub fn query_terminal_device(timeout: Duration) -> Option<DetectedProtocol> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }

    // Attempt querying terminal in raw mode
    let _ = crossterm::terminal::enable_raw_mode();
    let result = query_internal(timeout);
    let _ = crossterm::terminal::disable_raw_mode();

    result
}

fn query_internal(timeout: Duration) -> Option<DetectedProtocol> {
    let mut stdout = io::stdout();

    // Query 1: Kitty graphics test query (1x1 pixel query, quiet mode disabled to get reply)
    // Query 2: DA1 (Primary Device Attributes: ESC [ c)
    let query_bytes = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";
    if stdout.write_all(query_bytes).is_err() || stdout.flush().is_err() {
        return None;
    }

    let mut response = String::new();
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        let remaining = timeout.saturating_sub(start.elapsed());
        if let Ok(true) = crossterm::event::poll(remaining) {
            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if let crossterm::event::KeyCode::Char(c) = key.code {
                    response.push(c);
                    // Check if we received Kitty response
                    if response.contains("_Gi=31;") {
                        return Some(DetectedProtocol::Kitty);
                    }
                    // Check if we received DA1 response containing 4 (Sixel capability)
                    if response.ends_with('c') && response.contains('?') && parse_da1_has_sixel(&response) {
                        return Some(DetectedProtocol::Sixel);
                    }
                }
            }
        } else {
            break;
        }
    }

    if response.contains("_Gi=31;") {
        Some(DetectedProtocol::Kitty)
    } else if parse_da1_has_sixel(&response) {
        Some(DetectedProtocol::Sixel)
    } else {
        None
    }
}

/// Checks whether a Primary Device Attributes (DA1) response indicates Sixel graphics support (feature code 4).
pub fn parse_da1_has_sixel(resp: &str) -> bool {
    // DA1 responses look like: \x1b[?62;1;4;6;7;8;9c
    if let Some(start) = resp.find('?') {
        let trimmed = resp[start + 1..].trim_end_matches('c');
        for part in trimmed.split(';') {
            if part == "4" {
                return true;
            }
        }
    }
    false
}
