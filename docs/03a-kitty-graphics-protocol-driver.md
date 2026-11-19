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
\x1b[<row+1>;<col+1>H\x1b_Ga=p,i=<id>,p=1,c=<cols>,r=<rows>,z=-1073741825,q=2\x1b\
```
- `c`/`r` scale the image to the fitted slide rectangle.
- `z` is `INT32_MIN/2 - 1` (`-1073741825`). Kitty draws `INT32_MIN/2 <= z < 0` images *between* cell backgrounds and text, so `z=-1` covers an overlay's opaque fill and leaves only its glyphs visible on Kitty and Ghostty. Values strictly below `INT32_MIN/2` are drawn under non-default cell backgrounds, keeping help modals, warning drawers, and the presenter UI opaque. The Alacritty fork collapses every negative `z` into that same behind-the-cell-pass layer, so one value behaves identically on Kitty, Ghostty and the fork.
- Repositioning uses the same sequence with a new cursor position and cell rectangle.
- Unicode placeholder cells (`U=1`, `U+10EEEE`) are not used.

---

## 5. Image Lifecycle & Staging Subsystem

```
Idle Time (Pipeline Worker)                 Flip Time (UI Thread)
───────────────────────────                 ───────────────────────
1. Rasterize RgbaSlide                      1. Keypress event (Space/Enter)
2. TransmitEncoder pre-encodes PNG+base64   2. transmit_prebuilt(id, payload)
   └─ cached as a TransmitPayload              └─ One batched write, no encode
3. staged_payload(index) hands it to UI     3. present(id=1, Cells(rect))
                                               └─ Emits a=p sequence
                                               └─ Flip latency < 16 ms (O(1))
```

### 5.1 The Partitioned Image ID Pool
The driver partitions the terminal's image namespace into disjoint ranges so that releasing one class of images can never delete another ([`driver.rs`](../crates/kitty-pptx-display/src/driver.rs)):

| Range | Class | Slots | Notes |
| :--- | :--- | :--- | :--- |
| `1..=8` (`CURRENT_IMAGE_ID_BASE`) | Full-screen current slide | 8 | Rotates with `slide_index % 8`. |
| `9..=16` (`PREVIEW_IMAGE_ID_BASE`) | Presenter next-slide preview | 8 | Separate range so releasing a preview cannot delete the incoming slide. |
| `17..=80` (`THUMBNAIL_IMAGE_ID_BASE`) | Slide-sorter thumbnail cards | 64 (`MAX_THUMBNAIL_IMAGES`) | Allocated from a free-list, stable while a card stays visible. |
| `81..=92` (`READER_IMAGE_ID_BASE`) | Document-reader page panes | 12 (`READER_IMAGE_ID_SLOTS`) | Disjoint range so reader releases can never delete slide/preview/thumbnail images; reader mode is exclusive with the other views. |

- **Re-transmission is destructive**: the protocol states that "when re-transmitting image data for a specific id, the existing image and all its placements must be deleted … the new data … is not actually displayed until a placement for it is created" ([kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol) §*Transmitting images*). Transmitting over a displayed id therefore **blanks** the placement for the whole transmit + decode window. Anything that re-stages a live slot goes through the *swap discipline* instead (`graphics.rs::stage_id`): fresh pixels go to a spare id of the same pool, the new bitmap is presented, and only then is the outgoing id released — so a slot is never empty on screen. `reposition(id, …)` remains flicker-free because it re-places the same (image id, placement id) pair without touching image data.
- **Resident capacity**: `DriverCaps::resident_ids` and `StageSink::resident_capacity()` report `RESIDENT_IMAGE_IDS` (92), covering all four ranges end-to-end.
- **Sorter uses a dedicated card target**: cards rasterize at an aspect-derived 1280-px-wide target (`graphics_thumbnails::thumbnail_target_for_aspect`), not the canvas raster: 1280 px keeps every realistic text run above the text rasterizer's 6 px font-size floor while costing ~5x less than a canvas raster, and it is resize/zoom invariant so cards survive layout changes. Cards are downscaled to ~480 px before transmission and the terminal scales them into the card cells.
- **Provisional frames**: a view that resolves a slide before its exact target is cached (the sorter's card raster on the way out of the grid, the pre-resize frame after a resize) shows that raster and later swaps in the target raster through the swap discipline above. `RenderPipeline::entry_at` remains the exact-target lookup for views that must not see a foreign target.
- **Memory bound**: a resident card costs one downscaled card (~0.5 MB) plus a cached 1280-px raster (~3.7–5.1 MB). The 64-slot cap keeps a full grid inside the documented terminal-memory budget.
- **Sorter lifecycle**: only visible cards are staged (at most 4 new transmissions per frame, to keep the event loop responsive); the visible window is prefetched at the sorter target, cards that scroll out of view are released immediately, and leaving the sorter releases every thumbnail — but only once the slide view has placed its own bitmap, so the teardown never leaves an empty screen.

### 5.2 Driver Operations

| Method | Escape Sequence Form | Latency Cost |
| :--- | :--- | :--- |
| `stage(bmp, id)` | Encodes PNG + base64 on the calling thread, then transmits `a=t,f=100,i=<id>,m=...` chunks | Idle time (fallback path) |
| `transmit_prebuilt(id, payload)` | One batched write of a worker-pre-encoded `TransmitPayload` (wire framing identical to `stage`) | Wire write, see below |

**PNG encode settings are a wire-throughput decision.** Payloads are encoded with `CompressionType::Default` + `FilterType::Adaptive`: the pty write is the transmission bottleneck (the terminal applies backpressure to `write_all`, stalling the UI thread for the whole drain), and the encode itself runs on the raster workers. `Fast` + `NoFilter` produced a near-raw PNG — ~34 MB for a 2560×3312 reader page, ~45 MB of base64 on the wire — so every page entering the viewport froze the reader's smoothed scroll at the boundary and left the pane blank until the placement landed. `Default` + `Adaptive` encodes the same page to ~0.05–2 MB (~250× less wire traffic) for ~35 ms more worker time; a page transmit now completes in single-digit milliseconds.
| `present(id, rect)` | Cursor position + `a=p,c=...,r=...,z=-1073741825` | $< 16\text{ ms}$ ($O(1)$) |
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
