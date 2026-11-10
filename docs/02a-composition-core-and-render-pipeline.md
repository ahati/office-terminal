# PRD 02a: Composition Core & Render Pipeline

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies the **slide compositor** (block B10) and the **render pipeline/scheduler** (block B11). Normative interface signatures are defined in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Scope

The Composition Core transforms parsed slide models, asset resources, and render target specifications into composited RGBA pixel buffers deterministically, off the UI thread, ensuring neighboring slides are ready before user navigation occurs.

| Block | Component | Responsibility |
| :--- | :--- | :--- |
| **B10** | **Slide Compositor** | Viewport aspect fitting, background rendering, element traversal, z-ordering, renderer dispatch, canvas epilogue. Emits `RgbaSlide`. |
| **B11** | **Render Pipeline / Scheduler** | Multithreaded worker coordination, cache budgeting, prefetch priority queue, cancellation, terminal staging orchestration. |
| **B12\*** | *(Producer seam)* | Constructs `RgbaSlide` and consumes `RenderTargetSpec` as an opaque contract. |

The Composition Core **never** accesses the terminal: no ioctls, no escape sequences, no terminal cell math. It accepts `RenderTargetSpec` and returns `RgbaSlide`. The Display Adapter ([PRD 03a](./03a-kitty-graphics-protocol-driver.md)) alone interacts with the terminal.

---

## 2. Coordinate System & Canvas Setup

### 2.1 Units & Math
PPTX coordinates are defined in English Metric Units (EMU):
- $1\text{ inch} = 914,400\text{ EMU}$
- $1\text{ point} = 12,700\text{ EMU}$
- $1\text{ pixel (at 96 DPI)} = 9,525\text{ EMU}$

A standard 16:9 widescreen presentation measures $9,144,000 \times 5,143,500\text{ EMU}$ ($10.0 \times 5.625\text{ inches}$, or $720 \times 405\text{ pt}$):

| Base Metric | Widescreen (16:9) | Standard (4:3) |
| :--- | :--- | :--- |
| Dimensions in EMU | $9,144,000 \times 5,143,500$ | $9,144,000 \times 6,858,000$ |
| Dimensions in Points | $720 \times 405\text{ pt}$ | $720 \times 540\text{ pt}$ |
| Logical Pixels @ 96 DPI | **$960 \times 540\text{ px}$** | **$960 \times 720\text{ px}$** |

### 2.2 Aspect Fitting & Viewport Mapping
When rendering a slide of dimension $(W_{\text{emu}}, H_{\text{emu}})$ into a target $(W_{\text{target}}, H_{\text{target}})$:
1. **Uniform Scale Factor**:
   $$s = \min\left(\frac{W_{\text{target}}}{W_{\text{emu}}},\; \frac{H_{\text{target}}}{H_{\text{emu}}}\right)$$
2. **Centered Offsets (Letterbox / Pillarbox)**:
   $$dx = \frac{W_{\text{target}} - (W_{\text{emu}} \cdot s)}{2.0},\qquad dy = \frac{H_{\text{target}} - (H_{\text{emu}} \cdot s)}{2.0}$$
3. **Default Interactive Target**: `1920×1080` ($2.0\times$ supersampling of the 96 DPI base). Export targets derive from `--dpi` ([PRD 05](./05-cli-and-export.md) §2.3).

### 2.3 Canvas Epilogue & Alpha Conversion
`tiny-skia` maintains an internal premultiplied RGBA8 buffer. The compositor converts this buffer into straight (non-premultiplied) RGBA8 at the seam:
```rust
fn finish_canvas(pixmap: tiny_skia::Pixmap, target: &RenderTargetSpec) -> RgbaSlide {
    let mut pixels = pixmap.take(); // Vec<u8>, len = width * height * 4
    // In-place un-premultiply: RGB = (RGB * 255 + Alpha/2) / Alpha
    unpremultiply_rgba8(&mut pixels);
    RgbaSlide {
        width: target.width_px,
        height: target.height_px,
        pixels: Arc::from(pixels.into_boxed_slice()),
    }
}
```
*Why straight alpha is mandatory*: PNG encoders (`f=100`) and terminal raw drivers (`f=32`) require straight alpha. Performing this conversion exactly once in the canvas epilogue eliminates color distortion and driver blending bugs.

---

## 3. Composition Traversal & Background Rendering

### 3.1 Background Layer Painting
Before traversing slide elements, the compositor paints the resolved slide background:
1. **Solid Fill**: Fills the slide rectangle with solid sRGB color.
2. **Gradient Fill**: Computes gradient angle and stop interpolations, rendering across the slide bounds.
3. **Blip Fill**: Scales or tiles the background image to fill the slide rectangle.
4. **Fallback**: If no background is defined in slide, layout, or master, the slide canvas is cleared to white (`#FFFFFF`) or the presentation default `lt1`.

