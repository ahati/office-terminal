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
        let mut out = self.stdout.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        out.write_all(seq.as_bytes())?;
        out.flush()
    }

    /// Writes an entire batch of escape sequences as one buffered write.
    ///
    /// One lock acquisition, one `write_all` and one flush per batch (e.g. per
    /// transmitted image) instead of one syscall per 4 KiB chunk. Kitty parses
    /// the individual chunk framing from the byte stream, so coalescing is
    /// invisible to the protocol (PRD 03a §6).
    pub fn write_seq_batch(&self, seqs: &[&str]) -> io::Result<()> {
        let mut out = self
            .stdout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut buf = Vec::with_capacity(seqs.iter().map(|s| s.len()).sum());
        for seq in seqs {
            buf.extend_from_slice(seq.as_bytes());
        }
        out.write_all(&buf)?;
        out.flush()
    }

    pub fn write_raw(&self, bytes: &[u8]) -> io::Result<()> {
        let mut out = self.stdout.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
