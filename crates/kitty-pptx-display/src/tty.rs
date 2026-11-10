use std::io::{self, Write};
use std::sync::Mutex;

pub struct TtyWriter {
    is_tmux: bool,
    stdout: Mutex<io::Stdout>,
}

impl TtyWriter {
    pub fn new() -> Self {
        let is_tmux = std::env::var("TMUX").is_ok();
        Self {
            is_tmux,
            stdout: Mutex::new(io::stdout()),
        }
    }
}

impl Default for TtyWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl TtyWriter {

    pub fn is_tmux(&self) -> bool {
        self.is_tmux
    }

    /// Atomically writes an escape sequence, applying tmux DCS pass-through if needed.
    pub fn write_seq(&self, seq: &str) -> io::Result<()> {
        let mut out = self.stdout.lock().unwrap();

        if self.is_tmux {
            // Tmux pass-through: wrap in DCS and double every ESC (\x1b -> \x1b\x1b)
            let mut escaped = String::with_capacity(seq.len() + 32);
            escaped.push_str("\x1bPtmux;");
            for b in seq.bytes() {
                if b == 0x1B {
                    escaped.push_str("\x1b\x1b");
                } else {
                    escaped.push(b as char);
                }
            }
            escaped.push_str("\x1b\\");
            out.write_all(escaped.as_bytes())?;
        } else {
            out.write_all(seq.as_bytes())?;
        }

        out.flush()
    }

    pub fn write_raw(&self, bytes: &[u8]) -> io::Result<()> {
        let mut out = self.stdout.lock().unwrap();
        out.write_all(bytes)?;
        out.flush()
    }
}

static GLOBAL_TTY: std::sync::OnceLock<TtyWriter> = std::sync::OnceLock::new();

pub fn global_tty() -> &'static TtyWriter {
    GLOBAL_TTY.get_or_init(TtyWriter::new)
}
