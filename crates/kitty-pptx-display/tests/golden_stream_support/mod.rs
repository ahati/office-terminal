//! Shared capture/decoding helpers for `golden_stream_tests.rs`.
//!
//! Kept in a submodule so the test file itself stays well below the 600-line
//! repository limit. Nothing here is compiled outside the test binary.

use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;

use kitty_pptx_core::RgbaSlide;
use kitty_pptx_display::*;

pub const CHILD_ENV: &str = "KITTY_PPTX_STREAM_CHILD";
pub const BEGIN: &str = "@@KITTY_STREAM_BEGIN@@";
pub const END: &str = "@@KITTY_STREAM_END@@";

pub const IMAGE_ID: ImageId = ImageId(0x1234);
pub const SLIDE_W: u32 = 8;
pub const SLIDE_H: u32 = 6;

/// Deterministic synthetic slide: four solid quadrants, one semi-transparent.
pub fn synthetic_slide() -> RgbaSlide {
    let mut pixels = Vec::with_capacity((SLIDE_W * SLIDE_H * 4) as usize);
    for y in 0..SLIDE_H {
        for x in 0..SLIDE_W {
            let color = match (x < SLIDE_W / 2, y < SLIDE_H / 2) {
                (true, true) => [255, 0, 0, 255],       // red, opaque
                (false, true) => [0, 200, 0, 255],      // green, opaque
                (true, false) => [0, 0, 255, 128],      // blue, half transparent
                (false, false) => [255, 255, 255, 255], // white, opaque
            };
            pixels.extend_from_slice(&color);
        }
    }
    RgbaSlide {
        width: SLIDE_W,
        height: SLIDE_H,
        pixels: Arc::from(pixels.into_boxed_slice()),
    }
}

fn alpha_blend(pixel: &[u8]) -> [u8; 3] {
    let alpha = pixel[3] as u16;
    [
        ((pixel[0] as u16 * alpha) / 255) as u8,
        ((pixel[1] as u16 * alpha) / 255) as u8,
        ((pixel[2] as u16 * alpha) / 255) as u8,
    ]
}

/// Runs the child writer for `kind` and returns the captured raw stream.
pub fn capture_stream(kind: &str) -> Vec<u8> {
    let exe = std::env::current_exe().expect("current test executable");
    let output = Command::new(exe)
        .args(["--exact", "golden_stream_child_writer", "--nocapture"])
        .env(CHILD_ENV, kind)
        .env_remove("TMUX")
        .env_remove("KITTY_WINDOW_ID")
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn stream child {kind}: {e}"));
    assert!(
        output.status.success(),
        "stream child {kind} failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = output.stdout;
    let begin = find_subslice(&stdout, BEGIN.as_bytes())
        .unwrap_or_else(|| panic!("stream child {kind} emitted no begin marker"));
    let after_begin = begin + BEGIN.len();
    let end = find_subslice(&stdout[after_begin..], END.as_bytes())
        .unwrap_or_else(|| panic!("stream child {kind} emitted no end marker"));
    stdout[after_begin..after_begin + end].to_vec()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Writes a marker directly to stdout (outside libtest's capture buffer).
pub fn write_marker(marker: &str) {
    let mut stdout = std::io::stdout();
    stdout.write_all(marker.as_bytes()).ok();
    stdout.flush().ok();
}

// ---------------------------------------------------------------------------
// ANSI / APC / OSC / DCS parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedCell {
    pub row: u16,
    pub col: u16,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    pub glyph: char,
}

fn parse_csi_params(raw: &str) -> Vec<u16> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(';')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .collect()
}

/// Decodes the half-block stream into cells with their active colours.
pub fn parse_halfblock_cells(stream: &str) -> Vec<DecodedCell> {
    let mut cells = Vec::new();
    let mut row: u16 = 0;
    let mut col: u16 = 0;
    let mut fg: Option<[u8; 3]> = None;
    let mut bg: Option<[u8; 3]> = None;

    let mut chars = stream.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            let Some(kind) = chars.next() else { break };
            if kind == '[' {
                let mut raw = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                    raw.push(c);
                }
                let params = parse_csi_params(&raw);
                match final_byte {
                    Some('H') if params.len() >= 2 => {
                        row = params[0].saturating_sub(1);
                        col = params[1].saturating_sub(1);
                    }
                    Some('m') => {
                        if params.first() == Some(&0) {
                            fg = None;
                            bg = None;
                        } else if params.len() >= 5 && params[0] == 38 && params[1] == 2 {
                            fg = Some([params[2] as u8, params[3] as u8, params[4] as u8]);
                        } else if params.len() >= 5 && params[0] == 48 && params[1] == 2 {
                            bg = Some([params[2] as u8, params[3] as u8, params[4] as u8]);
                        }
                    }
                    _ => {}
                }
            }
        } else if ch == '\u{2580}' {
            cells.push(DecodedCell {
                row,
                col,
                fg,
                bg,
                glyph: ch,
            });
            col = col.saturating_add(1);
        }
    }
    cells
}

