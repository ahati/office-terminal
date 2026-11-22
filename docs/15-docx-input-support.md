# PRD 15: DOCX Input Support — WordprocessingML Documents & Vector Layout Bridge

> **Status:** Normative Design Specification (ready for implementation).
> **Feature Track:** Read-only WordprocessingML (`.docx`, `.dotx`, `.docm`) input for `view`, `present`, `export`, `info`, and `dump`.
> **Depends on:**
> - [PRD 00: System Overview](./00-system-overview.md) (architecture, CLI binaries, and invariants)
> - [PRD 01: Parser & Data Model](./01-parser-and-data-model.md) (loader, container sniffing, asset store)
> - [PRD 02a: Composition Core & Pipeline](./02a-composition-core-and-render-pipeline.md) (two-tier caching, prefetch queue)
> - [PRD 02c: Typography & Text Layout](./02c-typography-and-text-layout.md) (font matching, glyph shaping, bundled fonts)
> - [PRD 02d: Element Compilers & Fidelity Policy](./02d-element-renderers-and-fidelity-policy.md) (honest degradation matrix)
> - [PRD 06: Component Interface Contracts](./06-component-interface-contracts.md) (`VectorSlide`, `PageSource`, `DeckSource`)
> - [PRD 07: Quality & Testing](./07-quality-and-testing.md) (test levels, DSSIM scoring, golden scenes)
> - [PRD 13: Metafile Pictures & Vector Playback](./13-metafile-pictures-and-vector-playback.md) (EMF/WMF playback)
> - [PRD 14: PDF Input Support](./14-pdf-input-support.md) (`PageSource` abstraction, multi-page document model)
> - [PRD 14a: PDF & Document Reader UI Designs](./14a-pdf-reader-ui-designs.md) (continuous scroll, dual-page reader mode)
> **Issue Reference:** [Issue #40](https://github.com/ahati/office-terminal/issues/40)

---

## 1. Executive Summary & Problem Statement

### 1.1 Context & Current Limitations
Office Terminal (`ot` / `office-terminal`) natively renders Microsoft PowerPoint (`.pptx`) presentations and Portable Document Format (`.pdf`) files directly to the terminal using the Kitty Graphics Protocol, alongside headless vector SVG and raster PNG exports.

Currently, any `.docx` file supplied to `ot` is rejected at the container boundary:
```
$ ot view report.docx
Error: Failed to load presentation: Missing required OOXML part: ppt/presentation.xml
```
Because `sniff_container_format` in `crates/office-terminal-core/src/loader/presentation_loader.rs` checks only for the ZIP local-file header (`[0x50, 0x4B, 0x03, 0x04]`), every OOXML package is presumed to be a PresentationML deck (`SourceFormat::Pptx`). The parser then fails when presentation parts are missing.

### 1.2 The Document Rendering Challenge: Parsing vs. Layout
Unlike PPTX (where shapes and text frames have explicit coordinates stored in the markup) and PDF (where glyphs and graphics have pre-calculated coordinates), WordprocessingML (`.docx`) is a **flow document**:
- Document content flows across paragraphs, runs, inline shapes, and tables without explicit page coordinates.
- **Pagination, line breaking, justification, hyphenation, table layout, and floating object positioning must be computed dynamically** by a layout engine following the complex pagination rules of ECMA-376 / ISO/IEC 29500.
- Parsing the OPC package and XML DOM is straightforward; **layout and pagination is the critical engineering challenge**.

### 1.3 Strategic Solution
PRD 15 establishes DOCX as the third first-class document format in Office Terminal:
1. **Container Sniffing**: Inspect `[Content_Types].xml` to distinguish WordprocessingML packages from PresentationML packages and generic ZIP archives.
2. **Pluggable Engine Trait (`DocxEngine`)**: Decouple the document layout engine from the presentation pipeline, enabling immediate MVP delivery and long-term parity evolution.
3. **Engine Selection**:
   - **MVP Track**: Integrate `rdocx` 0.17.x / `oxml-layout` 0.15.x in a dedicated crate (`office-terminal-docx`) to perform high-fidelity pagination.
   - **Parity Track**: Maintain an architectural path toward porting the upstream `office-open-xml-viewer` TypeScript layout engine to Rust.
4. **Vector-First Compilation Bridge**: Bridge paginated document frames (`oxml-layout::PageFrame`) directly into the project's intermediate representation (`VectorSlide`).
5. **Unified Document UX**: Seamlessly route `.docx` documents into Reader Mode ([PRD 14a](./14a-pdf-reader-ui-designs.md)), sharing continuous scrolling, two-page spread, zoom, outline navigation, and fulltext search with PDF.

---

## 2. Goals & Non-Goals

### 2.1 Goals
1. **Unified Terminal Experience**: View `.docx`, `.dotx`, and `.docm` files in Reader Mode by default with continuous vertical scrolling, responsive centered rendering, two-page side-by-side spreads, page jump, and zoom.
2. **Presentation Mode Parity**: Allow viewing in Slide Mode (`--mode slides`) for decks or reports formatted as landscape slides.
3. **True Vector Export Parity**: Export `.docx` pages directly to resolution-independent standalone SVG with selectable text, clean CSS styles, embedded vector paths, and PNG rasters (`ot export document.docx --format svg,png`).
4. **Interactive Navigation & Search**: Extract document headings into an interactive Table of Contents outline (`TocEntry`) and text runs into a searchable corpus (`SearchCorpus`) for in-terminal jump and search.
5. **Integrated Metafile Playback**: Pass embedded Windows Metafiles (EMF / WMF / EMF+) to `office-terminal-core`'s native metafile playback engine ([PRD 13](./13-metafile-pictures-and-vector-playback.md)) rather than failing or relying on external converters.
6. **Honest Fidelity Degradation**: Every unsupported Word feature degrades gracefully to a typed `WarningEvent` and visual placeholder—never panicking, crashing, or producing invisible holes.
7. **Strict Engineering Standards**:
   - 100% pure Rust (`#![forbid(unsafe_code)]`).
   - Hard `< 600` line source file limit across all submodules ([AGENTS.md](../AGENTS.md) §A).
   - Zero compiler warnings and zero Clippy warnings (`-D warnings`).
   - Feature-gated CLI compilation (`--features docx`) to avoid bloat in PPTX-only workflows.

### 2.2 Non-Goals
- **Document Editing or Saving**: Read-only consumption. No round-tripping, template manipulation, or file writing.
- **VBA Macro Execution**: `.docm` documents are opened read-only; macros are stripped and ignored.
- **External Subprocesses**: No spawning of LibreOffice, Pandoc, Microsoft Word, or Node.js.
- **Legacy Binary Formats**: Legacy `.doc` (Word 97-2003 OLE2 binary compound files) is rejected with a clear diagnostic error suggesting conversion.

---

## 3. Normative Dependencies & Engine Evaluation

### 3.1 Evaluated Candidates

| Candidate | Parser / DOM | Layout Engine | License | Dependency Footprint | Feasibility Assessment |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Upstream (`office-open-xml-viewer`)** | Rust (~62,000 lines) | TypeScript only (~81,900 lines) | MIT | Zero extra Rust crates | **Long-term parity target**. Parser compiles cleanly with our vendored `ooxml-common`, but pagination requires porting 82k lines of TypeScript layout/line-breaking to Rust. |
| **`rdocx` 0.17.x / `oxml-layout` 0.15.x** | Pure Rust (`rdocx-oxml`) | Pure Rust (~63,200 lines) | MIT | 147 dependencies (~39 MB binary cost) | **Selected for MVP**. Full feature coverage (tables, sections, headers/footers, footnotes, floats). One known divergence on `w:docGrid` with `w:spacing lineRule="auto"`. |
| **`zavora-docx` 0.1.4** | Fork of old `rdocx` | Pure Rust (~5,900 lines) | MIT OR Apache-2.0 | ~40 dependencies | **Rejected**. Incomplete layout engine; fails multi-column, floating shapes, and complex tables; page flow breaks on standard reference documents. |

### 3.2 Selected Engine Architecture: `rdocx` + `oxml-layout`
`rdocx` separates the document into clean modular crates:
- `rdocx-oxml`: Low-level strongly typed OPC and WordprocessingML DOM.
- `oxml-layout`: Abstract, backend-agnostic pagination engine producing `oxml_layout::PageFrame`.
- `rdocx`: High-level convenience facade.

**Dependency Footprint & Mitigation**:
- `cargo tree -p rdocx` pulls 147 normal dependency names.
- Duplicate font crates (`fontdb 0.23` and `ttf-parser 0.25` next to workspace versions) are isolated in `crates/office-terminal-docx/`.
- `zip = "9.0.3"` unifies cleanly with the workspace pinned `zip = "=9.0.4"`.
- By making `office-terminal-docx` an **optional dependency** gated behind `[features] docx = ["dep:office-terminal-docx"]` in `office-terminal-cli`, PPTX/PDF developer builds retain rapid compile times.

### 3.3 Engine Abstraction Trait (`DocxEngine`)
To protect the codebase from upstream breaking changes and enable seamless migration to the native parser/paginator in Phase 3, all engine interactions are isolated behind a trait:

```rust
pub trait DocxEngine: Send + Sync {
    /// Paginate the document into structured page frames.
    fn paginate(&self, docx_bytes: &[u8]) -> Result<Vec<Arc<oxml_layout::PageFrame>>, DocxError>;

    /// Extract document-level metadata and table of contents.
    fn table_of_contents(&self) -> Vec<TocEntry>;

    /// Extract plain text search corpus indexed by page.
    fn search_corpus(&self) -> Arc<dyn SearchCorpus>;

    /// Extract embedded media assets by relationship ID.
    fn extract_asset(&self, r_id: &str) -> Option<Arc<[u8]>>;
}
```

---

## 4. Architecture & Two-Stage Vector Pipeline

### 4.1 System Integration

```
Source (.docx file or stdin)
         │
         ▼
presentation_loader::sniff_container_format(&bytes)
         │
         ├── %PDF- magic                          ──► SourceFormat::Pdf
         ├── ZIP with ppt/presentation.xml        ──► SourceFormat::Pptx
         ├── ZIP with word/document.xml           ──► SourceFormat::Docx
         └── Other / Unknown                      ──► Error::UnsupportedFormat
         │
         ▼
PresentationLoader::load(&bytes) ──► DeckSource::Docx(Arc<dyn PageSource>)
         │
         ▼
DefaultRenderPipeline::new(Arc<dyn PageSource>)
         │
         ▼
DocxPageSource::compile_page(page_index)
         │  (Translates oxml_layout::PageFrame ──► VectorSlide)
         ▼
VectorSlide (Resolution-Independent Display List)
         │
         ├──► TinySkiaRasterizer ──► RgbaSlide ──► Kitty Graphics Driver (TUI)
         ├──► Vector SvgSerializer ──► Standalone Scalable Vector Graphic (.svg)
         └──► Headless PNG Exporter ──► High-Resolution Pixel Buffer (.png)
```

### 4.2 Module Layout (`crates/office-terminal-docx`)
To comply with [AGENTS.md](../AGENTS.md) §A (strict `< 600` line limit per file), the crate is divided into cohesive submodules:

```
crates/office-terminal-docx/
├── Cargo.toml
├── src/
│   ├── lib.rs                   # Public exports, registration hook (< 120 lines)
│   ├── detect.rs                # OPC package sniffing ([Content_Types].xml) (< 150 lines)
│   ├── error.rs                 # Typed DocxError & degradation taxonomy (< 180 lines)
│   ├── engine/
│   │   ├── mod.rs               # DocxEngine trait definition (< 100 lines)
│   │   └── rdocx_impl.rs        # rdocx/oxml-layout adapter (< 350 lines)
│   ├── source.rs                # PageSource implementation (< 250 lines)
│   ├── bridge/
│   │   ├── mod.rs               # PageFrame -> VectorSlide compiler (< 300 lines)
│   │   ├── text.rs              # Text run & glyph positioning (< 450 lines)
│   │   ├── paths.rs             # Borders, lines, and custom geometry (< 350 lines)
│   │   ├── tables.rs            # Table grid, cell fills, and borders (< 400 lines)
│   │   ├── images.rs            # Inline & anchored images, AssetStore (< 280 lines)
│   │   └── metafile.rs          # EMF/WMF vector playback bridge (< 220 lines)
│   ├── text_corpus.rs           # SearchCorpus extraction for search/find (< 250 lines)
│   ├── toc.rs                   # Headings to TocEntry mapper (< 200 lines)
│   └── limits.rs                # Resource bounds, timeouts, bomb protection (< 220 lines)
└── tests/
    ├── detection_tests.rs
    ├── layout_tests.rs
    └── golden_tests.rs
```

---

## 5. Layout-to-Vector Compilation Bridge

The compilation bridge converts `oxml_layout::PageFrame` into `office-terminal-core::VectorSlide`.

### 5.1 Coordinate Systems & Dimensional Units
- **WordprocessingML Units**: Points ($1\text{ pt} = 20\text{ dxa} = 1/72\text{ in}$).
- **Display List Units**: English Metric Units ($1\text{ pt} = 12,700\text{ EMUs}$, $1\text{ dxa} = 635\text{ EMUs}$).
- **Transformation Formula**:
  $$\text{emu} = \text{round}(\text{pt} \times 12{,}700.0)$$
- **Page Dimensions**: Computed per page from `PageFrame.width` and `PageFrame.height`, fully supporting mixed portrait and landscape sections in a single document.

### 5.2 Element Mapping Specification

| DOCX Layout Node (`oxml_layout`) | `VectorCommand` Target | Styling & Coordinate Handling |
| :--- | :--- | :--- |
| **Page Surface** | Canvas Background | Mapped to `DrawPath` with rect bounds `(0, 0, width_emu, height_emu)` filled with page background color (default `#FFFFFF`). |
| **Text Run / Glyph Run** | `DrawText(PositionedGlyphRun)` | Font family mapped through `fontdb`. Coordinates offset by baseline. Colors, underline strokes, and strikeout paths emitted explicitly. |
| **Paragraph & Cell Borders** | `DrawPath` | Width converted to EMU stroke. Solid, dashed, dotted, double lines mapped to `VectorStroke`. |
| **Cell Shading / Fills** | `DrawPath` | Cell boundary polygon mapped to `VectorPaint::Solid` with fill color. |
| **Inline / Floating Image** | `DrawBitmap` | Decoded raster bytes (PNG, JPEG, WebP) wrapped in `Arc<[u8]>`. Placed via `AffineTransform`. |
| **Embedded Metafile (EMF/WMF)** | Native Vector Expansion | Metafile bytes passed to `office_terminal_core::metafile::playback::play_metafile_record()`, emitting native `VectorCommand::DrawPath` commands. |
| **Header & Footer** | `VectorCommand` sequence | Positioned at top/bottom margins. Page number fields dynamically evaluated during pagination. |
| **Footnotes & Endnotes** | `DrawText` + `DrawPath` | Positioned at bottom of page frame above footer, separated by standard horizontal rule. |

### 5.3 Text & Font Handling
1. **Font Matching**:
   Word documents rely on common Windows fonts. The bridge maps standard Word font families to bundled open-source fonts when not available on the host:
   - `Calibri`, `Aptos` $\longrightarrow$ `Carlito` (bundled SIL OFL 1.1)
   - `Cambria` $\longrightarrow$ `Caladea` (bundled SIL OFL 1.1)
   - `Arial`, `Helvetica` $\longrightarrow$ `Nunito Sans` / `Arimo` (bundled)
   - `Courier New`, `Consolas` $\longrightarrow$ System monospace / `Cousine`
2. **Missing Glyphs**: Fallback to Unicode replacement glyphs; missing font metrics use conservative bounding box approximations.
3. **Selectable Text in SVG**: Emits native SVG `<text>` and `<tspan>` nodes with exact glyph positioning coordinates (`x`, `y`, `dx`), preserving selectable and searchable text.

---

## 6. Terminal Presentation & Reader Mode UX

### 6.1 Presentation Mode vs. Reader Mode
DOCX documents behave fundamentally as documents rather than slide decks:
- **Default Mode**: Always launches in **Reader Mode** ([PRD 14a](./14a-pdf-reader-ui-designs.md)), regardless of page count.
- **Manual Override**: The user may specify `--mode slides` (for landscape-oriented presentation decks authored in Word) or `--mode reader`.
- **Viewport Centering**: Document pages narrower than the terminal viewport are centered horizontally. Dual-page spreads (`--spread` or key 'd') position facing pages side-by-side with border separators.

### 6.2 Keybindings & Interaction

| Key | Context | Action |
| :--- | :--- | :--- |
| `j` / `Down` / `WheelDown` | Reader | Scroll viewport down by 2 terminal rows. |
| `k` / `Up` / `WheelUp` | Reader | Scroll viewport up by 2 terminal rows. |
| `Space` / `PageDown` | Reader | Advance viewport by one full screen page. |
| `b` / `PageUp` | Reader | Retreat viewport by one full screen page. |
| `d` | Reader | Toggle dual-page spread (side-by-side). |
| `+` / `-` / `0` | Reader | Zoom in (+10%), Zoom out (-10%), Reset zoom (100%). |
| `t` | Any | Open Table of Contents / Outline navigation modal. |
| `/` | Any | Open interactive text search overlay. |
| `n` / `N` | Search | Jump to next / previous search match. |
| `q` / `Esc` | Any | Quit application / close modal. |

---

## 7. CLI & Export Integration

### 7.1 Command-Line Matrix

```bash
# 1. Interactive terminal reading (defaults to Reader Mode)
ot view document.docx
ot view report.docm --spread

# 2. Presenter mode (for landscape slide documents)
ot present slides.docx --mode slides

# 3. Vector and raster export
ot export document.docx --format svg -o ./output/
ot export document.docx --format png --dpi 300 -o ./output/
ot export document.docx --pages 1-5 --format svg

# 4. Document introspection
ot info document.docx
# Outputs:
#   Format: WordprocessingML (ECMA-376)
#   Pages: 14
#   Page Dimensions: 8.5" x 11.0" (Letter, Portrait)
#   Sections: 2
#   TOC Entries: 8 headings
#   Media Assets: 6 images (4 PNG, 2 EMF)

# 5. Text & structure dump
ot dump document.docx --format markdown > document.md
ot dump document.docx --format text > document.txt
ot dump document.docx --format json > ast.json
```

---

## 8. Fidelity Policy & Degradation Matrix

Conforming to [PRD 02d: Fidelity Policy](./02d-element-renderers-and-fidelity-policy.md):

| Feature Category | Support Tier | Visual Behavior & Diagnostics |
| :--- | :--- | :--- |
| Standard Paragraphs & Typography | **FULL** | Exact kerning, margins, line spacing, alignments (left, center, right, justified). |
| Complex Tables (Merged, Borders) | **FULL** | Full table grid resolution, colSpan/rowSpan cells, padding, individual border styles. |
| Raster Images (PNG, JPEG, WebP) | **FULL** | Placed at calculated layout coordinates with aspect ratio maintained. |
| Vector Metafiles (EMF, WMF) | **FULL** | Emitted as native vector paths via `office-terminal-core::metafile`. |
| Headers, Footers, Page Numbers | **FULL** | Resolved per section and page parity (first-page, odd/even). |
| Footnotes & Endnotes | **FULL** | Positioned at bottom of page frame above footer with separator rule. |
| Shapes & Textboxes (DrawingML) | **FULL / DEGRADED** | Geometric shapes mapped to `DrawPath`. Complex 3D effects flattened to 2D approximation + `WarningEvent`. |
| Section Page Size/Orientation Changes | **FULL** | Dynamic page dimensions passed directly to `VectorSlide`. |
| `w:docGrid` with `w:spacing lineRule="auto"` | **DEGRADED** | Known `rdocx` pitch divergence; renders with standard font line height. Emits diagnostic warning. |
| Embedded Charts (ChartML) | **DEGRADED** | Fallback to cached chart image part if present; otherwise placeholder box + warning. |
| OMML Math Equations | **DEGRADED** | Inline math formatted via plain text Unicode representation or placeholder; future OMML engine ([PRD 09](./09-omml-equation-typesetting.md)) integration. |
| Tracked Changes & Comments | **DEGRADED** | Renders accepted text view; comments omitted from layout, surfaced in `ot dump --comments`. |
| Password-Protected / Encrypted DOCX | **ERROR** | Returns typed `DocxError::EncryptedDocument` with actionable prompt to decrypt before viewing. |
| VBA Macros (`.docm`) | **OMITTED** | Macros completely stripped for security. Document rendered normally. |

---

## 9. Security, Resource Limits & Robustness

### 9.1 Threat Vectors & Mitigations
1. **ZIP Decompression Bombs**:
   - Limit unpacked archive expansion ratio to at most **50:1**.
   - Maximum decompressed size capped at **500 MB** total per document.
2. **XML Entity Attacks (Billion Laughs)**:
   - Use secure XML parsing flags disabling external DTD resolution and recursive entity expansion.
3. **Deep Nested Table/Section Recursion**:
   - Maximum XML element depth capped at **256 levels**.
4. **Layout Loop Starvation**:
   - Limit layout computation time to **15 seconds** per document.
   - Limit maximum paginated page count to **1,000 pages** (configurable via `--max-pages`).
5. **Foreign Layout Panics**:
   - All pagination execution inside `rdocx`/`oxml-layout` is wrapped in `std::panic::catch_unwind`.
   - Any panic is intercepted, logged, and translated into a typed `DocxError::LayoutEnginePanic`, ensuring the interactive TUI never crashes.

---

## 10. Phased Implementation Roadmap

```
┌─────────────────────────────────────────────────────────────┐
│ Phase 0: 1–3 Day Validation Spike                           │
│ DOCX ──► PDF (via rdocx) ──► PdfPageSource ──► TUI Render    │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 1: MVP Direct Vector Bridge                           │
│ Crates: office-terminal-docx                                │
│ oxml_layout::PageFrame ──► VectorSlide                      │
│ Full CLI integration (view, present, export, info, dump)    │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 2: Parity Refinement & Native Features                │
│ docGrid line pitch fix upstream                             │
│ Native EMF/WMF playback integration                         │
│ Table of contents & fulltext search integration             │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 3: Long-term Native Engine                            │
│ Port upstream office-open-xml-viewer TS layout to Rust      │
│ Zero external non-workspace layout dependencies             │
└─────────────────────────────────────────────────────────────┘
```

### Milestone Deliverables

#### Milestone 1: Core Detection & Spiking (Phase 0)
- Add OPC content-type sniffer to `presentation_loader.rs`.
- Wire `rdocx` PDF export spike into `PdfPageSource` to validate user interaction and CLI commands.

#### Milestone 2: Direct Vector Bridge (Phase 1 MVP)
- Create `crates/office-terminal-docx/` with modular submodules under 600 lines.
- Implement `DocxPageSource` implementing `PageSource`.
- Implement `bridge/text.rs`, `bridge/paths.rs`, `bridge/tables.rs`, and `bridge/images.rs`.
- Wire `ot export document.docx --format svg,png`.
- Add unit and golden tests verifying zero regressions and zero Clippy warnings.

#### Milestone 3: Polish & Integration (Phase 2)
- Wire `office-terminal-core::metafile` into `bridge/metafile.rs` for EMF vector playback.
- Implement `SearchCorpus` and `TocEntry` for interactive search and outline navigation.
- Implement strict resource bounds and `catch_unwind` protections.

#### Milestone 4: Native Engine Track (Phase 3)
- Vendor upstream `office-open-xml-viewer` DOCX parser.
- Incrementally port TypeScript line-breaking and layout engine slices into pure Rust.
- Eliminate external `rdocx` dependency tree.

---

## 11. Quality, Verification & Acceptance Criteria

### 11.1 Acceptance Criteria
1. **Line Limit Invariant**: Every Rust source file in `crates/office-terminal-docx` must have **fewer than 600 lines** of code (`wc -l`).
2. **Zero-Warning Invariant**: `cargo clippy --workspace --all-targets -- -D warnings` must compile with **zero warnings**.
3. **Safety Invariant**: `#![forbid(unsafe_code)]` declared in `crates/office-terminal-docx/src/lib.rs`.
4. **Format Parity**:
   - `ot view document.docx` opens immediately in Reader mode.
   - `ot export document.docx --format svg` generates valid, standalone SVG with selectable `<text>` elements.
   - `ot export document.docx --format png` generates clean raster images matching visual layout.
5. **Regression Invariant**: Existing PPTX and PDF test suites must pass 100% without changes.

### 11.2 Verification Test Suite
- **L1 Unit Tests**: Detection logic, EMU conversion math, style inheritance, table span calculations.
- **L2 Scene Goldens**: Stable JSON AST snapshots of compiled `VectorSlide` structures for standard DOCX fixtures (headings, lists, 5x5 table, floating image, multi-column section).
- **L3 Visual Comparison**: Automated DSSIM difference threshold ($\le 0.05$) against rendered LibreOffice / Microsoft Word PDF export baselines.

---

## 12. References & Standards
- **ECMA-376 5th Edition Part 1**:
  - §17.3.1.33 `w:spacing` / `w:lineRule` (Line Spacing Semantics)
  - §17.6.5 `w:docGrid` (Document Line Grid Calculation)
  - §17.4.x (Table Properties, Cell Margins, and Spanning Rules)
  - §17.3.1.20 `w:lastRenderedPageBreak` (Page Break Hints)
- **ISO/IEC 29500-1:2016**: Information technology — Document description and processing languages — Office Open XML File Formats.
- **Upstream Viewer**: [office-open-xml-viewer](https://github.com/yukiyokotani/office-open-xml-viewer) (Commit `59b28ea9eb0dd9e8770e5bdcb0437592b8098cd3`).
- **Layout Reference**: [rdocx](https://github.com/tensorbee/rdocx) (Commit `eb980e94c8ae1f8b42612f918482bc627e0141a0`).
