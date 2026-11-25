//! Layered terminal pixel-geometry resolution.
//!
//! The raster targets (reader page classes, the canvas clamp) must follow
//! the terminal's real device pixels, but no single source is universally
//! available:
//!
//! 1. `OT_CELL=WxH` env override — explicit, wins over everything;
//! 2. `TIOCGWINSZ` pixel fields — correct on kitty/xterm/iTerm2/Alacritty
//!    (locally and over SSH: RFC 4254 §6.7 window-change carries pixel
//!    dims), but some terminals report `0`;
//! 3. `CSI 16 t` — reply `CSI 6 ; <width> ; <height> t`: the pixel size of
//!    a single cell (kitty, xterm; "supported in less terminals" per the
//!    kitty spec's own guidance for getting the window size);
//! 4. `CSI 14 t` — reply `CSI 4 ; <height> ; <width> t`: the text-area size
//!    in pixels, widely supported; divided by the ioctl's rows/cols it
//!    yields the cell size;
//! 5. the historical 10×20 default.
//!
//! The CSI round-trips need to own the input stream, so they run exactly
//! once, from [`prime_cell_metrics`], before the TUI starts reading events
//! (the same startup slot as the graphics-protocol probe). Resizes later
//! re-resolve through the ioctl; when the ioctl reports zero pixels the
//! primed cell size stays sticky — numerically what `CSI 14 t` would report
//! for the new grid, without touching stdin mid-session. A font-size-only
//! change on a zero-pixel terminal therefore needs the env override until
//! restart; that only shifts raster sharpness, never layout correctness.

use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use rustix::fs::OFlags;

use crate::geometry::{CellPixelSize, DEFAULT_CELL_ASPECT_RATIO};

/// How long to wait for each CSI reply before falling through to the next
/// layer. Replies are bytes on the tty; anything beyond this is a terminal
/// that ignores the query.
const CSI_TIMEOUT: Duration = Duration::from_millis(120);

/// Where a resolved cell size came from (diagnostics for `info`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellSizeSource {
    /// `OT_CELL=WxH` override (legacy: `KITTY_PPTX_CELL`).
    EnvOverride,
    /// `TIOCGWINSZ` pixel fields.
    Ioctl,
    /// `CSI 16 t` cell-size reply.
    CsiCell,
    /// `CSI 14 t` area reply divided by the ioctl grid.
    CsiArea,
    /// The built-in 10×20 assumption.
    Default,
}

impl CellSizeSource {
    /// Short human-readable name (status lines, `info`).
    pub fn label(self) -> &'static str {
        match self {
            CellSizeSource::EnvOverride => "env (OT_CELL)",
            CellSizeSource::Ioctl => "ioctl (TIOCGWINSZ pixels)",
            CellSizeSource::CsiCell => "csi (CSI 16 t cell size)",
            CellSizeSource::CsiArea => "csi (CSI 14 t area ÷ grid)",
            CellSizeSource::Default => "default (10×20 assumption)",
        }
    }
}

/// A cell size plus the layer it came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedCellSize {
    pub size: CellPixelSize,
    pub source: CellSizeSource,
}

/// Parses `OT_CELL` (`WxH`, e.g. `11x24`; legacy `KITTY_PPTX_CELL`).
pub fn parse_cell_env(raw: &str) -> Option<(f64, f64)> {
    let lowered = raw.trim().to_ascii_lowercase();
    let (w, h) = lowered.split_once('x')?;
    let (w, h): (f64, f64) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    plausible_cell(w, h).then_some((w, h))
}

/// `CSI 16 t` reply: `CSI 6 ; <width> ; <height> t`.
pub fn parse_csi_cell_reply(bytes: &[u8]) -> Option<(f64, f64)> {
    let dims = parse_csi_reply(bytes, b'6')?;
    let (w, h) = dims;
    plausible_cell(w, h).then_some((w, h))
}

/// `CSI 14 t` reply: `CSI 4 ; <height> ; <width> t` (height first).
pub fn parse_csi_area_reply(bytes: &[u8]) -> Option<(f64, f64)> {
    let (h, w) = parse_csi_reply(bytes, b'4')?;
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// Shared CSI reply shape: `ESC [ <code> ; <a> ; <b> t`.
fn parse_csi_reply(bytes: &[u8], code: u8) -> Option<(f64, f64)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let body = text.trim_start_matches("\x1b[").trim_end_matches('t');
    let mut parts = body.split(';');
    if parts.next()?.trim().as_bytes() != [code] {
        return None;
    }
    let a: f64 = parts.next()?.trim().parse().ok()?;
    let b: f64 = parts.next()?.trim().parse().ok()?;
    Some((a, b))
}

/// Cell sanity: real terminal cells are a few to a few dozen pixels and
/// taller than wide.
fn plausible_cell(w: f64, h: f64) -> bool {
    (2.0..=100.0).contains(&w) && (4.0..=200.0).contains(&h) && w < h
}

