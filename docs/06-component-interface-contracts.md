# PRD 06: Component Interface Contracts

## 1. Role & Precedence

This document is the **normative contract layer** of the architecture: it defines the types and traits that every component boundary passes through, plus the cross-cutting rules (error taxonomy, threading, ownership, serialization of terminal writes) that keep the layers independent. Where another PRD shows an interface sketch, this document's version is authoritative; discrepancies are bugs in the other document.

The single most important contract in the system:

> **The Composition Engine ([PRD 02a](./02a-composition-core-and-render-pipeline.md) et seq.) and the Display Adapter ([PRD 03](./03-terminal-graphics-protocol.md)) share exactly two types — `RgbaSlide` and `RenderTargetSpec` — and nothing else.** Composition never touches the terminal; Display never interprets slides. `export` (PRD 05) links the composition half *without any display code in the build*, which is the enforced proof of the seam.

## 2. Core Value Types

### 2.1 `RgbaSlide` — the frame contract

```rust
pub struct RgbaSlide {
    pub width: u32,          // > 0
    pub height: u32,         // > 0
    pub pixels: Arc<[u8]>,   // len == (width * height * 4) as usize
}
```

| Property | Rule |
| :--- | :--- |
| Layout | Row-major, tightly packed, `stride = width * 4`, no padding. |
| Alpha | **Straight (non-premultiplied) RGBA8** everywhere at this seam. |
| Sharing | `Arc`-backed: cloning is pointer-copy. Cache, driver staging, and export encode may hold the same frame concurrently. |
| Thread safety | `Send + Sync` (implied by contents). |
| Producer | Composition Engine only (PRD 02a §2.4). It converts from the rasterizer's premultiplied buffer in the canvas epilogue — one un-premultiply pass, done once per render. |
| Consumers | Any `TerminalGraphicsDriver`, the PNG encoder, the golden-image hasher (PRD 07). |

**Why straight alpha is normative**: PNG encoding (`f=100`) and raw transmission (`f=32`) both expect straight alpha; premultiplied input silently breaks transparency. Converting exactly once, at the production boundary, removes an entire class of driver bugs.

### 2.2 `RenderTargetSpec`

```rust
pub struct RenderTargetSpec { pub width_px: u32, pub height_px: u32 }
```

- Produced **only** by the Display Adapter's geometry negotiation (PRD 03 §4) or by the export pipeline from `--dpi` (PRD 05 §2.3).
- Opaque to composition: no DPI, no cell metrics, no terminal concepts.
- Cache-validity key component: pipeline caches are keyed `(slide_index, RenderTargetSpec)` (PRD 02a §5.1).

### 2.3 Geometry & Identity Types

```rust
pub struct ImageId(pub u32);                     // driver image-pool slot (1..=8)

pub struct CellRect { pub row: u16, pub col: u16, pub rows: u16, pub cols: u16 }

pub enum Placement {
    Cells(CellRect),        // interactive mode: absolute cell rect
    ScrollbackBlock,        // scrollback mode: emit at cursor via U=1 placeholders
}

pub enum PlacementMode { Interactive, Scrollback }

pub struct DriverCaps {
    pub alpha: bool,            // true: terminal composites RGBA
    pub placeholders: bool,     // true: scrollback persistence (Kitty protocol only, v1)
    pub terminal_side_scaling: bool, // true: c/r cell-area scaling (Kitty protocol)
    pub resident_ids: u32,      // >0: stage/present split supported; 0: present re-emits
}
```

## 3. Parser Facade

```rust
pub enum Source { File(PathBuf), Stdin, Bytes(Vec<u8>) }

pub trait PresentationLoader {
    fn load(&self, src: Source) -> Result<Presentation, ParseError>;
}
```

- Wraps the vendored `pptx-parser::parse_pptx_model_native` (native feature set, [PRD 01](./01-parser-and-data-model.md) §2).
- On success, returns the parsed `Presentation` **plus an open `AssetStore` view** over the same archive bytes (media parts are lazily read from the retained archive; PRD 01 §5).
- `ParseError` carries upstream's typed failure reasons (resource limit hit, corrupt part, unsupported content) — mapped into the taxonomy in §8.

## 4. Warning Event Stream

```rust
pub enum WarningEvent {
    FontSubstituted { requested: String, substitute: String },
    UnsupportedPreset { name: String },
    MediaFailed { rel: String, reason: AssetErrorKind },
    FidelityDegraded { element_id: String, feature: &'static str, class: FidelityClass },
    ResourceLimitHit { limit: &'static str },
}
```

