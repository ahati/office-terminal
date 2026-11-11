# PRD 03a: Kitty Graphics Protocol Driver

> Specifies the **Kitty Graphics Protocol** driver (block B15) and terminal image lifecycle (block B16). `kitty-pptx` is Kitty-only: there are no fallback drivers. Interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Role & Architectural Seam

The Kitty Graphics Driver bridges straight-alpha `RgbaSlide` frames into terminal graphics.
- **Seam Rule**: The driver consumes `RgbaSlide` and `Placement`. It has zero knowledge of slides, shapes, fonts, or tiny-skia.
- **TtyWriter Funnel**: All bytes pass through the process-wide `TtyWriter` ([PRD 06](./06-component-interface-contracts.md) §10). There is no multiplexer-specific wrapping.

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
| `f` | Format | `100` (PNG) |
| `i` | Image ID | Integer slot `1..=8` from the driver image pool |
| `s`, `v` | Source Dimensions | Pixel width (`s`) and height (`v`) |
| `c`, `r` | Cell Dimensions | Target columns (`c`) and rows (`r`) for direct placement |
| `m` | More Chunks | `1` (more chunks follow), `0` (final chunk) |
| `q` | Quietness | `1` (suppress OK responses), `2` (suppress all responses) |
| `z` | Z-Index | `-1` keeps the image behind TUI text and overlays |

---

## 3. Payload Encoding & Adaptive Slicing

### 3.1 Format Selection
- **PNG (`f=100`)**: Encoded once during idle staging off the UI thread. Payload size is typically $0.5\dots 1.8\text{ MB}$ for 1080p.

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

## 4. Direct Cell Placement (`a=p`)

The TUI reserves a rectangular cell region with blank cells and the driver presents the image there:
```
\x1b[<row+1>;<col+1>H\x1b_Ga=p,i=<id>,p=1,c=<cols>,r=<rows>,z=-1,q=2\x1b\
```
- `c`/`r` scale the image to the fitted slide rectangle.
- `z=-1` keeps the image behind text, so help modals, warning drawers, and the presenter UI composite above it.
- Repositioning uses the same sequence with a new cursor position and cell rectangle.
- Unicode placeholder cells (`U=1`, `U+10EEEE`) are not used.

---

## 5. Image Lifecycle & Staging Subsystem

```
Idle Time (Pipeline Worker)                 Flip Time (UI Keypress)
───────────────────────────                 ───────────────────────
1. Rasterize RgbaSlide                      1. Keypress event (Space/Enter)
2. stage(bmp, id=1)                         2. present(id=1, Cells(rect))
   └─ Transmits a=t,f=100 chunks               └─ Emits a=p sequence
   └─ Decoded into GPU memory                  └─ Flip latency < 16 ms (O(1))
```

### 5.1 The 8-Slot Image ID Pool
The driver maintains a pool of 8 numeric IDs (`1..=8`) for current-slide images. Presenter previews use a separate `9..=16` range so releasing a preview cannot delete the current slide's image.
- **Idempotent Replacement**: Staging a new frame with an existing active `id` atomically overwrites the slot in terminal memory without screen flicker.
- Maximum terminal memory residency is bounded to 8 slides ($\approx 12\text{ MB}$ GPU VRAM).

### 5.2 Driver Operations

| Method | Escape Sequence Form | Latency Cost |
| :--- | :--- | :--- |
| `stage(bmp, id)` | `\x1b_Ga=t,f=100,i=<id>,m=...;...\x1b\` | Idle time (amortized) |
| `present(id, rect)` | Cursor position + `a=p,c=...,r=...,z=-1` | $< 16\text{ ms}$ ($O(1)$) |
| `reposition(id, rect)` | Re-emit `a=p` at the new cell rect | $< 5\text{ ms}$ ($O(1)$) |
| `release(id)` | `\x1b_Ga=d,d=i,i=<id>\x1b\` | Instantaneous |
| `release_all()` | `\x1b_Ga=d,d=a\x1b\` | Single sequence |

> **Critical Fix**: Cleanup uses lowercase `d=a`. Uppercase `d=A` targets named image namespaces rather than deleting all images.

---

## 6. Capability Probing

There is no fallback renderer. Startup must fail with `DisplayError::NoCapableTerminal` unless:
1. `KITTY_WINDOW_ID`/`KITTY_PID`, `TERM_PROGRAM=kitty`, or a `TERM` containing `kitty` identifies the terminal, or
2. the active Kitty graphics query (`a=q`) receives a valid response.

`KITTY_PPTX_PROTOCOL=kitty` bypasses the probe for debugging. Any other override is an error.
