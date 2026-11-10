# PRD 01: Core Parser & Data Model Integration

## 1. Purpose & Scope
This document specifies the integration of the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer) into `kitty-pptx`: how `.pptx` archives are opened, how PresentationML parts and inheritance trees are resolved, what the internal data structures carry (verified against the vendored source), and how media assets are managed. The access interfaces (`PresentationLoader`, `AssetStore`) are specified normatively in [PRD 06](./06-component-interface-contracts.md) §3, §5.

---

## 2. Upstream Parser Architecture & Integration

The upstream project structures its PPTX implementation into two core Rust crates:
- `packages/pptx/model` (`pptx-model`): Intermediate AST representing presentations, slides, shapes, text, fills, and charts.
- `packages/pptx/parser` (`pptx-parser`): OPC (Open Packaging Conventions) archive reader and XML deserializer using `roxmltree`, `zip`, and `ooxml-common`.

```
┌──────────────────────────────────────────────────────────┐
│                   deck.pptx (ZIP File)                   │
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
└──────────────────────────────────────────────────────────┘
```

### 2.1 Workspace Crate Vendoring
`kitty-pptx` consumes these upstream crates directly in a cargo workspace:
```toml
[dependencies]
ooxml-common = { path = "./crates/ooxml-common", default-features = false }
pptx-model = { path = "./crates/pptx-model", default-features = false }
pptx-parser = { path = "./crates/pptx-parser", default-features = false }
```
*Note*: `pptx-parser` depends directly on `ooxml-common` (for OPC relationships, namespaces, and depth/resource guards) and `pptx-model`. The `wasm-entry` feature in `pptx-parser` (the default) gates the `wasm-bindgen` exports; disabling it compiles the parser as a lean native library. The native entry points `parse_pptx_model_native` and `to_markdown_native` are unconditional and used by this project. *(Verified against the upstream `Cargo.toml` and `parser/src/lib.rs`.)*

### 2.2 Upstream Synchronization Policy
The upstream repository moves quickly (AI-authored, spec-first, fuzz-tested; migration docs for multiple minor versions in-tree). Policy:
1. **Pin**: the vendored copy records the upstream commit hash in `crates/VENDORED.md`.
2. **Sync**: re-vendor deliberately, not opportunistically; a sync PR runs the full golden corpus ([PRD 07](./07-quality-and-testing.md) §4) — model changes that alter rendered output re-baseline in the same PR.
3. **Patch-carrying**: local changes to vendored crates are kept as a numbered patch series in `crates/patches/`, never in-place edits, so future syncs are mechanical.

---

## 3. The OOXML PresentationML Pipeline

### 3.1 Bootstrap Phase
1. **`[Content_Types].xml` & Root Relationships (`_rels/.rels`)**: locates the primary presentation part (`ppt/presentation.xml`).
2. **`ppt/presentation.xml`**:
   - Slide dimensions (`p:sldSz @cx/@cy`; 16:9 default 9144000×5143500 EMU, i.e. 10×5.625 in — see the corrected math table in [PRD 02a](./02a-composition-core-and-render-pipeline.md) §2.2).
   - Slide ordering (`p:sldIdLst`).
3. **Themes & Master Inheritance** (`ppt/slideMasters/`, `ppt/slideLayouts/`, `ppt/theme/`):
   - The parser resolves the theme palette and the full ECMA-376 modifier chain (`lumMod`, `lumOff`, `tint`, `shade`, `satMod`, hue rotation) and **bakes the results into concrete sRGB hex strings** in the model. Renderers never re-derive theme colors ([PRD 02b](./02b-vector-style-and-geometry-engine.md) §2).
   - Layout placeholder hierarchies resolve Master → Layout → Slide; the model emits a flat, inheritance-resolved element list per slide, each element tagged with its `SlideElementOrigin` (Master | Layout | Slide).

### 3.2 Slide Materialization (`ppt/slides/slideN.xml`)
Each slide carries resolved dimensions/transforms (EMU), background (solid / gradient / image), a z-ordered `Vec<SlideElement>`, and:
- `notes: Option<String>`: Plain text from `ppt/notesSlides/notesSlideN.xml` for speaker notes.
- `comments: Vec<PptxComment>`: Threaded reviewer comments associated with the slide.
- `hidden: bool`: Authoring flag (`<p:sld show="0">`) indicating the slide is skipped during linear presentation shows.
- `parse_error: Option<String>`: Upstream partial degradation (RB7) marker: if a single slide's XML part is malformed, other slides load normally while this slide carries the error detail for placeholder rendering.

**Group flattening**: `<p:grpSp>` groups are flattened by the parser into child elements with composed transforms, under a fail-closed nesting-depth budget. The model has **no group variant**; the renderer handles exactly one transform level per element ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §3.1).

---

## 4. In-Memory Data Model (`pptx-model`) — verified shape

### 4.1 Slide Element Types
```rust
pub enum SlideElement {
    Shape(ShapeElement),
    Picture(PictureElement),
    Table(TableElement),
    Chart(ChartElement),   // carries a full ChartModel (classic + ChartEx data)
    Media(MediaElement),
}
```

### 4.2 Shapes & Transforms (`ShapeElement`)
- **Transform**: `x, y, width, height: i64` (EMU); `rotation: f64` (degrees, parser-converted from 60,000ths); `flip_h`, `flip_v`.
- **Geometry**:
  - `geometry: String` — preset name (e.g. `rect`, `roundRect`) or `"custGeom"`.
  - `cust_geom: Vec<Vec<PathCmd>>` — custom paths, coordinates **normalized to [0,1]**; `cust_geom_paint: Option<Vec<PathPaint>>` — per-path fill/stroke policy.
  - Adjustment values `adj`…`adj8` (OOXML units 0–100000) for preset parametrization.