- Emitted on a thread-safe `mpsc` sink created per session; consumed by the TUI status area (PRD 04 §6), `--debug-parser` stderr (PRD 01 §7), and `dump --json` output.
- Rule from [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §1.2: **every non-FULL emission fires a warning**. Silent degradation is a bug.

## 5. Asset Store

```rust
pub enum SizeHint { Native, AtMost { width: u32, height: u32 } }

pub struct DecodedImage { pub width: u32, pub height: u32, pub pixels: RgbaPixels /* straight RGBA8 */ }

pub trait AssetStore: Send + Sync {
    fn get(&self, rel: &str, hint: SizeHint) -> Result<DecodedImage, AssetError>;
    fn stats(&self) -> AssetStats; // entries, decoded cache bytes, hit ratio (diagnostics)
}
```

Behavior (normative):
1. **Lazy decode** on first `get`; decoded bitmaps cached in an LRU (default budget 64 MB, `[assets] cache_mb`).
2. `SizeHint::AtMost` downscales at decode time when the source is materially larger (≥ 1.5×) than needed — big stock photos must not evict the whole cache for a thumbnail pass.
3. SVG rasterizes via `resvg` at the hinted size; PowerPoint-bundled PNG fallbacks take precedence over the SVG itself.
4. EMF/WMF → `AssetError::UnsupportedFormat` (renderer draws placeholder per PRD 02d §2.2 — the store never fabricates stand-ins).
5. All failures are values, never panics; the caller decides placeholder vs. slide failure.

## 6. Display Contracts

### 6.1 `TerminalGraphicsDriver` (full behavior)

```rust
pub trait TerminalGraphicsDriver: Send {
    fn name(&self) -> &'static str;
    fn caps(&self) -> DriverCaps;
    fn placement_mode(&self) -> PlacementMode;
    fn stage(&mut self, bmp: &RgbaSlide, id: ImageId) -> Result<(), DisplayError>;
    fn present(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn reposition(&mut self, id: ImageId, p: Placement) -> Result<(), DisplayError>;
    fn release(&mut self, id: ImageId) -> Result<(), DisplayError>;
    fn release_all(&mut self) -> Result<(), DisplayError>;
}
```

| Rule | Rationale |
| :--- | :--- |
| `stage` is idempotent-replace: staging an existing `id` atomically replaces its frame. | Lets the pipeline re-use pool slots without ordering hazards. |
| `present`/`reposition` on an unstaged id ⇒ `DisplayError::NotStaged`. | Forces prefetch discipline; makes the < 16 ms flip target structurally achievable (PRD 03 §5.1). |
| `present` must be a single escape sequence (or equivalent) — O(1) in frame size. | Flip latency contract. |
| Every byte goes through the process-wide `TtyWriter` (§10). | No interleaved partial sequences from racing writers. |
| Calls never block beyond one chunk write (~4 KB); long transmissions are chunked by the pipeline's staging thread. | UI thread calls (`present`, `reposition`, `release`) stay sub-millisecond. |

### 6.2 `StageSink` — pipeline → display bridge

```rust
pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32; // driver's resident_ids
}
```

The pipeline (PRD 02a §5) holds a `StageSink` handle — **not** a driver reference — so export mode simply constructs no sink. This is the type-level guarantee that export builds without display code.

## 7. Pipeline Contracts

```rust
pub struct PrefetchPlan { pub center: usize, pub behind: usize, pub ahead: usize, pub stage: bool }

pub enum PipelineEntry {
    Ready { bitmap: RgbaSlide, staged: bool },
    Rendering,
    NotLoaded,
    Failed { error: RenderError },
}

pub enum InvalidateCause { Resize, TargetChanged, DeckReload }

pub trait RenderPipeline: Send + Sync {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec);
    fn entry(&self, index: usize) -> PipelineEntry;   // non-blocking snapshot
    fn invalidate(&self, cause: InvalidateCause);
}
```

Behavior (normative, details in PRD 02a §5): cache key `(slide, spec)`; eviction LRU-by-distance per spec; `stage: true` requires a `StageSink` at construction (absent ⇒ export mode, `staged` is always `false`); invalidation semantics per the table in PRD 02a §5.3 (`Resize` never re-rasterizes).

## 8. Error Taxonomy & Exit Projection

```rust
pub enum KittyPptxError {
    Io(std::io::Error),
    Config(ConfigError),            // bad flags, unreadable config
    Parse(ParseError),              // corrupt/unsupported pptx, resource limits
    Asset(AssetError),              // decode failures (per-element; usually degrades, not exits)
    Render(RenderError),            // composition failure of a slide
    Display(DisplayError),          // driver failures, no capable terminal
}
```

- **Panic policy**: panics are bugs. Untrusted input (decks) is handled through `Result` only; a panic on any corpus deck is a release blocker (PRD 07 §6).
- Layer errors convert upward losslessly; the CLI maps to exit codes ([PRD 05](./05-cli-and-export.md) §5):

| Exit code | Projected from |
| :--- | :--- |
| `0` | success |
| `1` | `Io`, `Config` |
| `2` | `Parse` |
| `3` | `Display` (incl. stdout-not-a-TTY for graphics subcommands) |
| `130` | SIGINT |

## 9. Threading & Ownership Model

| Component | Thread affinity | Synchronization |
| :--- | :--- | :--- |
| Parser facade | caller (CLI startup) | one-shot |
| AssetStore | any (worker pool) | interior `RwLock`/LRU mutex |
| Compositor + services (02b/02c/02d) | worker pool | none needed — stateless `Send + Sync` |
| Pipeline scheduler | owns worker pool | interior locks; public API `&self` |
| Driver | **single owner** (UI thread for interactive; staging thread writes only via `TtyWriter`) | `&mut self`; not `Sync` by design |
| TtyWriter | process-wide | internal mutex; atomic flush/discard |
| TUI | single event loop | communicates with pipeline via `Arc<dyn RenderPipeline>` |

## 10. `TtyWriter` Contract

One raw-mode writer wrapping stdout/stderr:

1. All escape-sequence emission (drivers, ratatui enter/leave, cleanup) funnels through it.
2. Writes are **atomic per sequence**: a sequence is buffered fully, then flushed in one `write`; a failed write never strands a partial sequence.
3. Flush discipline: `present`/`reposition` flush immediately (latency-critical); staging chunks flush opportunistically (throughput-oriented).
4. On drop or signal: flush-or-discard, then mode restoration (PRD 03 §9).

## 11. Versioning & Stability

- **Golden-image stability**: `RgbaSlide` bytes for a given (deck, spec, font set, crate versions) tuple are stable; PRD 07 pins all four in its fixtures.
- **JSON dump**: `dump --json` emits `{"schemaVersion": 1, …}`; breaking AST changes bump the schema version and are called out in the changelog (PRD 05 §2.4).
- **Vendored model drift**: upstream `pptx-model` changes that alter rendered output require a corpus re-baseline, gated per PRD 07 §4.4.
