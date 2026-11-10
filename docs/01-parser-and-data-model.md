# PRD 01: Core Parser & Data Model Integration

## 1. Purpose & Scope
This document specifies the integration of the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer) into `kitty-pptx`: how `.pptx` archives are opened (including non-seekable streams such as stdin), how PresentationML parts, relationships, and inheritance trees are resolved, what the verified internal data structures carry, and how media assets are managed. The access interfaces (`PresentationLoader`, `AssetStore`) are normative in [PRD 06](./06-component-interface-contracts.md) §3, §5.

---

## 2. Upstream Parser Architecture & Integration

The upstream project structures its PPTX implementation into two core Rust crates:
- `packages/pptx/model` (`pptx-model`): Intermediate AST representing presentations, slides, shapes, text, fills, and charts.
- `packages/pptx/parser` (`pptx-parser`): OPC (Open Packaging Conventions) archive reader and XML deserializer using `roxmltree`, `zip`, and `ooxml-common`.

```
┌──────────────────────────────────────────────────────────┐
│          deck.pptx (ZIP Archive / Stdin Stream)          │
└────────────────────────────┬─────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────┐
│             pptx-parser (Native Rust Mode)               │
│  - zip crate: stream uncompressed parts                  │
│  - roxmltree: fast, memory-safe DOM XML parsing          │
│  - ooxml-common: OPC relationship and namespace lookup   │
└────────────────────────────┬─────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────┐
│                 pptx-model::Presentation                 │
│  - slide_width/slide_height (EMU)                        │
│  - theme-baked defaults (colors, major/minor fonts)      │
│  - slides: Vec<Slide> (resolved shapes, text, images)    │
└────────────────────────────┬─────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────┐
│             kitty-pptx AssetStore Session                │
│  - Lazy decode of ppt/media/*                            │
│  - LRU-cached raster bitmaps & resvg SVGs                │
└──────────────────────────────────────────────────────────┘
```

### 2.1 Workspace Crate Vendoring
`kitty-pptx` consumes upstream crates directly in a cargo workspace:
```toml
[dependencies]
ooxml-common = { path = "./crates/ooxml-common", default-features = false }
pptx-model = { path = "./crates/pptx-model", default-features = false }
pptx-parser = { path = "./crates/pptx-parser", default-features = false }
```
- `pptx-parser` depends directly on `ooxml-common` (for OPC relationships, namespaces, and depth/resource guards) and `pptx-model`.
- The `wasm-entry` feature in `pptx-parser` gates `wasm-bindgen` exports; disabling it compiles the parser as a lean native library.
- Native entry points `parse_pptx_model_native` and `to_markdown_native` are unconditional and used by this project.

### 2.2 Upstream Synchronization Policy
1. **Pin**: Vendored copies record the upstream commit hash in `crates/VENDORED.md`.
2. **Sync**: Re-vendor deliberately; a sync PR executes the full golden corpus ([PRD 07](./07-quality-and-testing.md) §4). Model changes altering rendered output re-baseline within the same PR.
3. **Patch-carrying**: Local modifications are maintained as a numbered patch series in `crates/patches/` (never in-place edits) ensuring future upstream syncs remain mechanical.

---

## 3. Archive Ingestion & Non-Seekable Streams (`Source::Stdin`)

The OPC format is a ZIP archive requiring random access (`std::io::Seek`). Streaming input (`io::stdin`) cannot be read by `zip::ZipArchive` directly without spooling.

### 3.1 Spooling Strategy
When `Source::Stdin` is received by `PresentationLoader`:
1. **In-Memory Spooling (≤ 32 MB)**: Stream bytes into an allocated memory buffer (`Vec<u8>`). If EOF is encountered within 32 MB, instantiate `zip::ZipArchive` directly over a `std::io::Cursor<Vec<u8>>`.
2. **Temporary File Spillover (32 MB – 256 MB)**: If the stream exceeds 32 MB, write buffered bytes and remaining stream chunks to a secure temporary file created via `tempfile::NamedTempFile`.
3. **Hard Inflation Limit (256 MB)**: If the incoming stream exceeds 256 MB, abort immediately with `ParseError::ResourceLimitHit("stdin stream exceeded 256 MB")` to prevent disk exhaustion.
4. **Lifecycle**: The `NamedTempFile` handle is owned by the session's `AssetStore`. Dropping the `AssetStore` unlinks and cleans up the temporary file automatically.

---

## 4. The OOXML PresentationML Pipeline

