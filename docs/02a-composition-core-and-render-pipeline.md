# PRD 02a: Composition Core & Render Pipeline

> This document is part of the PRD 02 split (02a/02b/02c/02d). It specifies the **composition orchestrator** and the **render pipeline/scheduler**. Normative interface signatures for every type and trait referenced here are defined in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Scope

The Composition Core answers one question: *given a parsed slide, its assets, and a render target, produce the final composited RGBA frame* — and do so deterministically, off the UI thread, with neighbors ready before the user needs them.

Components specified here:

| Block | Component | Responsibility |
| :--- | :--- | :--- |
| B10 | **Slide Compositor** | Element traversal, transform pipeline, z-order, dispatch to element renderers, canvas epilogue. Produces `RgbaSlide`. |
| B11 | **Render Pipeline / Scheduler** | Cache window, prefetch plans, worker pool, terminal staging coordination, invalidation, eviction. |
| B12* | *(producer side)* | Constructs `RgbaSlide` values and honors `RenderTargetSpec` as an opaque input. |

The Composition Core **never** touches the terminal: no ioctls, no escape sequences, no cell geometry. It receives a `RenderTargetSpec` and returns pixels. The Display Adapter ([PRD 03](./03-terminal-graphics-protocol.md)) is the only component that talks to the terminal.

Consumers of the pipeline:

- **TUI Presenter** ([PRD 04](./04-interactive-tui-presenter.md)) — interactive navigation, prefetch ahead of the user.
- **Export Pipeline** ([PRD 05](./05-cli-and-export.md)) — headless batch rendering, terminal staged display entirely absent.

## 2. Coordinate System & Canvas Setup

### 2.1 Units
PPTX geometry is in EMU ($1\text{ pt} = 12700\text{ EMU}$, $1\text{ in} = 914400\text{ EMU}$). The vendored parser emits element transforms in EMU (`x, y, width, height: i64`) and rotation in degrees (`f64`, converted by the parser from 60,000ths of a degree).

### 2.2 Slide Dimensions — Corrected Math
A standard 16:9 widescreen deck is $9144000 \times 5143500\text{ EMU}$:

| Unit | Width | Height |
| :--- | :--- | :--- |
| Inches | 10 | 5.625 |
| Points | 720 | 405 |
| Pixels @ 96 DPI | **960** | **540** |

*(The previous revision of this spec stated "1280×720 px at 96 DPI", which is incorrect.)*

### 2.3 EMU → Pixel Mapping
The compositor computes a single scale factor per render and applies it uniformly:

$$s = \frac{\text{target.width\_px}}{\text{slide\_width\_emu}},\qquad \text{px} = \text{emu} \cdot s$$

- Computed once per render as `f64`; sub-pixel accumulation is acceptable (differences are invisible at ≥ 2× supersampling of the 960×540 base).
- The **default interactive render target** is `1920×1080` (2× the 96-DPI base). Export targets derive from `--dpi` ([PRD 05](./05-cli-and-export.md) §2.3).

### 2.4 Canvas Epilogue
The compositor rasterizes into a `tiny-skia::Pixmap` (premultiplied RGBA internally) and converts to the contract frame type in a final pass:

```rust
fn finish(pixmap: Pixmap, target: &RenderTargetSpec) -> RgbaSlide {
    // Premultiplied → straight alpha (see PRD 06 §2.1: the seam is straight-alpha).
    // Arc'd into RgbaSlide so cache, drivers, and exporter share one buffer.
}
```

## 3. Element Traversal & Transform Pipeline

### 3.1 Input
The parser resolves inheritance (Master → Layout → Slide) and emits a flat, z-ordered `Vec<SlideElement>` per slide (`Shape | Picture | Table | Chart | Media`). **Groups are flattened by the parser**: nested `<p:grpSp>` elements are re-emitted with composed child transforms, under a fail-closed nesting-depth budget (upstream limit). The compositor therefore handles exactly one transform level — no nested group stacks.

### 3.2 Per-Element Transform
PowerPoint rotates around the shape's bounding-box center; flips are applied in the shape's local frame:

```rust
// 1. Position
let mut t = Transform::identity().post_translate(x_px, y_px);
// 2. Rotate about bbox center
let (cx, cy) = (w_px / 2.0, h_px / 2.0);
t = t.post_translate(cx, cy).post_rotate(rotation_deg)
       .post_translate(-cx, -cy);
// 3. Mirror in local frame
if flip_h { t = t.post_scale(-1.0, 1.0); }
if flip_v { t = t.post_scale(1.0, -1.0); }
```

Element-local geometry (paths in normalized $[0,1]$ coordinates) is scaled to the bbox in the element's local frame before `t` is applied.

### 3.3 Dispatch
Traversal is strictly in model order (document order = paint order). Each element dispatches to its renderer per [PRD 02d](./02d-element-renderers-and-fidelity-policy.md), using the geometry/style services of [PRD 02b](./02b-vector-style-and-geometry-engine.md) and the text engine of [PRD 02c](./02c-typography-and-text-layout.md). A renderer failure **never aborts the slide** — it is recorded as a `WarningEvent` and the element renders its placeholder per the fidelity policy.

