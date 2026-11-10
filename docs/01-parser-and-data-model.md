# PRD 01: Core Parser & Data Model Integration

## 1. Purpose & Scope
This document specifies the integration of the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer) into `kitty-pptx`. It covers how `.pptx` archives are opened, how PresentationML parts and inheritance trees are resolved, and how the internal data structures are managed in memory.

---

## 2. Upstream Parser Architecture & Integration

The upstream project structures its PPTX implementation into two core Rust crates:
- `packages/pptx/model` (`pptx-model`): Intermediate Abstract Syntax Tree (AST) representing presentations, slides, shapes, text, fills, and themes.
- `packages/pptx/parser` (`pptx-parser`): OPC (Open Packaging Conventions) archive reader and XML deserializer using `roxmltree` and `ooxml-common`.

```
┌──────────────────────────────────────────────────────────┐
│                   deck.pptx (ZIP File)                   │
└────────────────────────────┬─────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────┐
│             pptx-parser (Native Rust Mode)               │
│  - zip crate: stream uncompressed parts                  │
│  - roxmltree: fast, memory-safe DOM XML parsing          │
│  - ooxml-common: OPC relationship and namespace lookup    │
└────────────────────────────┬─────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────┐
│                 pptx-model::Presentation                 │
│  - dimensions: width, height (in EMU or points)          │
│  - theme: color schemes, font schemes, format schemes    │
│  - slides: Vec<Slide> (resolved shapes, text, images)    │
└──────────────────────────────────────────────────────────┘
```

### 2.1 Workspace Crate Vendoring
`kitty-pptx` consumes these upstream crates directly in a cargo workspace:
```toml
[dependencies]
pptx-model = { path = "./crates/pptx-model", default-features = false }
pptx-parser = { path = "./crates/pptx-parser", default-features = false }
```
*Note*: The `wasm-entry` feature in `pptx-parser` must be disabled so that WebAssembly bindings (`wasm-bindgen`) are excluded, compiling the parser as a lean native library.

---

## 3. The OOXML PresentationML Pipeline

A PowerPoint file is a ZIP archive containing interconnected XML parts. The parser resolves them in the following order:

### 3.1 Bootstrap Phase
1. **`[Content_Types].xml` & Root Relationships (`_rels/.rels`)**:
   Locates the primary presentation part, typically `ppt/presentation.xml`.
2. **`ppt/presentation.xml`**:
   - Reads presentation dimensions: `p:sldSz @cx`, `@cy` (default is 9144000x5143500 EMUs for 16:9 widescreen, or 10x7.5 inches for 4:3).
   - Reads slide identifiers and presentation ordering (`p:sldIdLst`).
3. **Themes & Master Inheritance (`ppt/slideMasters/`, `ppt/slideLayouts/`, `ppt/theme/`)**:
   - Builds the master theme palette (12 base color slots: `dk1`, `lt1`, `dk2`, `lt2`, `accent1`–`accent6`, `hlink`, `folHlink`).
   - Resolves layout placeholder hierarchies (Slide Layout inherits from Slide Master; Slide inherits from Slide Layout).

### 3.2 Slide Materialization (`ppt/slides/slideN.xml`)
For each slide in the presentation, the parser emits a `pptx_model::Slide` struct:
- **Slide Dimensions**: Inherited from presentation level or customized per slide.
- **Background**: Solid color, gradient, or background image (`p:bg`).
- **Elements (`Vec<SlideElement>`)**: Ordered Z-index list of visual objects.
- **Presenter Notes**: Text extracted from `ppt/notesSlides/notesSlideN.xml`.

---

## 4. In-Memory Data Model (`pptx-model`)

The parser produces the following high-level data types:

### 4.1 Slide Element Types (`SlideElement`)
```rust
pub enum SlideElement {
    Shape(ShapeElement),
    Picture(PictureElement),
    Table(TableElement),
    Chart(ChartElement),
    Media(MediaElement),
}
```

### 4.2 Shapes & Transforms (`ShapeElement`)
- **Transform (`Transform`)**:
  - `x`, `y`, `width`, `height` (in points / pixels).
  - `rotation` (in 60,000ths of a degree, converted to floating-point degrees).
  - `flip_h`, `flip_v` (boolean mirror flags).
- **Geometry**:
  - Preset geometries: `rect`, `roundRect`, `ellipse`, `triangle`, `arrowRight`, etc.
  - Custom geometries: Vector paths expressed as `Vec<PathCmd>` (`MoveTo`, `LineTo`, `QuadTo`, `CubicTo`, `Close`).
- **Styling**:
  - `fill`: `Solid`, `Gradient`, `BlipFill` (textured), or `None`.
  - `stroke`: Color, width, dash segments (`dash`, `dot`, `dashDot`), line joins, start/end arrowheads.

### 4.3 Typography & Text Bodies (`TextBody`)
- Paragraphs (`Vec<Paragraph>`):
  - Alignment: Left, Center, Right, Justified.
  - Line spacing and margin settings.
  - Bullet style: None, Character (e.g. `•`), Auto-numbered (e.g. `1.`, `a.`, `(i)`), or Graphic Blip.
- Text Runs (`Vec<TextRunData>`):
  - String content.
  - Font family (Latin, East Asian, Complex Script).
  - Font size (in points).
  - Weight and style: Bold, Italic, Underline, Strikethrough.
  - Text color and opacity.

### 4.4 Media & Pictures (`PictureElement`)
- Contains a relationship ID (`rId`) pointing to an asset inside `ppt/media/`.
- Crop boundaries (`srcRect`: left, top, right, bottom percentages).

---

## 5. Media & Asset Management

To prevent unnecessary memory inflation while ensuring smooth slide transitions, media assets are managed through a dedicated `AssetStore`:

```rust
pub struct AssetStore {
    archive_data: Vec<u8>,
    decoded_images: HashMap<String, DynamicImage>,
    max_cache_bytes: usize,
}
```

### Lifecycle Rules:
1. **Lazy Decoding**: Images inside `ppt/media/*` (PNG, JPEG, WebP) are extracted on-demand when the slide referencing them is first rendered.
2. **Decoded LRU Cache**: Decoded bitmap rasters (`DynamicImage`) are stored in an LRU cache with a default limit of 64 MB.
3. **SVG Fallback**: If an image is stored as an embedded `.svg` or `.emf`/`.wmf` vector:
   - Modern PPTX files bundle a PNG fallback for vector formats. The parser checks for fallback PNG relationships first.
   - For standalone SVGs, `resvg` is invoked to rasterize directly to pixels.

---

## 6. Resource Limits & Error Handling

OOXML files can be malformed, zip-bombed, or excessively complex. `kitty-pptx` inherits upstream's safety limits:

| Risk | Guard Mechanism | Default Limit |
| :--- | :--- | :--- |
| **Zip Bomb / Decompression Inflation** | Maximum uncompressed part size | 64 MB per XML part, 256 MB total |
| **Deep XML Nesting** | Max recursion depth in `roxmltree` | 256 levels |
| **DOM Complexity** | Total element node threshold | 200,000 nodes |
| **Corrupted Media** | Fail-soft placeholder | Render an error icon / placeholder box; do not crash the slide |
| **Unsupported Shapes** | Fallback to bounding box | Log warning and render placeholder stroke |

### Diagnostic Mode
When `--debug-parser` is passed via CLI:
- Dumps the resolved `Slide` AST as pretty-printed JSON.
- Prints relationship graph resolution logs and missing font warnings to `stderr`.