### 4.1 Bootstrap Phase & OPC Relationships
1. **`[Content_Types].xml` & Root Relationships (`_rels/.rels`)**: Locates the primary presentation part (`ppt/presentation.xml`).
2. **Relationship URI Normalization**: OPC relationship targets can be relative (e.g. `../media/image1.png` referenced from `ppt/slides/_rels/slide1.xml.rels`). The loader normalizes all target paths against the referencing part directory to produce canonical internal archive keys (e.g. `ppt/media/image1.png`).
3. **Presentation Part (`ppt/presentation.xml`)**:
   - Slide dimensions (`p:sldSz @cx/@cy`; 16:9 widescreen default: 9,144,000 × 5,143,500 EMU, i.e. 10 × 5.625 inches — see [PRD 02a](./02a-composition-core-and-render-pipeline.md) §2.2).
   - Slide sequence and IDs (`p:sldIdLst`).

### 4.2 Themes & Background Inheritance
1. **Theme Palette Resolution**: The parser evaluates `ppt/theme/theme1.xml`, resolves DrawingML color slots (`dk1`, `lt1`, `dk2`, `lt2`, `accent1..=6`, `hlink`, `folHlink`), applies the ECMA-376 modifier chain (`lumMod`, `lumOff`, `tint`, `shade`, `satMod`, hue rotation), and **bakes the result into concrete sRGB hex strings**.
2. **Background Inheritance Hierarchy**:
   A slide's effective background resolves through a strict four-tier fallback chain:
   $$\text{Slide } (`p:sld/p:bg`) \;\longrightarrow\; \text{Layout } (`p:sldLayout/p:bg`) \;\longrightarrow\; \text{Master } (`p:sldMaster/p:bg`) \;\longrightarrow\; \text{Theme Default } (`lt1` \text{ or } \#FFFFFF)$$
   Background fills resolve to `Solid`, `Gradient`, `Blip` (image), or `None` (transparent).

### 4.3 Slide Materialization (`ppt/slides/slideN.xml`)
Each slide materializes with resolved dimensions, background fill, a z-ordered `Vec<SlideElement>`, and:
- `notes: Option<String>`: Extracted speaker notes (§4.4).
- `comments: Vec<PptxComment>`: Threaded reviewer comments (author, timestamp, text).
- `hidden: bool`: Slide visibility flag (`<p:sld show="0">`).
- `parse_error: Option<String>`: Partial degradation marker: malformed slide XML isolates to that slide while remaining slides load normally.

### 4.4 Speaker Notes Extraction & Filtering
`ppt/notesSlides/notesSlideN.xml` contains multiple shapes including header/footer placeholders and a slide thumbnail shape (`<p:ph type="sldImg"/>`).
- **Extraction Rule**: The loader ignores the `sldImg` shape, headers, footers, and slide number shapes.
- It extracts text runs exclusively from shapes with `<p:ph type="body"/>` or unassigned body text shapes, concatenating paragraphs with `\n` to produce a clean plain-text string.

### 4.5 Group Shape Flattening
Nested `<p:grpSp>` elements are flattened by the parser into child elements with composed affine transforms:
$$s_x = \frac{\text{ext}_x}{\text{chExt}_x},\qquad s_y = \frac{\text{ext}_y}{\text{chExt}_y}$$
$$x_{\text{child}}' = \text{off}_x + (x_{\text{child}} - \text{chOff}_x) \cdot s_x,\qquad y_{\text{child}}' = \text{off}_y + (y_{\text{child}} - \text{chOff}_y) \cdot s_y$$
$$w_{\text{child}}' = w_{\text{child}} \cdot s_x,\qquad h_{\text{child}}' = h_{\text{child}} \cdot s_y$$
$$\text{rot}_{\text{child}}' = (\text{rot}_{\text{group}} + \text{rot}_{\text{child}}) \pmod{360},\qquad \text{flip}' = \text{flip}_{\text{group}} \oplus \text{flip}_{\text{child}}$$
The resulting data model contains **zero group variants**; the compositor processes flat elements with single-level transforms ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §3.1).

---

## 5. In-Memory Data Model (`pptx-model`)

### 5.1 Slide Element Types
```rust
pub enum SlideElement {
    Shape(ShapeElement),
    Picture(PictureElement),
    Table(TableElement),
    Chart(ChartElement),
    Media(MediaElement),
}
```