### 3.2 Element Traversal & Transform Pipeline
The parser resolves element hierarchies into a flat, z-ordered `Vec<SlideElement>`. Group shapes are already flattened ([PRD 01](./01-parser-and-data-model.md) §4.5).
For each element, the compositor constructs the affine transform:
```rust
// 1. Convert EMU origin and extent to pixel coordinates
let x_px = dx + (element.x as f64 * s);
let y_px = dy + (element.y as f64 * s);
let w_px = element.width as f64 * s;
let h_px = element.height as f64 * s;

// 2. Center-pivot rotation and local mirroring
let cx = w_px / 2.0;
let cy = h_px / 2.0;
let mut transform = tiny_skia::Transform::identity()
    .post_translate(x_px, y_px)
    .post_translate(cx, cy)
    .post_rotate(element.rotation as f32)
    .post_translate(-cx, -cy);

if element.flip_h { transform = transform.post_scale(-1.0, 1.0); }
if element.flip_v { transform = transform.post_scale(1.0, -1.0); }
```
Element rendering executes in strict document order. Renderer failures emit a `WarningEvent` and paint the placeholder representation ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3) — a single element error **never** aborts the slide render.

---

## 4. Render Pipeline & Scheduler Architecture

```
┌────────────────────────────────────────────────────────┐
│ RenderPipeline Scheduler                               │
│ - Generation counter (cancellation tracking)           │
│ - Priority Prefetch Queue                              │
│ - Frame Cache (Byte-bounded LRU)                       │
└───────────────────────────┬────────────────────────────┘
                            │ dispatches tasks
       ┌────────────────────┼────────────────────┐
       ▼                    ▼                    ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ Worker 1     │     │ Worker 2     │     │ Worker N     │
│ SlideCompositor    SlideCompositor    SlideCompositor  │
└──────┬───────┘     └──────┬───────┘     └──────┬───────┘
       │                    │                    │
       └────────────────────┼────────────────────┘
                            ▼
             RgbaSlide (Straight RGBA8)
                            │
               [ If StageSink present ]
                            ▼
┌────────────────────────────────────────────────────────┐
│ Terminal Staging Thread (off UI thread)                │
│ - Encodes PNG / raw payload                            │
│ - Writes escape sequences via TtyWriter                │
└────────────────────────────────────────────────────────┘
```

### 4.1 Cache Hierarchy & Byte Budgeting
Pipeline frames are cached using the compound key `(slide_index: usize, target: RenderTargetSpec)`:
- Different specs (e.g. `1920×1080` presentation view vs `320×180` sorter thumbnail) coexist without mutual eviction.
- **Budgeting**: Default frame cache memory budget is 64 MB (configured via `[presenter] cache_mb`).
  - One 1920×1080 frame: $1920 \times 1080 \times 4 \approx 8.3\text{ MB}$.
  - A 5-frame window $[N-2 \dots N+2]$ occupies $\approx 41.5\text{ MB}$, well within budget.
  - In 4K mode (`3840×2160`), one frame is $\approx 33.2\text{ MB}$; the cache automatically scales its retention window down to 2 frames to honor the memory budget.

### 4.2 Priority Prefetch & Cooperative Cancellation
When slide $N$ becomes active, the pipeline schedules prefetch in strict order:
$$\text{Priority: } N \;\longrightarrow\; N+1 \;\longrightarrow\; N-1 \;\longrightarrow\; N+2 \;\longrightarrow\; N-2$$
1. **Generation Counter**: Any invalidation event (`TargetChanged`, `DeckReload`, or fast jump) increments an atomic `pipeline_generation: AtomicU64`.
2. **Cooperative Cancellation**: Worker tasks verify `task.generation == pipeline.current_generation()` before allocating resources. If stale, the task drops immediately without rendering.

### 4.3 Staging Lifecycle Coordination
When `plan.stage == true` (interactive presentation mode):
1. Upon rasterization completion, the worker passes `RgbaSlide` to the pipeline staging worker.
2. The staging worker claims an available `ImageId` from the 8-slot driver pool and calls `stage_sink.stage(id, &bitmap)`.
3. Staging sequence encoding (PNG compression) and transmission occur in idle time off the UI thread.
4. When the user initiates a flip, presentation is instantaneous ($O(1)$) because the image is already resident in terminal memory.

---

## 5. Invalidation Rules

| Cause | Trigger Event | Pipeline Action | Staging Action |
| :--- | :--- | :--- | :--- |
| **`Resize`** | Terminal window resized | **No re-render.** Cached frames remain valid. | Recompute `CellRect`; issue driver `reposition(id, new_cells)`. |
| **`TargetChanged`** | Scale mode altered (`ActualPixels`), DPI switch | Increment generation. Old cache keys evicted; prefetch window rasterized for new spec. | Existing staged images released (`release`); new frames staged. |
| **`DeckReload`** | Deck reloaded from disk | Clear all caches. Increment generation. Rebuild pipeline. | Driver issues `release_all()`; pool reset. |

---

## 6. Performance Guarantees

| Metric | Target (Reference 4-core Hardware) | Measurement Protocol |
| :--- | :--- | :--- |
| Simple slide render (title, bullets, logo) | $< 15\text{ ms}$ (p95) | [PRD 07](./07-quality-and-testing.md) §5 |
| Complex slide render (tables, shapes, images) | $< 40\text{ ms}$ (p95) | PRD 07 §5 |
| Staged slide flip latency (keypress $\to$ present) | $< 16\text{ ms}$ (p99) | PRD 07 §5 |
| Sorter thumbnail generation (20 slides, parallel) | $< 400\text{ ms}$ total | PRD 07 §5 |
