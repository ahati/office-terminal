use std::io::{self, IsTerminal, Write};
use std::time::Duration;

/// Sends the Kitty graphics query and returns whether the terminal answered.
///
/// The query is non-destructive (`a=q`) and uses a 1x1 RGBA probe. If either
/// stdin or stdout is not a TTY, the function returns `false` without writing.
pub fn query_kitty_graphics(timeout: Duration) -> bool {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return false;
    }

    let _ = crossterm::terminal::enable_raw_mode();
    let result = query_internal(timeout);
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

fn query_internal(timeout: Duration) -> bool {
    let mut stdout = io::stdout();
    let query = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
    if stdout.write_all(query).is_err() || stdout.flush().is_err() {
        return false;
    }

    let mut response = String::new();
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let remaining = timeout.saturating_sub(start.elapsed());
        if let Ok(true) = crossterm::event::poll(remaining) {
            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if let crossterm::event::KeyCode::Char(c) = key.code {
                    response.push(c);
                    if response.contains("_Gi=31;") {
                        return true;
                    }
                }
            }
        } else {
            break;
        }
    }
    response.contains("_Gi=31;")
}