### 5.2 Shapes & Geometry (`ShapeElement`)
- **Transform**: `x, y, width, height: i64` (EMU); `rotation: f64` (degrees); `flip_h, flip_v: bool`.
- **Geometry**:
  - `geometry: String`: Preset name (e.g. `rect`, `roundRect`) or `"custGeom"`.
  - `cust_geom: Vec<Vec<PathCmd>>`: Custom paths with normalized coordinates $[0.0, 1.0]$.
  - `cust_geom_paint: Option<Vec<PathPaint>>`: Per-subpath fill/stroke policy.
  - `adj: [Option<f64>; 8]`: Adjustment values (0–100,000) for parametric presets.
- **`PathCmd` Variants**:
  ```rust
  pub enum PathCmd {
      MoveTo { x: f32, y: f32 },
      LineTo { x: f32, y: f32 },
      CubicBezTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
      QuadBezTo { x1: f32, y1: f32, x: f32, y: f32 },
      ArcTo { wr: f32, hr: f32, st_ang: f32, sw_ang: f32 }, // angles in degrees
      Close,
  }
  ```
- **Fills & Strokes**:
  - `fill: Fill`: `Solid { color: String }`, `Gradient { stops: Vec<(f32, String)>, angle_deg: f32, kind: GradientKind }`, `None`.
  - `stroke: Stroke`: `color: String`, `width_emu: i64`, `dash_style: String`, `dash_segments: Option<Vec<f32>>`, `alignment: StrokeAlignment` (`Center | Inset`).

### 5.3 Typography & Text Bodies (`TextBody`)
- `vertical_anchor: VerticalAnchor` (`Top | Center | Bottom`).
- `wrap: WrapMode` (`None | Square`), insets `l_ins, r_ins, t_ins, b_ins` in EMU.
- `auto_fit: AutoFit` (`NoAutofit | NormAutofit { font_scale: f32, ln_spc_reduction: f32 } | SpAutoFit`).
- Paragraphs: `mar_l, mar_r, indent: i64` (EMU), `space_before, space_after`, `line_spacing`, `alignment`.
- Bullets: `None | Char { char: char, font: Option<String>, size: BulletSize, color: Option<String> } | AutoNum { format: AutoNumFormat, start_at: u32 } | Blip { rel_id: String }`.
- Runs (`TextRunData`): `text: String`, `font: Option<String>`, `size_pt: f32`, `bold: Option<bool>`, `italic: Option<bool>`, `underline: Option<UnderlineStyle>`, `strikethrough: bool`, `color: Option<String>`, `opacity: f32`.

### 5.4 Tables (`TableElement`)
- `grid_cols: Vec<i64>`: Column widths in EMU.
- `rows: Vec<TableRow>`: Row height in EMU, cells `Vec<TableCell>`.
- `TableCell`: `text_body: Option<TextBody>`, `fill: Option<Fill>`, `borders: CellBorders`, `grid_span: u32`, `row_span: u32`, `h_merge: bool`, `v_merge: bool`.

---

## 6. Media & Asset Management

The `AssetStore` trait and `DecodedImage` are normative in [PRD 06](./06-component-interface-contracts.md) §5:
1. **Lazy Decoding**: Media parts decode upon first reference using the `image` crate (PNG, JPEG, GIF first-frame, WebP, BMP, TIFF).
2. **LRU Cache**: Bounded decoded-image cache (default 64 MB, configurable via `[assets] cache_mb`).
3. **`SizeHint::AtMost`**: When the on-screen target is substantially smaller than the source image, decode with target-bounded downsampling to conserve cache memory.
4. **SVG & Metafiles**:
   - SVG decodes via `resvg` at the hinted target size. Bundled PNG fallback relationships take precedence over raw SVG.
   - EMF/WMF return `AssetError::UnsupportedFormat`; element renderers draw standard placeholders ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3).

---

## 7. Resource Limits & Diagnostics

| Risk | Guard Mechanism | Default Limit | Failure Behavior |
| :--- | :--- | :--- | :--- |
| Zip bomb / inflation | Max part uncompressed size | 64 MB / part, 256 MB total | `ParseError::ResourceLimitHit` |
| Deep XML nesting | Max element recursion | 256 levels | `ParseError::ResourceLimitHit` |
| Group nesting | Max group depth | 16 levels | Flattening stops, child dropped |
| DOM complexity | Total XML element count | 200,000 nodes | `ParseError::ResourceLimitHit` |
| Corrupt media | Fail-soft decode | — | `WarningEvent::MediaFailed` + placeholder |

All limit breaches surface as typed `ParseError` values ([PRD 06](./06-component-interface-contracts.md) §8) — **panics are prohibited**. Diagnostic details are emitted via the session `WarningEvent` stream and `--debug-parser` JSON output.
