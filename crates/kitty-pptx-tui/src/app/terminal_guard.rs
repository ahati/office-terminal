//! Terminal lifecycle guard.
//!
//! `PresentationApp::run` used to restore raw mode / alternate screen only on
//! its normal return path: a `?` error, a panic during `terminal.draw`, or
//! SIGTERM left the user's terminal unusable. The guard restores on `Drop`
//! (including unwind) and a signal thread restores before exiting.
//!
//! Restoration is idempotent so the normal path, a panic and a signal can all
//! call it without fighting each other.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor;
use crossterm::event;
use crossterm::execute;
use crossterm::terminal;

static RESTORED: AtomicBool = AtomicBool::new(false);

/// Restore the terminal and clear every Kitty image.
///
/// Called from `Drop`, the signal thread, and the normal path. `crossterm`
/// errors are deliberately ignored: cleanup must never mask the original
/// failure.
pub(crate) fn restore_terminal_once() {
    if RESTORED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut stdout = std::io::stdout();
    // Clear all placements/transmissions first, then leave the alternate
    // screen. The sequence is valid even if the terminal is not Kitty; it is
    // ignored there and keeps the cleanup path single.
    let _ = stdout.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\");
    let _ = stdout.flush();
    let _ = execute!(
        stdout,
        event::DisableMouseCapture,
        terminal::LeaveAlternateScreen,
        cursor::Show
    );
    let _ = terminal::disable_raw_mode();
}

/// RAII owner of the terminal modes entered for the interactive presenter.
pub(crate) struct TerminalGuard;

impl TerminalGuard {
    /// Enter raw mode, the alternate screen, mouse capture and hidden cursor.
    pub(crate) fn enter() -> std::io::Result<Self> {
        RESTORED.store(false, Ordering::SeqCst);
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(
            stdout,
            terminal::EnterAlternateScreen,
            event::EnableMouseCapture,
            cursor::Hide
        ) {
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }
        install_signal_restore();
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal_once();
    }
}

#[cfg(unix)]
fn install_signal_restore() {
    use std::sync::Once;
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let Ok(mut signals) = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ]) else {
            return;
        };
        std::thread::spawn(move || {
            if let Some(signal) = signals.forever().next() {
                restore_terminal_once();
                std::process::exit(128 + signal);
            }
        });
    });
}

#[cfg(not(unix))]
fn install_signal_restore() {}
