# PRD 03a: Kitty Graphics Protocol Driver

> Part of the PRD 03 split (03a/03b). Specifies the primary reference driver implementing the **Kitty Graphics Protocol** (block B15) and terminal image lifecycle (block B16). Fallback drivers and capability probing are specified in [PRD 03b](./03b-display-fallbacks-and-probing.md). Interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Role & Architectural Seam

The Kitty Graphics Driver bridges straight-alpha `RgbaSlide` frames into terminal graphics for Kitty, Ghostty, and WezTerm.
- **Seam Rule**: The driver consumes `RgbaSlide` and `Placement`. It has zero knowledge of slides, shapes, fonts, or tiny-skia.
- **TtyWriter Funnel**: All bytes pass through the process-wide `TtyWriter` ([PRD 06](./06-component-interface-contracts.md) §10).

---

## 2. Escape Sequence Syntax & Control Keys

The Kitty graphics escape sequence syntax:
```
\x1b_G<control_keys>;<payload>\x1b\
```
- Initiator: `\x1b_G` (`ESC _ G`)
- Control Keys: Comma-separated `key=value` pairs
- Separator: `;`
- Payload: Base64-encoded chunked data
- Terminator: `\x1b\` (`ESC \`, ST)

### 2.1 Control Keys Reference

| Key | Name | Values / Purpose |
| :--- | :--- | :--- |
| `a` | Action | `t` (transmit/stage only), `p` (display resident image), `d` (delete image) |
| `f` | Format | `100` (PNG, default), `32` (32-bit straight RGBA), `24` (24-bit RGB) |
| `i` | Image ID | Integer slot `1..=8` from driver image pool |
| `s`, `v` | Source Dimensions | Pixel width (`s`) and height (`v`) — mandatory for `f=32` |
| `c`, `r` | Cell Dimensions | Target columns (`c`) and rows (`r`) for terminal-side client scaling |
| `U` | Unicode Placeholder | `1` — place image via Unicode placeholder grid (`U+10EEEE`) |
| `m` | More Chunks | `1` (more chunks follow), `0` (final chunk) |
| `q` | Quietness | `1` (suppress OK responses), `2` (suppress all responses) |

---

## 3. Payload Encoding & Adaptive Slicing

### 3.1 Format Selection
- **PNG (`f=100`, Default)**: Encoded once during idle staging off the UI thread. Payload size is $\approx 0.5\dots 1.8\text{ MB}$ for 1080p, minimizing transmission overhead over SSH and local PTYs.
- **Raw RGBA (`f=32`, Optional)**: Zero CPU encode overhead, but transmits $\approx 8.3\text{ MB}$ uncompressed. Enabled via `--payload raw`.

### 3.2 Chunking Protocol (4096-Byte Slices)
Terminal buffers cap single escape sequence payloads. Base64 data is sliced into chunks of $\le 4096$ characters:
```
Chunk 1 (Header): \x1b_Ga=t,f=100,i=1,s=1920,v=1080,m=1;<4096 base64 chars>\x1b\
Chunk 2 (Cont.):   \x1b_Gm=1;<4096 base64 chars>\x1b\
...
Chunk N (Final):  \x1b_Gm=0;<remaining base64 chars>\x1b\
```
If the entire payload fits within 4096 characters, it is emitted in a single sequence with `m=0`.

---

## 4. Unicode Placeholder Grid (`U=1`)

For interactive presentation in Ratatui ([PRD 04](./04-interactive-tui-presenter.md)) and scrollback preservation ([PRD 05](./05-cli-and-export.md) §2.2), `kitty-pptx` uses the Kitty Unicode Placeholder protocol:

### 4.1 Staging with `U=1`
The frame is staged into terminal memory with the `U=1` flag:
```
\x1b_Ga=t,f=100,i=<id>,s=1920,v=1080,U=1,q=2,m=1;<chunk>\x1b\
```
The terminal allocates the image in GPU memory but displays nothing.

### 4.2 Display via Cell Grid Characters
To present the staged image over a cell area of $C$ columns by $R$ rows:
The driver/TUI writes the private-use Unicode character `\u{10EEEE}` into each cell of the rectangular area.
For each cell at 0-indexed column $c \in [0, C-1]$ and row $r \in [0, R-1]$:
- **Character**: `\u{10EEEE}` (Private Use Area 17).
- **Foreground Color**: Encodes the 24-bit Image ID:
  $$\text{FG RGB} = (\text{id} \pmod{256},\; (\text{id} \gg 8) \pmod{256},\; (\text{id} \gg 16) \pmod{256})$$
- **Background Color**: Encodes cell offsets $(c, r)$:
  $$\text{BG RGB} = (c \pmod{256},\; \lfloor c / 256 \rfloor,\; r \pmod{256})$$

### 4.3 Why `U=1` is Structurally Superior
1. **Ratatui Compatibility**: The slide canvas is a standard Ratatui widget writing placeholder characters into `ratatui::buffer::Buffer`.
2. **Zero-Glitch Overlays**: When a help modal (`?`), warning drawer (`W`), or comments drawer (`c`) opens, Ratatui's cell diffing simply overwrites placeholder cells with modal box-drawing characters and text. The GPU hardware image never clips through or corrupts the modal!
3. **Scrollback Persistence**: In one-shot `view` mode, placeholder characters flow with the text stream, preserving slides in terminal scrollback buffers.

---

## 5. Image Lifecycle & Staging Subsystem

```
Idle Time (Pipeline Worker)                 Flip Time (UI Keypress)
───────────────────────────                 ───────────────────────
1. Rasterize RgbaSlide                      1. Keypress event (Space/Enter)
2. stage(bmp, id=1)                         2. present(id=1, Cells(rect))
   └─ Transmits a=t,U=1 chunks                 └─ Emits 1 sequence / placeholder write
   └─ Decoded into GPU memory                  └─ Flip latency < 16 ms (O(1))
```

### 5.1 The 8-Slot Image ID Pool
The driver maintains a pool of 8 numeric IDs (`1..=8`):
- Managed via an LRU policy by the pipeline scheduler.
- **Idempotent Replacement**: Staging a new frame with an existing active `id` atomically overwrites the slot in terminal memory without screen flicker.
- Maximum terminal memory residency is bounded to 8 slides ($\approx 12\text{ MB}$ GPU VRAM).

### 5.2 Driver Operations

| Method | Escape Sequence Form | Latency Cost |
| :--- | :--- | :--- |
| `stage(bmp, id)` | `\x1b_Ga=t,f=100,i=<id>,U=1,m=...;...\x1b\` | Idle time (amortized) |
| `present(id, rect)` | Write placeholder grid $\mathbf{U+10EEEE}$ to buffer | $< 16\text{ ms}$ ($O(1)$) |
| `reposition(id, rect)` | Recompute cell grid bounds and update placeholder buffer | $< 5\text{ ms}$ ($O(1)$) |
| `release(id)` | `\x1b_Ga=d,d=i,i=<id>\x1b\` | Instantaneous |
| `release_all()` | `\x1b_Ga=d,d=a\x1b\` | Single sequence |

> **Critical Fix**: Cleanup uses lowercase `d=a`. Uppercase `d=A` targets named image namespaces rather than deleting all images.
