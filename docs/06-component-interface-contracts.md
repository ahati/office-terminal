# PRD 06: Component Interface Contracts

## 1. Role & Precedence

This document is the **normative contract layer** of the architecture. It defines the core types, traits, error taxonomies, and cross-cutting rules that govern every component boundary.
Where another PRD shows an interface sketch, this document's version is authoritative.

> **The Architectural Seam Invariant**:
> The Composition Engine ([PRD 02a](./02a-composition-core-and-render-pipeline.md)) and the Display Adapter ([PRD 03a](./03a-kitty-graphics-protocol-driver.md), [PRD 03b](./03b-display-fallbacks-and-probing.md)) share **exactly two types — `RgbaSlide` and `RenderTargetSpec` — and nothing else.**
> Composition never touches the terminal; Display never interprets slide data. `export` links the composition half without display code in the build ([PRD 05](./05-cli-and-export.md) §2.3).

---

## 2. Core Value Types

### 2.1 `RgbaSlide` (The Frame Contract)
```rust
#[derive(Clone)]
pub struct RgbaSlide {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // len == (width * height * 4) as usize
}
```
- **Pixel Layout**: Row-major, tightly packed, `stride = width * 4`, zero padding.
- **Color Space**: **Straight (non-premultiplied) RGBA8** everywhere across this seam.
- **Sharing**: Backed by `Arc`: cloning is an $O(1)$ pointer copy.
- **Thread Safety**: Implements `Send + Sync`.
- **Producer**: Composition Engine only ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §2.3). Converts from premultiplied internal pixmap in the canvas epilogue.
- **Consumers**: Terminal graphics drivers, PNG export encoders, golden image testers.

### 2.2 `RenderTargetSpec`
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RenderTargetSpec {
    pub width_px: u32,
    pub height_px: u32,
}
```
- Produced by Display Adapter geometry negotiation or CLI export `--dpi`.
- Opaque to composition: carries no terminal, cell, or DPI concepts.
- Serves as the pipeline cache key component: `(slide_index, RenderTargetSpec)`.

### 2.3 Geometry & Driver Primitives
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32); // Valid driver pool range: 1..=8

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Clone, Debug)]
pub enum Placement {
    Cells(CellRect),
    ScrollbackBlock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacementMode { Interactive, Scrollback }

#[derive(Clone, Debug)]
pub struct DriverCaps {
    pub alpha: bool,
    pub placeholders: bool,
    pub terminal_side_scaling: bool,
    pub resident_ids: u32, // > 0: stage/present supported; 0: present re-emits
}
```

---

## 3. Concrete Error Taxonomy

```rust
#[derive(Debug, thiserror::Error)]
pub enum KittyPptxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("Asset error: {0}")]
    Asset(#[from] AssetError),
    #[error("Render error: {0}")]
    Render(#[from] RenderError),
    #[error("Display error: {0}")]
    Display(#[from] DisplayError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("Unreadable config file: {0}")]
    UnreadableConfig(String),
    #[error("Invalid slide range: {0}")]
    InvalidSlideRange(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Corrupted PPTX archive: {0}")]
    ArchiveError(String),
    #[error("Malformed XML part '{part}': {details}")]
    CorruptPart { part: String, details: String },
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitHit(&'static str),
    #[error("Unsupported encrypted/password-protected file")]
    PasswordProtected,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("Asset not found in archive: {0}")]
    NotFound(String),
    #[error("Unsupported media format: {0}")]
    UnsupportedFormat(String),
    #[error("Image decoding failed: {0}")]
    DecodeFailed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Slide composition failed: {0}")]
    CompositionFailed(String),
    #[error("Rasterization error: {0}")]
    RasterizationError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("Image {0:?} was not staged prior to presentation")]
    NotStaged(ImageId),
    #[error("Terminal I/O error: {0}")]
    Io(std::io::Error),
    #[error("No graphics-capable terminal found")]
    NoCapableTerminal,
    #[error("Graphics command requires a valid TTY")]
    TtyRequired,
    #[error("Terminal image memory quota exceeded")]
    QuotaExceeded,
}
```

---

## 4. Diagnostics & Warning Event Stream

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum WarningEvent {
    FontSubstituted { requested: String, substitute: String },
    UnsupportedPreset { name: String },
    MediaFailed { rel_id: String, reason: String },
    FidelityDegraded { element_id: String, feature: &'static str, class: FidelityClass },
    ResourceLimitHit { limit: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidelityClass {
    Full,
    Degraded,
    Placeholder,
    Omitted,
}
```
Rule: Every non-FULL element emission MUST emit a `WarningEvent` to the session-scoped MPSC sink.

---

## 5. Core Trait Contracts

### 5.1 Parser & Asset Loader
```rust
pub enum Source {
    File(PathBuf),
    Stdin,
    Bytes(Vec<u8>),
}

pub trait PresentationLoader: Send + Sync {
    fn load(&self, src: Source) -> Result<(Presentation, Arc<dyn AssetStore>), ParseError>;
}

pub enum SizeHint {
    Native,
    AtMost { width: u32, height: u32 },
}

pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // Straight RGBA8
}

pub trait AssetStore: Send + Sync {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError>;
    fn stats(&self) -> AssetStoreStats;
}
```

### 5.2 Slide Compositor & Pipeline
```rust
pub trait SlideCompositor: Send + Sync {
    fn render_slide(
        &self,
        slide: &Slide,
        presentation_defaults: &PresentationDefaults,
        assets: &dyn AssetStore,
        target: &RenderTargetSpec,
        warning_sink: &crossbeam_channel::Sender<WarningEvent>,
    ) -> Result<RgbaSlide, RenderError>;
}

pub struct PrefetchPlan {
    pub center: usize,
    pub behind: usize,
    pub ahead: usize,
    pub stage: bool,
}

pub enum PipelineEntry {
    Ready { bitmap: RgbaSlide, staged: bool },
    Rendering,
    NotLoaded,
    Failed { error: RenderError },
}

pub enum InvalidateCause { Resize, TargetChanged, DeckReload }

pub trait RenderPipeline: Send + Sync {
    fn prefetch(&self, plan: &PrefetchPlan, target: &RenderTargetSpec);
    fn entry(&self, index: usize) -> PipelineEntry;
    fn invalidate(&self, cause: InvalidateCause);
}
```

### 5.3 Display Adapter & Stage Sink
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

pub trait StageSink: Send + Sync {
    fn stage(&self, id: ImageId, bmp: &RgbaSlide) -> Result<(), DisplayError>;
    fn release(&self, id: ImageId) -> Result<(), DisplayError>;
    fn resident_capacity(&self) -> u32;
}
```

---

## 6. The `TtyWriter` Serialization Contract

All terminal writes (drivers, Ratatui layouts, signal handlers) funnel through a single `TtyWriter`:
1. **Thread Safety**: Protected by an internal non-reentrant mutex.
2. **Atomic Sequence Buffering**: Escape sequences are fully assembled in a local buffer before flushing in a single system `write()` call. No interleaved or broken sequences can ever be emitted.
3. **Multiplexer Encapsulation**: Automatically applies `tmux` DCS wrapping and escape doubling (`\x1b\x1b`) when `$TMUX` is active ([PRD 03b](./03b-display-fallbacks-and-probing.md) §3).
4. **Signal-Safe Cleanup**: Flushes cleanup escape sequences on exit or SIGINT before restoring terminal canonical mode.
