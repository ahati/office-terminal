# PRD 03b: Fallback Drivers & Capability Probing

> Part of the PRD 03 split (03a/03b). Specifies terminal capability probing (block B14), terminal geometry negotiation (block B13), multiplexer passthrough, and fallback graphics drivers: Sixel, iTerm2, and Unicode half-blocks (block B15). Primary Kitty driver is specified in [PRD 03a](./03a-kitty-graphics-protocol-driver.md). Interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Capability Probing & Detection State Machine

```
┌────────────────────────────────────────────────────────┐
│ Startup Capability Probe                               │
│ - Probe queries emitted simultaneously                 │
│ - 150 ms non-blocking response window                  │
└───────────────────────────┬────────────────────────────┘
                            │
       ┌────────────────────┼────────────────────┐
       ▼                    ▼                    ▼
Kitty Query             DA1 Query            Window/Cell
\x1b_Gi=1...;AAAA\x1b\  \x1b[c               \x1b[14t / \x1b[16t
       │                    │                    │
       ▼                    ▼                    ▼
Kitty Driver           Sixel Flag (q=4)     Pixel Resolution
```

### 1.1 Probe Sequence & Non-Blocking Read
At initialization, the probing subsystem configures raw terminal mode and emits three queries in a single write:
1. **Kitty Graphics Echo**: `\x1b_Gi=1,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\` (attempts transparent staging of a 1×1 pixel).
2. **Primary Device Attributes (DA1)**: `\x1b[c` (returns terminal feature flags; parameter `4` indicates Sixel).
3. **Cell/Window Pixel Dimensions**: `\x1b[16t` (requests cell pixels) and `\x1b[14t` (requests window pixels).

### 1.2 The 150 ms State Machine & Keystroke Preservation
The driver opens a non-blocking 150 ms event window using `poll()`:
- **Response Parsing**: Bytes matching terminal query responses are consumed and mapped to driver capabilities.
- **Keystroke Safety Buffer**: Any user keystrokes entered during this 150 ms startup window (e.g. presenter hitting Space/Enter immediately) are **not dropped**; they are pushed to an internal unconsumed input queue and replayed to the TUI event loop on startup.
- **Probe Timeout**: If the 150 ms timer expires without response, probe results fall back to environment variables (`KITTY_WINDOW_ID`, `TERM_PROGRAM`) or degrade to the Half-block driver.

---

## 2. Geometry Negotiation & Fitting

### 2.1 Window Size Discovery (`ioctl` vs Queries)
Window pixel dimensions are queried via `ioctl(TIOCGWINSZ)`:
```rust
#[repr(C)]
pub struct WinSize {
    pub ws_row: u16, pub ws_col: u16,
    pub ws_xpixel: u16, pub ws_ypixel: u16,
}
```
If `ws_xpixel > 0` and `ws_ypixel > 0`:
$$\text{cell\_w} = \frac{\text{ws\_xpixel}}{\text{ws\_col}},\qquad \text{cell\_h} = \frac{\text{ws\_ypixel}}{\text{ws\_row}}$$

If `ws_xpixel == 0` (virtual PTYs, tmux, SSH sessions):
1. The driver checks the ANSI `CSI 16 t` response (`\x1b[6;<h>;<w>t`).
2. If absent, fallback to default estimated cell metrics ($10 \times 20\text{ px}$, $2:1$ aspect ratio).

---

## 3. Terminal Multiplexer Support (`tmux`, `zellij`)

When running inside terminal multiplexers:
### 3.1 `tmux` DCS Pass-Through
`tmux` filters unhandled escape sequences. To pass graphics through to the underlying terminal:
1. Wrap the entire graphics escape sequence in a Device Control String (DCS) pass-through envelope:
   ```
   \x1bPtmux;\x1b<ESCAPED_GRAPHICS_COMMAND>\x1b\\
   ```
2. **Escape Doubling**: Every `\x1b` within the graphics payload must be doubled to `\x1b\x1b`.
3. **Chunk Size Guard**: `tmux` DCS input buffers can overflow on large sequences; chunk size is restricted to 2048 base64 bytes (instead of 4096) when `$TMUX` is detected.
4. If passthrough fails, a diagnostic warning advises setting `set -g allow-passthrough on` in `tmux.conf`.

### 3.2 `zellij`
Modern `zellij` releases pass through Kitty and Sixel graphics natively without DCS wrapping.

---

## 4. Fallback Drivers Specification

### 4.1 Sixel Graphics Driver
Supported on `foot`, `xterm` (`-ti 340`), `mlterm`, and Windows Terminal.
1. **Color Quantization**: Sixel limits palettes to 256 colors. The driver quantizes straight-alpha frames using NeuQuant / Median Cut, producing a 256-color palette and indexed pixel buffer.
2. **Alpha Compositing**: Sixel lacks hardware transparency. Straight alpha is pre-composited over black or the configured terminal background.
3. **Encoding & Run-Length Compression**:
   - Frames are sliced into 6-pixel horizontal bands (sixel lines).
   - Each vertical column of 6 pixels forms one character: $\text{char} = 63 + \sum_{k=0}^5 b_k \cdot 2^k$.
   - Repeated characters are compressed using `!<count><char>`.
   - Lines terminate with `$` (carriage return) and `-` (newline).
4. **Lifecycle**: Sixel lacks terminal-side image IDs. `stage` caches the quantized payload in memory; `present` emits the byte stream at flip time.

### 4.2 iTerm2 Inline Images Driver
Supported on iTerm.app and WezTerm:
```
\x1b]1337;File=inline=1;width=<cols>;height=<rows>;preserveAspectRatio=1:<base64_png>\x07
```
- Terminates with `BEL` (`0x07`).
- Image payload is always PNG encoded.
- No terminal-side image registry: `stage` stores PNG bytes in an in-memory cache; `present` emits the OSC sequence to the cursor position.

### 4.3 Unicode Half-Block Driver
Universal fallback on any 24-bit truecolor terminal emulator:
1. **Grid Resolution**: Each terminal character cell represents 2 vertical pixels using the `▀` character (U+2580, Upper Half Block).
2. **Color Mapping**:
   - Upper pixel color $\to$ ANSI 24-bit foreground: `\x1b[38;2;R;G;Bm`
   - Lower pixel color $\to$ ANSI 24-bit background: `\x1b[48;2;R;G;Bm`
3. **Box-Filter Downscale**: The $1920 \times 1080$ frame is downscaled via area averaging to $(cols) \times (rows \cdot 2)$ pixels before emitting text characters.

---

## 5. Driver Comparison & Capabilities

| Capability | Kitty Driver ([PRD 03a](./03a-kitty-graphics-protocol-driver.md)) | Sixel Driver | iTerm2 Driver | Half-Block Driver |
| :--- | :--- | :--- | :--- | :--- |
| **Max Colors** | Truecolor (24-bit) | 256 quantized | Truecolor (24-bit) | Truecolor (24-bit) |
| **Hardware Alpha** | Yes (straight RGBA) | No (pre-blended) | Yes | No (pre-blended) |
| **Terminal Image ID Pool** | Yes (8 resident IDs) | No | No | No |
| **Flip Latency** | $< 16\text{ ms}$ (pre-staged) | $\approx 40\dots 80\text{ ms}$ | $\approx 30\dots 60\text{ ms}$ | $\approx 20\dots 40\text{ ms}$ |
| **Ratatui Modals** | Flicker-free (`U=1`) | Overwritten by text | Cursor positioning | Standard text cells |
| **Scrollback Support** | Yes (Unicode placeholder) | Pages on scroll | Pages on scroll | Standard text flow |

---

## 6. Cleanup & Error Handling Contract

On normal exit, error abort, or signal intercept (`SIGINT`, `SIGTERM`, `SIGHUP`):
1. **Release All Images**: The Kitty driver emits `\x1b_Ga=d,d=a\x1b\`.
2. **Restore Modes**: Exit alternate screen buffer (`\x1b[?1049l`), enable cursor visibility (`\x1b[?25h`), and restore canonical terminal mode.
3. **Atomic Write Guarantee**: Escape sequences are flushed through `TtyWriter`; partial sequences are never left in the terminal buffer.
