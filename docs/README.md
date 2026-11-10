# kitty-pptx — Product Requirements & Architecture Specifications

Welcome to the design documentation and Product Requirements Documents (PRDs) for **`kitty-pptx`**, a modern Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks directly inside terminal emulators using the **Kitty Graphics Protocol** and compatible graphics standards, with headless vector SVG and high-resolution PNG batch export.

---

## Reading Order

1. **[00-system-overview.md](./00-system-overview.md)** — Vision, target personas, two-stage vector architecture, Cargo workspace, NFRs, non-goals.
2. **[06-component-interface-contracts.md](./06-component-interface-contracts.md)** — *Foundational Seam Contracts*: `VectorSlide` display list, `RgbaSlide`, `RenderTargetSpec`, trait definitions, error taxonomy, threading model. Read before the engine PRDs.
3. **[01-parser-and-data-model.md](./01-parser-and-data-model.md)** — Upstream integration, verified data model, non-seekable stream spooling, asset store, diagnostics.
4. **The PRD 02 Composition Core Split**:
   - **[02a](./02a-composition-core-and-render-pipeline.md)** — Composition Core & Render Pipeline (`SlideCompiler` $\to$ `VectorSlide`, two-tier caching, priority prefetch queue, cooperative worker cancellation).
   - **[02b](./02b-vector-style-and-geometry-engine.md)** — Vector Style & Geometry Engine (color parsing, gradients, custom paths, stroke inset alignment, `VectorPath`).
   - **[02b Appendix](./02b-appendix-preset-geometries.md)** — Comprehensive Parametric Geometry Catalog (T1 shape math, T2 stand-in recipes).
   - **[02c](./02c-typography-and-text-layout.md)** — Typography & Text Layout (font registry, substitution matrix, cosmic-text $\to$ `PositionedGlyphRun`, Swash rasterization bridge, SVG text modes).
   - **[02d](./02d-element-renderers-and-fidelity-policy.md)** — Element Compilers & Fidelity Policy (normative FULL/DEGRADED/PLACEHOLDER/OMITTED matrix, unified placeholder language).
5. **The PRD 03 Display Adapter Split**:
   - **[03a](./03a-kitty-graphics-protocol-driver.md)** — Primary Kitty Reference Driver (protocol syntax, chunking, Unicode placeholder grid `U=1`, stage/present lifecycle, 8-slot ID pool).
   - **[03b](./03b-display-fallbacks-and-probing.md)** — Fallback Drivers & Capability Probing (probe state machine, input preservation, multiplexer DCS passthrough, Sixel quantization, iTerm2, Half-block).
6. **[04-interactive-tui-presenter.md](./04-interactive-tui-presenter.md)** — Interactive Presenter TUI (modal state machine, Ratatui custom slide widget, speaker notes, stopwatch, sorter grid, external media playback).
7. **[05-cli-and-export.md](./05-cli-and-export.md)** — CLI Subcommands, slide range expression parser, dual-backend headless export (PNG & Vector SVG), Unix stdin piping, configuration.
8. **[07-quality-and-testing.md](./07-quality-and-testing.md)** — Test levels L1–L5 (including L2a Vector Scene Goldens and L2b Composition Goldens), DSSIM perceptual diff scoring, automated NFR measurement protocols.
9. **[08-shape-effects-and-3d-engine.md](./08-shape-effects-and-3d-engine.md)** — Shape Effects & 3D Engine (DrawingML `<a:effectLst>`, shadows, glow, soft edges, reflections, 3D bevel/scene lighting, separable Gaussian box blurs, and standalone SVG filters).
10. **[09-omml-equation-typesetting.md](./09-omml-equation-typesetting.md)** — Live OMML Equation Typesetting Engine (ECMA-376 §22.1 `<m:oMath>`, TeX Appendix G box-and-glue layout, OpenType MATH table metrics, STIX Two Math fallback, dual-backend vector/raster execution).
11. **[10-interactive-hyperlinks-and-terminal-seam.md](./10-interactive-hyperlinks-and-terminal-seam.md)** — Interactive Hyperlinks & Terminal Interactivity Seam (three interactivity seams: terminal OSC 8 hyperlinks wrapping Kitty Unicode placeholders, Ratatui TUI Link Picker modal ('o') with non-blocking browser dispatch, and clickable native vector SVG exports).
12. **[11-extended-and-tier3-shape-presets.md](./11-extended-and-tier3-shape-presets.md)** — Extended & Tier 3 Shape Presets (systematic expansion across 180+ ECMA-376 DrawingML presets, parametric multi-corner rectangles, callouts with target pointers, extended curved/bent/notched arrows, brackets/braces, seals/stars 4..32, action buttons with vector glyph icons, and modular compiler submodules under 600 lines).

