# PRD 06: Component Interface Contracts

## 1. Role & Precedence

This document is the **normative contract layer** of the architecture. It defines the core types, traits, error taxonomies, and cross-cutting rules that govern every component boundary.
Where another PRD shows an interface sketch, this document's version is authoritative.

> **The Two-Stage Pipeline & Seam Invariant**:
> 1. **Vector Scene Seam (`VectorSlide`)**: The layout compiler translates DrawingML slide models into a pure, resolution-independent 2D display list (`VectorSlide`). Both the `tiny-skia` rasterizer and the SVG vector exporter consume this single intermediate representation.
> 2. **Display Seam (`RgbaSlide`, `RenderTargetSpec`)**: The Composition Engine and the Display Adapter share **exactly two types — `RgbaSlide` and `RenderTargetSpec` — and nothing else.**
> Composition never touches the terminal; Display never interprets slide data or vector commands. `export` links without display driver code in the build ([PRD 05](./05-cli-and-export.md) §2.3).

---

## 2. Core Value Types

### 2.1 `VectorSlide` (Resolution-Independent Display List)
```rust
#[derive(Clone, Debug)]
pub struct VectorSlide {
    pub width_emu: i64,
    pub height_emu: i64,
    pub background: Option<VectorBackground>,
    pub commands: Vec<VectorCommand>,
}

#[derive(Clone, Debug)]
pub enum VectorCommand {
    DrawPath {
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
    },
    DrawText {
        runs: Vec<PositionedGlyphRun>,
        transform: AffineTransform,
    },
    DrawImage {
        rel_id: String,
        src_rect: Rect<f32>,     // Normalized [0, 1] crop rectangle
        dest_rect: Rect<f32>,    // EMU bounding box
        opacity: f32,
        transform: AffineTransform,
    },
    PushClip(VectorPath),
    PopClip,
}

#[derive(Clone, Debug)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub fill_rule: FillRule, // NonZero | EvenOdd
}

#[derive(Clone, Copy, Debug)]
pub enum PathSegment {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadTo { x1: f32, y1: f32, x: f32, y: f32 },
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    Close,
}

#[derive(Clone, Debug)]
pub struct PositionedGlyphRun {
    pub font_id: fontdb::ID,
    pub font_family: String,
    pub font_size_pt: f32,
    pub color: [u8; 4], // Straight RGBA8
    pub glyphs: Vec<PositionedGlyph>,
    pub underline: Option<UnderlineStyle>,
    pub strikethrough: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32, // EMU offset from run origin
    pub y: f32,
    pub advance: f32,
}
```

### 2.2 `RgbaSlide` (The Terminal Frame Contract)
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
- **Producer**: `tiny-skia` Rasterizer backend only ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §2.3).
- **Consumers**: Terminal graphics drivers, PNG export encoders, golden image testers.

### 2.3 `RenderTargetSpec`
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RenderTargetSpec {
    pub width_px: u32,
    pub height_px: u32,
}
```
- Produced by Display Adapter geometry negotiation or CLI export `--dpi`.
- Opaque to layout compilation: carries no terminal, cell, or DPI concepts.
- Serves as the raster cache key component: `(slide_index, RenderTargetSpec)`.

### 2.4 Geometry & Driver Primitives
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
    #[error("Slide layout compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Rasterization error: {0}")]
    RasterizationError(String),
    #[error("Vector serialization error: {0}")]
    SerializationError(String),
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

### 5.2 Slide Compiler & Pluggable Backends
```rust
pub trait SlideCompiler: Send + Sync {
    /// Compiles a DrawingML slide into a resolution-independent VectorSlide display list
    fn compile(
        &self,
        slide: &Slide,
        defaults: &PresentationDefaults,
        warning_sink: &crossbeam_channel::Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError>;
}

pub trait SlideRasterizer: Send + Sync {
    /// Rasterizes a VectorSlide display list to straight RGBA8 pixels at target resolution
    fn rasterize(
        &self,
        scene: &VectorSlide,
        target: &RenderTargetSpec,
        assets: &dyn AssetStore,
    ) -> Result<RgbaSlide, RenderError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgTextMode {
    SelectableText, // Emits SVG <text> elements
    VectorGlyphs,   // Emits exact SVG <path> glyph outlines for 100% font-independent fidelity
}

pub trait SlideSvgSerializer: Send + Sync {
    /// Serializes a VectorSlide display list into a clean standalone SVG XML string
    fn serialize_svg(
        &self,
        scene: &VectorSlide,
        assets: &dyn AssetStore,
        text_mode: SvgTextMode,
    ) -> Result<String, RenderError>;
}
```

### 5.3 Render Pipeline & Display Driver
```rust
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
3. **Signal-Safe Cleanup**: Flushes cleanup escape sequences on exit or SIGINT before restoring terminal canonical mode.
