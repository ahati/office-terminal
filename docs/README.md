# kitty-pptx — Product Requirements & Architecture Specifications

Welcome to the design documentation and Product Requirements Documents (PRDs) for **`kitty-pptx`**, a modern Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks directly inside terminal emulators using the **Kitty Graphics Protocol** and compatible graphics standards.

---

## Reading Order

1. **[00-system-overview.md](./00-system-overview.md)** — Vision, target personas, component architecture, Cargo workspace, NFRs, non-goals.
2. **[06-component-interface-contracts.md](./06-component-interface-contracts.md)** — *Foundational Seam Contracts*: `RgbaSlide`, `RenderTargetSpec`, trait definitions, error taxonomy, threading model. Read before the engine PRDs.
3. **[01-parser-and-data-model.md](./01-parser-and-data-model.md)** — Upstream integration, verified data model, non-seekable stream spooling, asset store, diagnostics.
4. **The PRD 02 Composition Engine Split**:
   - **[02a](./02a-composition-core-and-render-pipeline.md)** — Composition Core & Render Pipeline (viewport mapping, caching, prefetch priority queue, cooperative worker cancellation).
   - **[02b](./02b-vector-style-and-geometry-engine.md)** — Vector Style & Geometry Engine (color parsing, gradients, custom paths, stroke inset alignment).
   - **[02b Appendix](./02b-appendix-preset-geometries.md)** — Comprehensive Parametric Geometry Catalog (T1 shape math, T2 stand-in recipes).
   - **[02c](./02c-typography-and-text-layout.md)** — Typography & Text Layout (font registry, substitution matrix, cosmic-text $\to$ tiny-skia rasterization bridge, bullet symbol translation).
   - **[02d](./02d-element-renderers-and-fidelity-policy.md)** — Element Renderers & Fidelity Policy (normative FULL/DEGRADED/PLACEHOLDER/OMITTED matrix, unified placeholder language).
5. **The PRD 03 Display Adapter Split**:
   - **[03a](./03a-kitty-graphics-protocol-driver.md)** — Primary Kitty Reference Driver (protocol syntax, chunking, Unicode placeholder grid `U=1`, stage/present lifecycle, 8-slot ID pool).
   - **[03b](./03b-display-fallbacks-and-probing.md)** — Fallback Drivers & Capability Probing (probe state machine, input preservation, multiplexer DCS passthrough, Sixel quantization, iTerm2, Half-block).
6. **[04-interactive-tui-presenter.md](./04-interactive-tui-presenter.md)** — Interactive Presenter TUI (modal state machine, Ratatui custom slide widget, speaker notes, stopwatch, sorter grid, external media playback).
7. **[05-cli-and-export.md](./05-cli-and-export.md)** — CLI Subcommands, slide range expression parser, headless streaming export, Unix stdin piping, configuration.
8. **[07-quality-and-testing.md](./07-quality-and-testing.md)** — Test levels L1–L5, synthetic & robustness corpora, DSSIM perceptual diff scoring, automated NFR measurement harnesses.

---

## Architecture at a Glance

Two engines — **Composition** (slides $\to$ pixels) and **Display** (pixels $\to$ terminal) — joined by exactly two shared types. Composition never touches the terminal; Display never interprets slides; `export` builds without display code at all. All contracts are normative in [PRD 06](./06-component-interface-contracts.md).

```
                        ┌───────────────────────────────┐
                        │      kitty-pptx Binary        │
                        │  clap · ratatui · crossterm   │
                        └───────────────┬───────────────┘
                                        │
        ┌──────────────────────┬────────┴─────────────┬─────────────────────┐
        ▼                      ▼                      ▼                     ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐    ┌───────────────┐
│  Parser &     │      │  Composition  │      │  Render       │    │   Display     │
│  Data Model   │      │  Engine       │      │  Pipeline     │    │   Adapter     │
│  pptx-parser  │      │  tiny-skia    │      │  cache·prefetch│   │  kitty·sixel· │
│  pptx-model   │      │  cosmic-text  │      │  stage/present │   │  iterm2·half- │
│  (vendored)   │      │  resvg        │      │  scheduler     │   │  block        │
└───────────────┘      └───────┬───────┘      └───────┬───────┘    └───────┬───────┘
                               │  RgbaSlide ◄─────────┼───────────────────┘
                               │  RenderTargetSpec ►──┘   (the only seam types)
                               ▼
                        ┌───────────────┐
                        │  RGBA frames  │
                        │  1920×1080    │
                        └───────────────┘
```

---

## Core Technology Stack

| Concern | Choice | Rationale |
| :--- | :--- | :--- |
| PPTX parsing | vendored `pptx-parser` / `pptx-model` (MIT) | Pure-Rust OPC archive reading and PresentationML deserialization |
| Rasterization | `tiny-skia` | Pure-Rust, memory-safe, thread-safe, battle-tested in `resvg` |
| Typography | `cosmic-text` (`fontdb` + `rustybuzz` + `bidi`) | Advanced shaping, CJK/RTL handling, bundled Carlito/Caladea fonts |
| Vector media | `resvg` | SVG rasterization directly to pixel buffers |
| Raster media | `image` crate | Pure-Rust decoding for PNG, JPEG, WebP, GIF, BMP, TIFF |
| Terminal UI | `ratatui` + `crossterm` | Flexible widget tree, custom placeholder graphics widgets |
| CLI Parser | `clap` | Subcommand derivation, flag validation, range parsing |
| Concurrency | Worker pool + Priority Prefetch Queue | Multithreaded rendering off UI thread; instant $O(1)$ flips |

---

## Implementation Roadmap (Dependency-Ordered)

1. **M0 — Contract Spike**: Vendor parser crates; load real deck; dump AST; compose background + title; stage/present via Kitty driver; verify scrollback persistence.
2. **M1 — Seam Contracts Landed**: `RgbaSlide`, `RenderTargetSpec`, driver and pipeline traits with L3 protocol golden tests ([PRD 07](./07-quality-and-testing.md) §2). Interfaces frozen.
3. **M2 — Composition Core**: Compositor, viewport aspect mapping, T1 geometry generators, solid/gradient fills, text layout with cosmic-text $\to$ tiny-skia bridge, pictures, composition goldens.
4. **M3 — Fidelity Breadth & Honesty**: T2 stand-ins, tables, bullets & symbol mappings, autofit, fidelity matrix wired with standard placeholders and warning events; headless export subcommand.
5. **M4 — Presenter TUI**: Layouts, modal state machine, speaker notes, stopwatch, sorter grid with thumbnail target; prefetch pipeline meeting $< 16\text{ ms}$ flip latency.
6. **M5 — Fallback Drivers**: Sixel, iTerm2, half-block, capability probe state machine with input preservation, multiplexer DCS passthrough, degraded-mode UX.
7. **M6 — Quality Regime Complete**: Fidelity corpus scoring with DSSIM, fuzz integration, automated NFR measurement harnesses, live-terminal verification matrix.
