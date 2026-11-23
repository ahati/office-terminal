# PRD 15: DOCX Input Support — WordprocessingML Documents & Vector Layout Bridge

> **Status:** Normative Design Specification (Full Implementation).
> **Feature Track:** Native read-only WordprocessingML (`.docx`, `.dotx`, `.docm`) document viewing and vector export.
> **Depends on:**
> - [PRD 00: System Overview](./00-system-overview.md) (architecture, CLI binaries, and invariants)
> - [PRD 01: Parser & Data Model](./01-parser-and-data-model.md) (loader, container sniffing, asset store)
> - [PRD 02a: Composition Core & Pipeline](./02a-composition-core-and-render-pipeline.md) (two-tier caching, prefetch queue)
> - [PRD 02c: Typography & Text Layout](./02c-typography-and-text-layout.md) (font matching, glyph shaping, bundled fonts)
> - [PRD 02d: Element Compilers & Fidelity Policy](./02d-element-renderers-and-fidelity-policy.md) (honest degradation matrix)
> - [PRD 06: Component Interface Contracts](./06-component-interface-contracts.md) (`VectorSlide`, `PageSource`, `DeckSource`)
> - [PRD 07: Quality & Testing](./07-quality-and-testing.md) (test levels, DSSIM scoring, golden scenes)
> - [PRD 09: Live OMML Equation Typesetting](./09-omml-equation-typesetting.md) (TeX box-and-glue math layout engine)
> - [PRD 13: Metafile Pictures & Vector Playback](./13-metafile-pictures-and-vector-playback.md) (EMF/WMF playback)
> - [PRD 14: PDF Input Support](./14-pdf-input-support.md) (`PageSource` abstraction, multi-page document model)
> - [PRD 14a: PDF & Document Reader UI Designs](./14a-pdf-reader-ui-designs.md) (continuous scroll, dual-page reader mode)
> **Issue Reference:** [Issue #40](https://github.com/ahati/office-terminal/issues/40)

---

## 1. Executive Summary & Problem Statement

### 1.1 Context & Current Limitations
Office Terminal (`ot` / `office-terminal`) renders Microsoft PowerPoint (`.pptx`) presentations and Portable Document Format (`.pdf`) files directly to the terminal using the Kitty Graphics Protocol, alongside headless vector SVG and raster PNG exports.

Currently, any `.docx` file supplied to `ot` is rejected at the container boundary:
```
$ ot view report.docx
Error: Failed to load presentation: Missing required OOXML part: ppt/presentation.xml
```
Because `sniff_container_format` in `crates/office-terminal-core/src/loader/presentation_loader.rs` checks only for the generic ZIP local-file header (`[0x50, 0x4B, 0x03, 0x04]`), every OOXML package is presumed to be a PresentationML deck (`SourceFormat::Pptx`). The parser then fails when presentation parts are missing.

### 1.2 The Document Flow Challenge: Flow Markup vs. Fixed Geometry
Unlike PPTX (where shapes and text frames have explicit coordinates stored in the markup) and PDF (where glyphs and graphics have pre-calculated coordinates), WordprocessingML (`.docx`) is a **flow document**:
- Content flows across paragraphs, runs, inline shapes, and tables without explicit page coordinates.
- **Pagination, line breaking, justification, hyphenation, table layout, and floating object positioning must be computed dynamically** by a layout engine following the pagination rules of ECMA-376 / ISO/IEC 29500.
- Parsing the OPC package and XML DOM is straightforward; **layout and pagination is the critical engineering challenge**.

### 1.3 Complete Implementation Vision
This specification defines the complete, full implementation of DOCX support:
1. **Container Sniffing**: Inspect `[Content_Types].xml` to distinguish WordprocessingML packages from PresentationML packages and generic ZIP archives.
2. **Pluggable Engine Architecture (`DocxEngine`)**: Decouple the document layout engine from the presentation pipeline, utilizing pure-Rust `rdocx` / `oxml-layout` for pagination.
3. **Comprehensive Vector Compilation Bridge**: Bridge paginated document frames (`oxml_layout::PageFrame`) directly into the project's intermediate representation (`VectorSlide`).
4. **Integrated Domain Engines**:
   - **OMML Math**: Wire mathematical equations directly into `office-terminal-core`'s live typesetting engine ([PRD 09](./09-omml-equation-typesetting.md)).
   - **Windows Metafiles**: Wire embedded EMF/WMF vector pictures directly into `office-terminal-core`'s native metafile playback engine ([PRD 13](./13-metafile-pictures-and-vector-playback.md)).
5. **Unified Document UX**: Seamlessly route `.docx` documents into Reader Mode ([PRD 14a](./14a-pdf-reader-ui-designs.md)), sharing continuous scrolling, two-page spread, zoom, outline navigation, and fulltext search with PDF.

---

## 2. In-Scope vs. Out-of-Scope Specification

To establish absolute clarity on implementation boundaries, capabilities are divided strictly between what is implemented and what is excluded.

### 2.1 Complete In-Scope Capabilities
The implementation includes complete, end-to-end support for:
1. **Container & Format Detection**:
   - Auto-detection of `.docx`, `.dotx`, and `.docm` packages via `[Content_Types].xml` inspection.
2. **Typography & Paragraph Layout**:
   - Paragraph alignment: left, center, right, and fully justified.
   - First-line indents, hanging indents, margins, paragraph before/after spacing.
   - Text styling: bold, italic, underline (single, double, dotted, dashed), strikethrough, superscript, subscript.
   - Colors: font colors, character highlight colors, paragraph background shading.
   - Font substitution: automatic mapping of Word fonts (`Calibri`, `Cambria`, `Arial`) to bundled fonts (`Carlito`, `Caladea`, `Nunito Sans`).
3. **Page Geometry & Sections**:
   - Dynamic multi-page pagination.
   - Multi-section documents with mixed orientations (e.g., Portrait $\to$ Landscape table $\to$ Portrait).
   - Standard and custom paper dimensions (Letter, A4, Legal, Executive, custom pt sizes).
   - Margins: top, bottom, left, right, header, and footer offsets.
4. **Headers, Footers & Numbering**:
   - First-page headers and footers.
   - Odd and even page headers and footers.
   - Dynamically evaluated page numbering fields (`PAGE`, `NUMPAGES`).
   - Footnotes and endnotes positioned at page bottoms with standard separator rules.
5. **Tables**:
   - Multi-row and multi-column grid layouts with explicit and automatic column sizing.
   - Horizontal cell merging (`gridSpan`) and vertical cell merging (`vMerge`).
   - Cell padding, text margins, and cell background shading.
   - Individual border styling: solid, dashed, dotted, double, varying thicknesses, and colors.
6. **Images & Embedded Vector Metafiles**:
   - Inline and floating raster images (PNG, JPEG, WebP, GIF, BMP).
   - Windows Metafiles (EMF, WMF, EMF+) executed through `office-terminal-core::metafile` into native vector paths.
7. **Mathematical Equations**:
   - OMML equations (`<m:oMath>`, `<m:oMathPara>`) routed directly to `office-terminal-core::compiler::math` ([PRD 09](./09-omml-equation-typesetting.md)) for live vector typesetting.
8. **Navigation & Interactivity**:
   - Interactive Table of Contents ('t') extracted from document heading styles (`Heading 1`..`Heading 6`).
   - Fulltext search corpus ('/') indexing text runs by page and position.
   - Continuous vertical scrolling, dual-page spread ('d'), and zoom ('+', '-').
9. **Export Parity**:
   - Headless SVG export with selectable `<text>` spans and clean CSS styles.
   - High-resolution headless PNG export at arbitrary DPI.
   - Document structure and plain-text extraction via `ot dump` and `ot info`.

### 2.2 Explicitly Out-of-Scope Capabilities & Rationale
The following capabilities are deliberately excluded from this implementation:

| Out-of-Scope Item | Description & Rationale | System Behavior |
| :--- | :--- | :--- |
| **Document Editing & Saving** | Office Terminal is strictly a viewer and vector export engine. Document authoring, editing, re-saving, or template rewriting are non-goals. | Read-only access. No mutation or serialization to `.docx`. |
| **VBA Macro Execution (`.docm`)** | Executing embedded Visual Basic for Applications (VBA) code poses severe supply-chain and arbitrary code execution security risks. | Macros are stripped and completely ignored. The document body renders normally. |
| **Active Form Controls & Submission** | Interactive form controls (`w:formField`, legacy checkboxes, dropdowns, ActiveX controls) are designed for Word data-entry templates. | Static display of current saved values. Controls cannot be toggled or edited. |
| **Password Protection & RMS Encryption** | Decrypting proprietary Office binary encryption, Agile encryption, or Rights Management Services (RMS) requires secret keys and cryptographic negotiation. | Returns typed error `DocxError::EncryptedDocument` requiring user decryption before opening. |
| **Legacy Binary Word (`.doc`)** | Word 97–2003 binary compound files (CFB / OLE2) use an entirely different, deprecated binary specification. | Rejected at container detection with a typed error advising conversion to `.docx`. |
| **Active OLE Object Server Activation** | Embedded Excel spreadsheets or PowerPoint presentations inside Word rely on Windows COM / OLE automation to launch external server applications. | Renders the cached static preview image part (`image1.png`) bundled in the package. |
| **Dynamic Chart Generation without Preview** | Generating complex ChartML from scratch without Microsoft Excel requires an independent charting engine. | Renders cached preview part if present; otherwise emits labeled placeholder box + warning. |
| **Tracked Changes Margin Balloons** | Interactive revision tracking (Word "Markup View") with floating margin bubbles and revision history. | Renders the "Final / Accepted" document view. Comment text is accessible via `ot dump --comments`. |
| **Text Reflow to Raw Terminal Cells** | Reflowing prose into monospace character cells destroys document typography, table alignment, and graphical layout. | Renders high-fidelity graphical page surfaces via Kitty Graphics Protocol. |

---

## 3. Normative Dependencies & Engine Architecture

### 3.1 Evaluated Candidates & Architecture Decision

| Candidate | Parser / DOM | Layout Engine | License | Dependency Footprint | Assessment |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`rdocx` 0.17.x / `oxml-layout` 0.15.x** | Pure Rust (`rdocx-oxml`) | Pure Rust (~63,200 lines) | MIT | 147 dependencies (~39 MB binary cost) | **Selected Engine**. Complete feature coverage (sections, tables, headers/footers, footnotes, floats). Pure Rust, `#![forbid(unsafe_code)]`. |
| **Upstream (`office-open-xml-viewer`)** | Rust (~62,000 lines) | TypeScript only (~81,900 lines) | MIT | Zero extra Rust crates | **Long-term parity target**. Parser compiles cleanly with vendored `ooxml-common`, but layout engine is TypeScript-only. |
| **`zavora-docx` 0.1.4** | Fork of old `rdocx` | Pure Rust (~5,900 lines) | MIT OR Apache-2.0 | ~40 dependencies | **Rejected**. Incomplete layout; fails multi-column, floats, and complex tables; broken page flow. |

### 3.2 Selected Engine: `rdocx` + `oxml-layout`
The implementation uses `rdocx` 0.17.x and `oxml-layout` 0.15.x:
- `rdocx-oxml`: Strongly typed OPC and WordprocessingML DOM.
- `oxml-layout`: Backend-agnostic pagination engine producing `oxml_layout::PageFrame`.
- Isolated in a dedicated crate (`crates/office-terminal-docx`).
- Pinned in workspace `Cargo.toml`.
- CLI compilation gated behind `[features] docx = ["dep:office-terminal-docx"]` to allow fast PPTX-only development builds.

### 3.3 Engine Abstraction Trait (`DocxEngine`)
All document pagination is encapsulated behind a clean trait (the shipped form lives in `crates/office-terminal-docx/src/engine.rs`; it exposes the engine's *output*, so nothing downstream sees an `rdocx` type):

```rust
pub trait DocxEngine: Send + Sync {
    fn page_count(&self) -> usize;
    /// Page frame by 0-based index (`oxml_layout::PageFrame`, backend-neutral).
    fn page(&self, index: usize) -> Option<Arc<PageFrame>>;
    /// Font identity by the engine's font id (glyph runs reference it).
    fn resolve_font(&self, id: FontId) -> Option<&ResolvedFont>;
    /// Every media id referenced by positioned images (for the asset store).
    fn media_ids(&self) -> Vec<MediaId>;
    /// Raw media bytes by id, when the engine can supply them.
    fn media(&self, id: MediaId) -> Option<MediaBytes>;
    /// Heading outline entries with 0-based page indices.
    fn outlines(&self) -> &[OutlineEntry];
    /// 0-based physical page of a placed bookmark target (internal links).
    fn bookmark_page(&self, name: &str) -> Option<usize>;
    /// Rendering approximations collected during layout.
    fn diagnostics(&self) -> &[String];
    /// Document metadata (title, author, …), when the package declares it.
    fn metadata(&self) -> Option<&DocumentMetadata>;
}
```

---

## 4. End-to-End System Architecture & Pipeline

### 4.1 Pipeline Integration Flow

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
         ├── Math nodes ──► office_terminal_core::compiler::math (PRD 09)
         └── Metafiles  ──► office_terminal_core::metafile (PRD 13)
         ▼
VectorSlide (Resolution-Independent Display List)
         │
         ├──► TinySkiaRasterizer ──► RgbaSlide ──► Kitty Graphics Driver (TUI)
         ├──► Vector SvgSerializer ──► Standalone Scalable Vector Graphic (.svg)
         └──► Headless PNG Exporter ──► High-Resolution Pixel Buffer (.png)
```

### 4.2 Crate Structure (`crates/office-terminal-docx`) & Budget
Conforming strictly to [AGENTS.md](../AGENTS.md) §A (all source files under 600 lines):

```
crates/office-terminal-docx/
├── Cargo.toml
├── src/
│   ├── lib.rs                   # Registration hook, exports (< 120 lines)
│   ├── detect.rs                # OPC package sniffing ([Content_Types].xml) (< 150 lines)
│   ├── error.rs                 # Typed DocxError & degradation taxonomy (< 180 lines)
│   ├── engine/
│   │   ├── mod.rs               # DocxEngine trait definition (< 100 lines)
│   │   └── rdocx_impl.rs        # rdocx/oxml-layout engine adapter (< 350 lines)
│   ├── source.rs                # PageSource implementation (< 250 lines)
│   ├── bridge/
│   │   ├── mod.rs               # PageFrame -> VectorSlide compiler (< 300 lines)
│   │   ├── text.rs              # Text run & glyph positioning (< 450 lines)
│   │   ├── paths.rs             # Borders, lines, and custom geometry (< 350 lines)
│   │   ├── tables.rs            # Table grid, cell fills, and borders (< 400 lines)
│   │   ├── images.rs            # Inline & anchored images, AssetStore (< 280 lines)
│   │   ├── math.rs              # PRD 09 OMML typesetting bridge (< 260 lines)
│   │   └── metafile.rs          # PRD 13 EMF/WMF playback bridge (< 220 lines)
│   ├── text_corpus.rs           # SearchCorpus extraction for search/find (< 250 lines)
│   ├── toc.rs                   # Headings to TocEntry outline mapper (< 200 lines)
│   └── limits.rs                # Resource bounds, timeouts, bomb protection (< 220 lines)
└── tests/
    ├── detection_tests.rs
    ├── layout_tests.rs
    └── golden_tests.rs
```

---

## 5. Complete Vector Compilation Bridge

The compilation bridge converts `oxml_layout::PageFrame` into `office-terminal-core::VectorSlide`.

### 5.1 Coordinate Systems & Dimensional Mapping
- **WordprocessingML Units**: Points ($1\text{ pt} = 20\text{ dxa} = 1/72\text{ in}$).
- **Display List Units**: English Metric Units ($1\text{ pt} = 12,700\text{ EMUs}$, $1\text{ dxa} = 635\text{ EMUs}$).
- **Coordinate Transformation**:
  $$\text{emu} = \text{round}(\text{pt} \times 12{,}700.0)$$
- **Page Dimensions**: Computed per page from `PageFrame.width` and `PageFrame.height`, supporting mixed portrait/landscape sections.

### 5.2 Comprehensive Element Mapping Matrix

| DOCX Layout Node (`oxml_layout`) | `VectorCommand` Target | Styling & Coordinate Handling |
| :--- | :--- | :--- |
| **Page Surface** | Canvas Background | Mapped to `DrawPath` with rect bounds `(0, 0, width_emu, height_emu)` filled with page background color (default `#FFFFFF`). |
| **Text Run / Glyph Run** | `DrawText(PositionedGlyphRun)` | Font family mapped through `fontdb`. Baseline offsets applied. Colors, underline strokes, and strikeout paths emitted explicitly. |
| **Paragraph & Cell Borders** | `DrawPath` | Width converted to EMU stroke. Solid, dashed, dotted, double lines mapped to `VectorStroke`. |
| **Cell Shading / Fills** | `DrawPath` | Cell boundary polygon mapped to `VectorPaint::Solid` with fill color. |
| **Inline / Floating Image** | `DrawBitmap` | Decoded raster bytes (PNG, JPEG, WebP) wrapped in `Arc<[u8]>`. Placed via `AffineTransform`. |
| **Embedded Metafile (EMF/WMF)** | Native Vector Expansion | Detected by content type (`image/x-emf`, `image/x-wmf`, …); the bridge emits a `DrawMetafile` marker keyed by the content-addressed media id, and `office_terminal_core::metafile::expand_metafiles` plays the bytes into native `VectorCommand::DrawPath` commands (format sniffed from the bytes, PRD 13). |
| **OMML Math Equation** | `DrawText` + `DrawPath` | Laid out by the engine (§5.3); glyphs paint from the exact face handle, rules map to `DrawPath`. |
| **Header & Footer** | `VectorCommand` sequence | Positioned at top/bottom margins. Page number fields dynamically evaluated during pagination. |
| **Footnotes & Endnotes** | `DrawText` + `DrawPath` | Positioned at bottom of page frame above footer, separated by standard horizontal rule. |

### 5.3 OMML Math Integration

Equations are laid out by the selected engine, not by `office-terminal-core`'s TeX-style engine ([PRD 09](./09-omml-equation-typesetting.md)):
1. `rdocx-layout` lays out `<m:oMath>` / `<m:oMathPara>` itself (its `math.rs`), because math must participate in line height, baseline and pagination decisions the engine owns.
2. The result reaches the bridge as ordinary positioned glyph runs carrying the exact face handle (§5.5), so fractions, radicals and scripts paint from the authoritative face bytes on every backend.
3. Rule lines (fraction bars, radical vinculums) arrive as `PositionedElement::Line`/`Path` and map to `DrawPath`.

PRD 09 remains the typesetting path for PPTX OMML; routing DOCX equations through it would require re-laying out text the engine has already paginated and is deferred to the native-engine parity track (issue #40 Phase 6).

### 5.4 Native Metafile Playback Integration (PRD 13)
When an embedded image is identified as a Windows Metafile (`image/x-emf`, `image/x-wmf`, `image/emf`, `image/wmf`):
1. `bridge/metafile.rs` bypasses the standard raster decoder.
2. Passes raw bytes to `office_terminal_core::metafile::playback::play_metafile_record()`.
3. Vector records are mapped directly to `VectorCommand::DrawPath` with precise transforms, preserving infinite vector scalability in terminal rendering and SVG export.

### 5.5 Typography & Font Substitution Strategy
1. **Core Font Mapping Matrix**:
   - `Calibri`, `Aptos` $\longrightarrow$ `Carlito` (bundled SIL OFL 1.1)
   - `Cambria` $\longrightarrow$ `Caladea` (bundled SIL OFL 1.1)
   - `Arial`, `Helvetica` $\longrightarrow$ `Nunito Sans` / `Arimo` (bundled)
   - `Courier New`, `Consolas` $\longrightarrow$ System monospace / `Cousine`
2. **Missing Glyphs**: Fallback to Unicode replacement glyphs; missing font metrics use conservative bounding box approximations.
3. **Selectable Text in SVG**: Emits native SVG `<text>` and `<tspan>` elements with exact glyph positioning coordinates (`x`, `y`, `dx`), preserving selectable and searchable text in exports.

---

## 6. Terminal Presentation & Reader Mode UX

### 6.1 Dual View Modes: Reader Mode & Slide Mode
- **Default Mode (Reader Mode)**: All Word documents open in Reader Mode ([PRD 14a](./14a-pdf-reader-ui-designs.md)):
  - Continuous vertical scrolling with kinetic momentum.
  - Horizontally centered pages when narrower than the terminal viewport.
  - Dual-page side-by-side spread ('d') for reading book/report spreads.
- **Slide Mode Override**: Specifying `--mode slides` switches to Presentation Mode with centered slides, thumbnail sorter ('s'), and presentation timer (useful for landscape slide decks authored in Word).

### 6.2 Keybindings & Document Navigation

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

## 7. CLI Subcommands & Export Parity

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
#   File:           document.docx
#   Format:         WordprocessingML (ECMA-376)
#   Pages:          14 pages
#   Dimensions:     612.0 x 792.0 pt (216 x 279 mm, Letter, Portrait)
#   Aspect Ratio:   17:22
#   TOC Entries:    8 headings
#   Media Assets:   6 images (1.8 MB)
#   Title:          Quarterly report        (only when docProps/core.xml declares it)
#   Author:         A. Writer               (only when docProps/core.xml declares it)
#   Subject:        …                       (only when docProps/core.xml declares it)

# 5. Text & structure dump
ot dump document.docx --format markdown > document.md   # `--markdown`, one "# Page N" section per page
ot dump document.docx --format json > ast.json         # `--json`, paged reading-order corpus
# `--notes` / `--comments` print per-page "(No notes)" / "(No comments)" placeholders:
# WordprocessingML has no speaker notes, and comments are intentionally not
# rendered or extracted in v1 (issue #40 §7.11).
# `--ast` is PPTX-only: a DOCX falls through to the paged hint
# "Document has N pages. Use --notes, --comments, --markdown, or --json."
```

---

## 8. Security, Resource Limits & Robustness

### 8.1 Threat Modeling & Bounds

Bounds are enforced in two places: package reads pass through `oxml_opc::PackageReadLimits` inside the engine, and the engine's output is checked after layout (`DocxLimits`, issue #40 §7.14). None of them is a user-facing knob in v1; `DocxLimits` is a single struct the CLI can expose later.

1. **ZIP Decompression Bombs**:
    - Maximum OPC entries: **10,000**.
    - Maximum decompressed size of a single part: **64 MiB**.
    - Maximum total decompressed size: **512 MiB** per document.
    - Crossing any of them is a typed `DocxError::ResourceLimit` (stage `docx-package`), never an OOM and never a "corrupt archive" misreport.
2. **XML Entity Attacks (Billion Laughs)**:
    - Parsing is delegated to the engine's hardened XML stack; no DTD resolution and no external entities.
3. **Deep Nested Table/Section Recursion**:
    - Bounded by the engine's own recursion limits; a panic is contained by §8.2.
4. **Layout Loop Starvation**:
    - **Page cap**: a document paginating past **10,000 pages** is rejected as a typed `DocxError::ResourceLimit` (stage `docx-layout`, metric `pages`). The check runs after layout, because the engine offers no mid-pagination abort; the cap bounds what the pipeline holds in memory, not layout time.
    - **Soft deadline**: layout longer than **30 s** records a diagnostic surfaced once as `WarningEvent::ResourceLimitHit { limit: "docx-layout-deadline" }` (a resource observation, not a fidelity loss). Pagination is never killed mid-run (issue #40 §7.14).

### 8.2 Panic Boundary Isolation (`catch_unwind`)
All pagination execution inside `rdocx`/`oxml-layout` is wrapped in `std::panic::catch_unwind`. Any unexpected panic is intercepted, logged, and translated into a typed `DocxError::LayoutEnginePanic`, guaranteeing that the interactive TUI presenter never crashes on malformed files.

---

## 9. Fidelity Policy & Degradation Matrix

Conforming to [PRD 02d: Fidelity Policy](./02d-element-renderers-and-fidelity-policy.md):

| Feature Category | Support Tier | Visual Behavior & Diagnostics |
| :--- | :--- | :--- |
| Standard Paragraphs & Typography | **FULL** | Exact kerning, margins, line spacing, alignments (left, center, right, justified). |
| Complex Tables (Merged, Borders) | **FULL** | Full table grid resolution, colSpan/rowSpan cells, padding, individual border styles. |
| Raster Images (PNG, JPEG, WebP) | **FULL** | Placed at calculated layout coordinates with aspect ratio maintained. |
| Vector Metafiles (EMF, WMF) | **FULL** | Emitted as native vector paths via `office-terminal-core::metafile`. |
| OMML Math Equations | **FULL** | Laid out by the engine (§5.3); glyphs painted as exact-face vector text, rules as native paths. |
| Headers, Footers, Page Numbers | **FULL** | Resolved per section and page parity (first-page, odd/even). |
| Footnotes & Endnotes | **FULL** | Positioned at bottom of page frame above footer with separator rule. |
| Shapes & Textboxes (DrawingML) | **FULL / DEGRADED** | Geometric shapes mapped to `DrawPath`. Complex 3D effects flattened to 2D approximation + `WarningEvent`. |
| Section Page Size/Orientation Changes | **FULL** | Dynamic page dimensions passed directly to `VectorSlide`. |
| `w:docGrid` with `w:spacing lineRule="auto"` | **DEGRADED** | Known `rdocx` pitch divergence; renders with standard font line height. Emits diagnostic warning. |
| Embedded Charts (ChartML) | **DEGRADED** | Fallback to cached chart image part if present; otherwise placeholder box + warning. |
| Tracked Changes & Comments | **DEGRADED** | Renders accepted text view; comments omitted from layout, surfaced in `ot dump --comments`. |
| Password-Protected / Encrypted DOCX | **ERROR** | Returns typed `DocxError::EncryptedDocument` with actionable prompt to decrypt before viewing. |
| VBA Macros (`.docm`) | **OMITTED** | Macros completely stripped for security. Document rendered normally. |

---

## 10. Verification, Testing & Acceptance Criteria

### 10.1 Acceptance Criteria
1. **Line Limit Invariant**: Every Rust source file in `crates/office-terminal-docx` must have **fewer than 600 lines** of code (`wc -l`).
2. **Zero-Warning Invariant**: `cargo clippy --workspace --all-targets -- -D warnings` must compile with **zero warnings**.
3. **Safety Invariant**: `#![forbid(unsafe_code)]` declared in `crates/office-terminal-docx/src/lib.rs`.
4. **Format Parity**:
   - `ot view document.docx` opens immediately in Reader mode.
   - `ot export document.docx --format svg` generates valid, standalone SVG with selectable `<text>` elements.
   - `ot export document.docx --format png` generates clean raster images matching visual layout.
5. **Regression Invariant**: Existing PPTX and PDF test suites must pass 100% without regressions.

### 10.2 Quality & Test Regime
- **L1 Unit Tests**: Detection logic, EMU conversion math, style inheritance, table span calculations.
- **L2 Scene Goldens**: Stable JSON AST snapshots of compiled `VectorSlide` structures for standard DOCX fixtures (headings, lists, 5x5 table, floating image, multi-column section, OMML math).
- **L3 Visual Comparison**: Automated DSSIM difference threshold ($\le 0.05$) against rendered LibreOffice / Microsoft Word PDF export baselines.

**Status (v1):** L1 is in place at both levels — hand-authored `PageFrame` unit tests for every bridge module, and in-process package fixtures (headings, hyperlinks, a `gridSpan`/`vMerge`/bordered/shaded table, header + footer with a `PAGE` field, an inline PNG, OMML, declared core properties) driven through the real engine, rasterizer and SVG serializer in `engine_roundtrip_tests.rs` and `docx_cli_tests.rs`. L2 goldens and L3 visual comparison are deferred until licensed real-world fixtures are available (issue #40 §8); the shared fixture module lives in `crates/office-terminal-docx/tests/common/` and is reused by the CLI tests via `#[path]`.

---

## 11. References & Standards
- **ECMA-376 5th Edition Part 1**:
  - §17.3.1.33 `w:spacing` / `w:lineRule` (Line Spacing Semantics)
  - §17.6.5 `w:docGrid` (Document Line Grid Calculation)
  - §17.4.x (Table Properties, Cell Margins, and Spanning Rules)
  - §17.3.1.20 `w:lastRenderedPageBreak` (Page Break Hints)
  - §22.1 (Office Math Markup Language)
- **ISO/IEC 29500-1:2016**: Information technology — Document description and processing languages — Office Open XML File Formats.
- **Layout Engine Reference**: [rdocx](https://github.com/tensorbee/rdocx) (Commit `eb980e94c8ae1f8b42612f918482bc627e0141a0`).