/// The resolution ladder as a pure function over the collected evidence.
fn resolve_layered(
    env: Option<(f64, f64)>,
    ioctl: Option<(f64, f64)>,
    csi_cell: Option<(f64, f64)>,
    csi_area: Option<(f64, f64)>,
    grid: Option<(u16, u16)>,
) -> ResolvedCellSize {
    let pick = |(w, h): (f64, f64), source: CellSizeSource| ResolvedCellSize {
        size: CellPixelSize {
            width_px: w,
            height_px: h,
        },
        source,
    };
    if let Some(cell) = env {
        return pick(cell, CellSizeSource::EnvOverride);
    }
    if let Some(cell) = ioctl.filter(|&(w, h)| w > 0.0 && h > 0.0) {
        return pick(cell, CellSizeSource::Ioctl);
    }
    if let Some(cell) = csi_cell {
        return pick(cell, CellSizeSource::CsiCell);
    }
    if let (Some((area_w, area_h)), Some((cols, rows))) = (csi_area, grid) {
        if cols > 0 && rows > 0 {
            let cell = (area_w / cols as f64, area_h / rows as f64);
            if plausible_cell(cell.0, cell.1) {
                return pick(cell, CellSizeSource::CsiArea);
            }
        }
    }
    pick((10.0, 10.0 / DEFAULT_CELL_ASPECT_RATIO), CellSizeSource::Default)
}

/// Result of the one-shot startup probe; sticky for the process.
static PRIMED: std::sync::OnceLock<ResolvedCellSize> = std::sync::OnceLock::new();

/// Runs the full pixel-geometry ladder once, before the TUI owns the input.
///
/// Order: env override → `TIOCGWINSZ` pixels → `CSI 16 t` → `CSI 14 t` ÷
/// grid → default. The CSI queries write to and read from the controlling
/// terminal on a private non-blocking `/dev/tty` descriptor, so they neither
/// contend with crossterm's stdin machinery nor disturb flags of the
/// standard streams. Idempotent: later calls return the first result.
pub fn prime_cell_metrics() -> ResolvedCellSize {
    *PRIMED.get_or_init(prime_internal)
}

fn prime_internal() -> ResolvedCellSize {
    let env = std::env::var("OT_CELL")
        .or_else(|_| std::env::var("KITTY_PPTX_CELL"))
        .ok()
        .and_then(|raw| parse_cell_env(&raw));
    let ioctl = ioctl_pixels();

    // The CSI layers only pay off when the ioctl cannot answer. They need
    // the terminal in raw/no-echo mode for the reply to become readable
    // (kitty's own guidance wraps the query in `stty raw -echo`); raw mode
    // is restored before returning, before the TUI sets up its own.
    let ioctl_answers = ioctl.is_some_and(|(w, h)| w > 0.0 && h > 0.0);
    let (csi_cell, csi_area) = if env.is_some() || ioctl_answers {
        (None, None)
    } else if crossterm::terminal::enable_raw_mode().is_ok() {
        let probed = (
            query_tty(b"\x1b[16t").and_then(|reply| parse_csi_cell_reply(&reply)),
            query_tty(b"\x1b[14t").and_then(|reply| parse_csi_area_reply(&reply)),
        );
        let _ = crossterm::terminal::disable_raw_mode();
        probed
    } else {
        (None, None)
    };

    resolve_layered(env, ioctl, csi_cell, csi_area, ioctl_grid())
}

/// Cell size derived from the winsize ioctl's pixel fields, zero pixels
/// included (the caller filters): window pixels ÷ grid.
fn ioctl_pixels() -> Option<(f64, f64)> {
    let size = crossterm::terminal::window_size().ok()?;
    if size.columns == 0 || size.rows == 0 {
        return None;
    }
    Some((
        size.width as f64 / size.columns as f64,
        size.height as f64 / size.rows as f64,
    ))
}

/// `(cols, rows)` from the winsize ioctl.
fn ioctl_grid() -> Option<(u16, u16)> {
    let size = crossterm::terminal::window_size().ok()?;
    Some((size.columns, size.rows))
}

/// Sends one CSI query to the controlling terminal and collects the reply
/// bytes up to the `t` terminator, bounded by [`CSI_TIMEOUT`].
///
/// Uses a private `/dev/tty` descriptor opened `O_NONBLOCK` (so a missing
/// reply can time out) and `poll(2)` to avoid busy-waiting. The descriptor
/// is closed on return; the standard streams are untouched.
fn query_tty(query: &[u8]) -> Option<Vec<u8>> {
    // `fd` is only used for reads; the pollfd borrows the File itself.
    use rustix::event::{poll, PollFd, PollFlags, Timespec};
    use rustix::io::read;

    let tty = open_tty_nonblocking()?;
    let fd = tty.as_fd();
    let _ = fd; // readability of the flow; reads use `fd` below

    let mut out = std::io::stdout();
    // The query itself goes to the standard output stream like every other
    // escape sequence this driver emits.
    out.write_all(query).ok()?;
    out.flush().ok()?;

    let mut reply = Vec::new();
    let mut chunk = [0u8; 64];
    let deadline = Instant::now() + CSI_TIMEOUT;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: remaining.subsec_nanos() as i64,
        };
        let mut fds = [PollFd::new(&tty, PollFlags::IN)];
        if poll(&mut fds, Some(&timeout)).unwrap_or(0) == 0 {
            break;
        }
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                reply.extend_from_slice(&chunk[..n]);
                // CSI t-replies terminate in `t`; stop as soon as one lands.
                if reply.ends_with(b"t") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if reply.is_empty() {
        None
    } else {
        Some(reply)
    }
}