---

## Architecture at a Glance

A two-stage pipeline: **DrawingML $\to$ Resolution-Independent Vector Display List (`VectorSlide`) $\to$ Pluggable Backends**.

```
                        ┌───────────────────────────────┐
                        │      kitty-pptx Binary        │
                        │  clap · ratatui · crossterm   │
                        └───────────────┬───────────────┘
                                        │
        ┌──────────────────────┬────────┴─────────────┬─────────────────────┐
        ▼                      ▼                      ▼                     ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐    ┌───────────────┐
│  Parser &     │      │ SlideCompiler │      │  Render       │    │   Display     │
│  Data Model   │      │ (Geometry &   │      │  Pipeline     │    │   Adapter     │
│  pptx-parser  │      │ Typography)   │      │  Vector &     │    │  kitty·sixel· │
│  pptx-model   │      │               │      │  Raster Cache │    │  iterm2·half- │
│  (vendored)   │      │               │      │  scheduler    │    │  block        │
└───────────────┘      └───────┬───────┘      └───────┬───────┘    └───────┬───────┘
                               │ VectorSlide          │                    │
                ┌──────────────┴──────────────┐       │                    │
                ▼                             ▼       │                    │
         ┌──────────────┐              ┌──────────────┤                    │
         │ tiny-skia    │              │ Vector SVG   │                    │
         │ Rasterizer   │              │ Serializer   │                    │
         └──────┬───────┘              └──────┬───────┘                    │
                │  RgbaSlide (Pixels) ◄───────┼────────────────────────────┘
                │  RenderTargetSpec ►─────────┘
                ▼
         ┌──────────────┐
         │  RGBA frames │
         │  1920×1080   │
         └──────────────┘
```

---

## Core Technology Stack

| Concern | Choice | Rationale |
| :--- | :--- | :--- |
| PPTX parsing | vendored `pptx-parser` / `pptx-model` (MIT) | Pure-Rust OPC archive reading and PresentationML deserialization |
| Vector Display List | `VectorSlide` | Resolution-independent intermediate representation shared by all backends |
| Rasterization | `tiny-skia` | Pure-Rust, memory-safe, thread-safe, battle-tested in `resvg` |
| Typography | `cosmic-text` (`fontdb` + `rustybuzz` + `bidi`) | Advanced shaping, CJK/RTL handling, bundled Carlito/Caladea fonts |
| Vector Export | Native SVG XML serializer | Clean, resolution-independent SVG output directly from `VectorSlide` |
| Vector Media | `resvg` | SVG rasterization directly to pixel buffers |
| Raster Media | `image` crate | Pure-Rust decoding for PNG, JPEG, WebP, GIF, BMP, TIFF |
| Terminal UI | `ratatui` + `crossterm` | Flexible widget tree, custom placeholder graphics widgets |
| CLI Parser | `clap` | Subcommand derivation, flag validation, range parsing |
| Concurrency | Worker pool + Priority Prefetch Queue | Multithreaded rendering off UI thread; instant $O(1)$ flips |

---

## Implementation Roadmap (Dependency-Ordered)

1. **M0 — Contract Spike**: Vendor parser crates; load real deck; dump AST; compile simple `VectorSlide`; rasterize & stage/present via Kitty driver; verify scrollback persistence.
2. **M1 — Seam Contracts Landed**: `VectorSlide`, `RgbaSlide`, `RenderTargetSpec`, driver and pipeline traits with L3 protocol golden tests ([PRD 07](./07-quality-and-testing.md) §2). Interfaces frozen.
3. **M2 — Vector Compilation Core**: SlideCompiler, viewport aspect mapping, T1 geometry generators, solid/gradient fills, text layout with cosmic-text shaping into `PositionedGlyphRun`, pictures, L2a scene goldens.
4. **M3 — Backends & Export**: `tiny-skia` rasterizer backend + native SVG vector serializer; headless export subcommand (`--format png,svg`); fidelity matrix wired with standard placeholders.
5. **M4 — Presenter TUI**: Layouts, modal state machine, speaker notes, stopwatch, sorter grid with thumbnail target; prefetch pipeline meeting $< 16\text{ ms}$ flip latency.
6. **M5 — Fallback Drivers**: Sixel, iTerm2, half-block, capability probe state machine with input preservation, multiplexer DCS passthrough, degraded-mode UX.
7. **M6 — Quality Regime Complete**: Fidelity corpus scoring with DSSIM, fuzz integration, automated NFR measurement harnesses, live-terminal verification matrix.
8. **Post-v1 Tracks**: Full interactive chart rendering; OMML equation layout; native PDF vector export backend (`pdf-writer`). (Shape effects & 3D engine specified in [PRD 08](./08-shape-effects-and-3d-engine.md)).