## 4. Determinism & Purity

The compositor is a pure function of `(Slide, Presentation defaults, AssetStore view, RenderTargetSpec)`:

1. **No hidden state.** Same inputs → bit-identical `RgbaSlide` output (conditional on identical font set and crate versions — the property PRD 07 golden tests rely on).
2. **Thread safety.** `SlideCompositor: Send + Sync`; renderable concurrently from any number of workers.
3. **No I/O.** Asset access goes through the `AssetStore` handle; no file system or terminal access.

## 5. Render Pipeline / Scheduler

### 5.1 Cache Model
The pipeline caches composited frames keyed by **(slide index, RenderTargetSpec)**. Two distinct targets may coexist — e.g. full-resolution presentation frames (`1920×1080`) and sorter thumbnails (`320×180`, [PRD 04](./04-interactive-tui-presenter.md) §2.3) — without evicting each other beyond LRU pressure.

Eviction policy: LRU by distance from the active navigation point, per target-spec key space, bounded by a configurable memory budget (`[presenter] cache_window` config, default window `[N-2 … N+2]` for the full-res key space). Approximate cost model: each 1920×1080 frame = 8.3 MB; 5-frame window ≈ 41.5 MB; thumbnail space budgeted separately (≤ 8 MB).

### 5.2 Prefetch & Terminal Staging
Rendering a frame is necessary but not sufficient for an instant flip — the frame must also **already be resident in the terminal** (see [PRD 03](./03-terminal-graphics-protocol.md) §5). The pipeline therefore coordinates two asynchronous steps:

```rust
pub struct PrefetchPlan {
    pub center: usize,       // active slide index
    pub behind: usize,       // slides behind center to keep (default 2)
    pub ahead: usize,        // slides ahead to keep (default 2)
    pub stage: bool,         // also stage into the terminal via StageSink
}
```

1. **Rasterize** the window on the worker pool (priority: `N`, `N+1`, `N-1`, `N+2`, `N-2`; then ahead-biased look-ahead).
2. **Stage** completed frames into the terminal through the `StageSink` trait (PRD 06 §6) using an `ImageId` from the driver's pool. Staging runs off the UI thread; encode cost (PNG) is paid here, in idle time — never at flip time.
3. **Present** at flip time is then O(1): the TUI calls `driver.present(id, placement)`, which emits only a display action (PRD 03 §5.3).

Contract consequences:

- `present()` on an unstaged id returns `DisplayError::NotStaged` — the pipeline must guarantee staged residency for the window.
- The terminal holds at most K images (driver pool size, default 8); the scheduler releases ids evicted from the window.

### 5.3 Invalidation

| Cause | Effect |
| :--- | :--- |
| `Resize` | **No re-rasterization.** The terminal scales a staged image to its placement cells client-side (PRD 03 §4.2); only the placement rect is recomputed. Raster cache stays valid. |
| `TargetChanged` (e.g. `ActualPixels` mode, sorter thumbnails, DPI change) | New `RenderTargetSpec` → new cache key space; old keys evicted lazily; window re-rasterized and re-staged. |
| `DeckReload` | All caches and staged images released; pipeline rebuilt. |

*(The previous revision specified an 80 ms debounced re-raster on resize. This is removed by design: client-side scaling makes it redundant, and it re-opened the latency bug of paying transmission at interaction time.)*

### 5.4 Worker Model
- Worker pool sized `min(cpu_count - 1, 4)` by default (configurable `[render] workers`).
- One frame per worker at a time; intra-frame parallelism is **not** attempted in v1 (element lists are short; cross-frame parallelism is the better axis).
- Cancellation: a `TargetChanged` or `DeckReload` invalidation sets a generation counter; workers discard results from stale generations.

### 5.5 Pipeline Entries

```rust
pub enum PipelineEntry {
    Ready    { bitmap: RgbaSlide, staged: bool },
    Rendering,
    NotLoaded,
    Failed   { error: RenderError },
}
```

- `entry(i)` is non-blocking; it reports current state only.
- `Failed` retains the error for UI surfacing ([PRD 04](./04-interactive-tui-presenter.md) §6); the pipeline does **not** retry automatically (a deterministic failure would fail again); a `DeckReload` clears it.

## 6. Performance Budgets

| Scenario | Budget (99th percentile on reference hardware, measured per [PRD 07](./07-quality-and-testing.md) §5) |
| :--- | :--- |
| Simple slide (title + bullets + logo) | < 15 ms |
| Complex slide (10+ shapes, images, table) | < 40 ms |
| Flip latency, staged frame (keypress → present written) | < 16 ms |
| Sorter thumbnail (grid of 20, parallel) | < 400 ms total |

## 7. Consumer Contracts

- **TUI (PRD 04)**: calls `prefetch` on slide change and sorter open/close; reads `entry`; calls `present` via its driver handle. It does not schedule rendering itself.
- **Export (PRD 05)**: constructs the pipeline with `stage: false` and no driver; consumes `Ready` frames sequentially or in parallel; encodes to PNG on its own workers.
