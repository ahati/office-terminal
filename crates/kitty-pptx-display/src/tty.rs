use std::io::{self, Write};
use std::sync::Mutex;

pub struct TtyWriter {
    stdout: Mutex<io::Stdout>,
}

impl TtyWriter {
    pub fn new() -> Self {
        Self {
            stdout: Mutex::new(io::stdout()),
        }
    }

    /// Atomically writes an escape sequence.
    pub fn write_seq(&self, seq: &str) -> io::Result<()> {
        let mut out = self.stdout.lock().unwrap();
        out.write_all(seq.as_bytes())?;
        out.flush()
    }

    pub fn write_raw(&self, bytes: &[u8]) -> io::Result<()> {
        let mut out = self.stdout.lock().unwrap();
        out.write_all(bytes)?;
        out.flush()
    }
}

impl Default for TtyWriter {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_TTY: std::sync::OnceLock<TtyWriter> = std::sync::OnceLock::new();

pub fn global_tty() -> &'static TtyWriter {
    GLOBAL_TTY.get_or_init(TtyWriter::new)
}