/// Decodes the Kitty Unicode placeholder stream, including OSC 8 link state.
pub fn parse_placeholder_cells(stream: &str) -> Vec<(DecodedCell, Option<String>)> {
    let mut cells = Vec::new();
    let mut row: u16 = 0;
    let mut col: u16 = 0;
    let mut fg: Option<[u8; 3]> = None;
    let mut bg: Option<[u8; 3]> = None;
    let mut link: Option<String> = None;

    let mut chars = stream.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            if ch == '\u{10EEEE}' {
                cells.push((
                    DecodedCell {
                        row,
                        col,
                        fg,
                        bg,
                        glyph: ch,
                    },
                    link.clone(),
                ));
                col = col.saturating_add(1);
            }
            continue;
        }

        match chars.next() {
            Some('[') => {
                let mut raw = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                    raw.push(c);
                }
                let params = parse_csi_params(&raw);
                match final_byte {
                    Some('H') if params.len() >= 2 => {
                        row = params[0].saturating_sub(1);
                        col = params[1].saturating_sub(1);
                    }
                    Some('m') => {
                        if params.first() == Some(&0) {
                            fg = None;
                            bg = None;
                        } else if params.len() >= 5 && params[0] == 38 && params[1] == 2 {
                            fg = Some([params[2] as u8, params[3] as u8, params[4] as u8]);
                        } else if params.len() >= 5 && params[0] == 48 && params[1] == 2 {
                            bg = Some([params[2] as u8, params[3] as u8, params[4] as u8]);
                        }
                    }
                    _ => {}
                }
            }
            Some(']') => {
                // OSC ... terminated by ST (ESC \).
                let mut body = String::new();
                while let Some(c) = chars.next() {
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                    body.push(c);
                }
                if let Some(rest) = body.strip_prefix("8;") {
                    if rest.is_empty() || rest == ";" {
                        link = None;
                    } else {
                        // `id=<key>;<url>` or `;<url>`
                        let url = rest.split_once(';').map(|(_, u)| u).unwrap_or(rest);
                        link = Some(url.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    cells
}

#[derive(Clone, Debug)]
pub struct ApcChunk {
    pub control: HashMap<String, String>,
    pub payload: String,
}

impl ApcChunk {
    pub fn key(&self, key: &str) -> Option<&str> {
        self.control.get(key).map(String::as_str)
    }
}

pub fn parse_apc_chunks(stream: &str) -> Vec<ApcChunk> {
    let mut chunks = Vec::new();
    let mut rest = stream;
    while let Some(start) = rest.find("\x1b_G") {
        let body_start = start + 3;
        let Some(relative_end) = rest[body_start..].find("\x1b\\") else {
            break;
        };
        let body = &rest[body_start..body_start + relative_end];
        let (control, payload) = body.split_once(';').unwrap_or((body, ""));
        let mut map = HashMap::new();
        if !control.is_empty() {
            for pair in control.split(',') {
                if let Some((key, value)) = pair.split_once('=') {
                    map.insert(key.to_string(), value.to_string());
                }
            }
        }
        chunks.push(ApcChunk {
            control: map,
            payload: payload.to_string(),
        });
        rest = &rest[body_start + relative_end + 2..];
    }
    chunks
}

/// Expected half-block colour for a source pixel, including alpha blending.
pub fn expected_halfblock_color(x: u32, y: u32) -> [u8; 3] {
    let slide = synthetic_slide();
    let offset = ((y * SLIDE_W + x) * 4) as usize;
    alpha_blend(&slide.pixels[offset..offset + 4])
}