/// Opens the controlling terminal read-write, non-blocking, without
/// acquiring it as a controlling tty.
fn open_tty_nonblocking() -> Option<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let flags = (OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC).bits() as i32;
    File::options()
        .read(true)
        .write(true)
        .custom_flags(flags)
        .open("/dev/tty")
        .ok()
}

/// Resolves the cell size for right now: env override, else a fresh ioctl,
/// else the sticky startup CSI result.
///
/// Used by [`crate::geometry::cell_pixel_size_cached`]; never talks to the
/// terminal (safe mid-session).
pub fn resolve_current() -> ResolvedCellSize {
    let env = std::env::var("OT_CELL")
        .or_else(|_| std::env::var("KITTY_PPTX_CELL"))
        .ok()
        .and_then(|raw| parse_cell_env(&raw));
    let ioctl = ioctl_pixels();
    let ioctl_answers = ioctl.is_some_and(|(w, h)| w > 0.0 && h > 0.0);
    if env.is_none() && !ioctl_answers {
        if let Some(primed) = PRIMED.get().copied() {
            return primed;
        }
    }
    resolve_layered(env, ioctl, None, None, ioctl_grid())
}

/// The sticky primed cell size, if [`prime_cell_metrics`] has run.
///
/// Used by the cached resolver on resizes when the ioctl reports zero
/// pixels: the cell rarely changes (font metrics), and the viewport is the
/// grid times this cell — exactly what `CSI 14 t` would report.
pub fn primed_cell_size() -> Option<ResolvedCellSize> {
    PRIMED.get().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_parses_wxh() {
        assert_eq!(parse_cell_env("11x24"), Some((11.0, 24.0)));
        assert_eq!(parse_cell_env(" 8 X 16 "), Some((8.0, 16.0)));
        assert_eq!(parse_cell_env("garbage"), None);
        assert_eq!(parse_cell_env("24x11"), None, "cells are taller than wide");
        assert_eq!(parse_cell_env("0x0"), None);
    }

    #[test]
    fn csi_replies_parse() {
        // CSI 16 t → CSI 6 ; width ; height t
        assert_eq!(parse_csi_cell_reply(b"\x1b[6;11;24t"), Some((11.0, 24.0)));
        // CSI 14 t → CSI 4 ; height ; width t (height first!)
        assert_eq!(parse_csi_area_reply(b"\x1b[4;1080;1920t"), Some((1920.0, 1080.0)));
        // Wrong code, garbage, or nonsense dims are rejected.
        assert_eq!(parse_csi_cell_reply(b"\x1b[4;1080;1920t"), None);
        assert_eq!(parse_csi_cell_reply(b"\x1b[6;0;0t"), None);
        assert_eq!(parse_csi_area_reply(b"not-a-reply"), None);
    }

    #[test]
    fn ladder_prefers_the_strongest_source() {
        let ioctl = Some((11.0, 24.0));
        let grid = Some((120u16, 40u16));

        // Env wins over everything.
        let r = resolve_layered(Some((9.0, 18.0)), ioctl, Some((11.0, 24.0)), None, grid);
        assert_eq!(r.source, CellSizeSource::EnvOverride);
        assert_eq!(r.size.width_px, 9.0);

        // Ioctl pixels beat CSI.
        let r = resolve_layered(None, ioctl, Some((11.0, 24.0)), None, grid);
        assert_eq!(r.source, CellSizeSource::Ioctl);

        // Zero-pixel ioctl falls through to the cell query.
        let r = resolve_layered(None, Some((0.0, 0.0)), Some((11.0, 24.0)), None, grid);
        assert_eq!(r.source, CellSizeSource::CsiCell);

        // Then to the area query divided by the grid.
        let area = Some((1320.0, 960.0)); // 120 cols x 40 rows of 11x24
        let r = resolve_layered(None, Some((0.0, 0.0)), None, area, Some((120, 40)));
        assert_eq!(r.source, CellSizeSource::CsiArea);
        assert_eq!(r.size.width_px, 11.0);
        assert_eq!(r.size.height_px, 24.0);

        // Nothing answered: the default.
        let r = resolve_layered(None, Some((0.0, 0.0)), None, None, grid);
        assert_eq!(r.source, CellSizeSource::Default);
        assert_eq!(r.size.width_px, 10.0);
        assert_eq!(r.size.height_px, 20.0);
    }

    #[test]
    fn a_degenerate_area_reply_falls_back_to_the_default() {
        // Area not divisible into plausible cells → default, not nonsense.
        let r = resolve_layered(None, Some((0.0, 0.0)), None, Some((3.0, 3.0)), Some((120, 40)));
        assert_eq!(r.source, CellSizeSource::Default);
    }
}