- **`PathCmd` variants** (exact, verified):
  ```rust
  enum PathCmd {
      MoveTo { x, y },  LineTo { x, y },
      CubicBezTo { x1, y1, x2, y2, x, y },
      QuadBezTo { x1, y1, x, y },
      ArcTo { wr, hr, st_ang, sw_ang },   // degrees
      Close,
  }
  ```
  *(The earlier revision of this document listed `QuadTo`/`CubicTo` and omitted `ArcTo`.)*
- **Styling**:
  - `fill: Fill` — `Solid { color: hex }`, `Gradient { stops, angle_deg, kind }`, `None` (plus further fill kinds as carried; classified in the [fidelity matrix](./02d-element-renderers-and-fidelity-policy.md) §2).
  - `stroke: Stroke` — `color` (hex), `width` (EMU), `dash_style` (`prstDash` name) or explicit dash segments (lengths as stroke-width multipliers), optional gradient line fill.
  - Text bodies per §4.3.

### 4.3 Typography & Text Bodies (`TextBody`)
- `vertical_anchor`, `wrap`, `alignment` (paragraph-level), EMU insets (`l_ins`/`r_ins` 91440, `t_ins`/`b_ins` 45720 defaults).
- `auto_fit` (`noAutofit` | `normAutofit` | `spAutoFit`) **plus PowerPoint's stored `font_scale` and line-spacing reduction** — applying the stored values is correct-by-construction for PowerPoint-saved decks ([PRD 02c](./02c-typography-and-text-layout.md) §6).
- Paragraphs: margins/indent (EMU, negative indent = hanging bullet), `space_before/after` (hundredths of a point or %), line spacing; bullets `None | Char | AutoNum{format, startAt} | Blip` with **independent size/color/typeface groups** (the parser resolves marker, size, and color groups independently — renderers must preserve that).
- Runs (`TextRunData`): text, family, size (pt), **three-state** `bold/italic` (`Option<bool>` — `None` = inherit), underline + style + color, strikethrough, color/opacity.
- WordArt warp (`TextWarp { preset, adj }`, 40 presets) and equation fallback previews (`math_fallback`) are carried when authored.

### 4.4 Media & Pictures (`PictureElement`)
Relationship id → `ppt/media/` asset; crop boundaries (`srcRect` percentages); blip effect stack (subset renderable — see fidelity matrix §2.2).

### 4.5 Presentation-Level Defaults
`default_text_color`, `major_font`/`minor_font`, hyperlink color — theme-baked, consumed by the style and text engines.

---

## 5. Media & Asset Management

The `AssetStore` interface, `DecodedImage`, `SizeHint`, and behavioral rules are **normative in [PRD 06](./06-component-interface-contracts.md) §5**. This section records the integration decisions:

1. **Lazy decode**: `ppt/media/*` parts decode on first use (PNG, JPEG, GIF first-frame, WebP, BMP, TIFF via the `image` crate).
2. **Decoded LRU cache**: default 64 MB (`[assets] cache_mb`); `SizeHint::AtMost` prevents oversized decodes for small on-screen targets.
3. **Vector media**:
   - **SVG**: rasterized via `resvg` at the hinted size. Where PowerPoint bundled a PNG fallback relationship for the SVG, the fallback PNG takes precedence (it is the author's own rasterization).
   - **EMF/WMF**: **no credible pure-Rust decoder exists**; `resvg` does not handle metafile formats. The store returns `AssetError::UnsupportedFormat`, and the renderer draws the [placeholder](./02d-element-renderers-and-fidelity-policy.md) §3 (using the bundled PNG fallback when one exists). *(Corrects the earlier revision's claim that resvg covers these formats.)*
4. **Ownership**: the store retains the archive bytes; parsed media references stay valid for the session.

---

## 6. Resource Limits & Error Handling

OOXML files can be malformed, zip-bombed, or excessively complex. Upstream's guards are inherited:

| Risk | Guard Mechanism | Default Limit |
| :--- | :--- | :--- |
| **Zip bomb / decompression inflation** | Max uncompressed part size | 64 MB per XML part, 256 MB total |
| **Deep XML nesting** | Max recursion depth | 256 levels |
| **Deep group nesting** | Fail-closed depth budget at the native package boundary | upstream limit |
| **DOM complexity** | Total element node threshold | 200,000 nodes |
| **Corrupted media** | Fail-soft placeholder ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3) | — |
| **Unsupported shapes** | Tier-3 placeholder + warning | — |

All limit breaches surface as typed `ParseError`/`ResourceLimitHit` values ([PRD 06](./06-component-interface-contracts.md) §3–§4) — never panics.

---

## 7. Diagnostics

Two channels, both fed from the same session-scoped event sink ([PRD 06](./06-component-interface-contracts.md) §4):

1. **Structured warnings** (`WarningEvent` stream): font substitutions, unsupported presets, media failures, fidelity degradations, resource-limit hits. Consumed live by the TUI status area ([PRD 04](./04-interactive-tui-presenter.md) §6), appended by `dump --json`, counted in `info` output.
2. **`--debug-parser` mode**:
   - Dumps the resolved `Presentation` AST as pretty-printed JSON (`schemaVersion`-stamped).
   - Logs relationship-graph resolution and asset-cache statistics to stderr.
   - Timing breakdown: parse, first-render, first-present (feeds the startup NFR harness, [PRD 07](./07-quality-and-testing.md) §5).
