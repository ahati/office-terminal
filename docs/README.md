# kitty-pptx — Product Requirements & Architecture Specifications

Welcome to the design documentation and Product Requirements Documents (PRDs) for **`kitty-pptx`**, a modern Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks directly inside the terminal using the **Kitty Graphics Protocol** and compatible terminal graphics standards.

---

## Reading Order

1. **[00-system-overview.md](./00-system-overview.md)** — vision, personas, component architecture, NFRs, non-goals.
2. **[06-component-interface-contracts.md](./06-component-interface-contracts.md)** — *foundational*: the seam contracts (`RgbaSlide`, `RenderTargetSpec`), trait definitions, error taxonomy, threading model. Read before the engine PRDs.
3. **[01-parser-and-data-model.md](./01-parser-and-data-model.md)** — upstream integration, verified data model, asset store, diagnostics.
4. **PRD 02 split** — the composition engine:
   - **[02a](./02a-composition-core-and-render-pipeline.md)** — Composition Core & Render Pipeline (orchestration, caching, prefetch/staging)
   - **[02b](./02b-vector-style-and-geometry-engine.md)** — Vector Style & Geometry (colors, preset shapes, custom paths)
   - **[02c](./02c-typography-and-text-layout.md)** — Typography & Text Layout (fonts, substitution, bullets, autofit)
   - **[02d](./02d-element-renderers-and-fidelity-policy.md)** — Element Renderers & Fidelity Policy (the normative FULL/DEGRADED/PLACEHOLDER/OMITTED matrix)
5. **[03-terminal-graphics-protocol.md](./03-terminal-graphics-protocol.md)** — Display Adapter: Kitty protocol, stage/present lifecycle, scrollback placement, fallback drivers.
6. **[04-interactive-tui-presenter.md](./04-interactive-tui-presenter.md)** — Presenter TUI as a pipeline consumer.
7. **[05-cli-and-export.md](./05-cli-and-export.md)** — CLI subcommands, headless export, config.
8. **[07-quality-and-testing.md](./07-quality-and-testing.md)** — test levels, golden corpus, NFR measurement protocols.

## Architecture at a Glance

Two engines — **Composition** (slides → pixels) and **Display** (pixels → terminal) — joined by exactly two shared types. Composition never touches the terminal; Display never interprets slides; `export` builds without display code at all. All contracts are normative in [PRD 06](./06-component-interface-contracts.md).

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

## Core Technology Stack

| Concern | Choice |
| :--- | :--- |
| PPTX parsing | vendored `pptx-parser` / `pptx-model` (from `yukiyokotani/office-open-xml-viewer`, MIT) |
| Rasterization | `tiny-skia` (pure Rust, thread-safe) |
| Typography | `cosmic-text` (`fontdb` + `rustybuzz` + `bidi`), bundled Carlito/Caladea |
| Vector media | `resvg` (SVG rasterization) |
| Raster media | `image` crate |
| TUI | `ratatui` + `crossterm` |
| CLI | `clap` |
| Concurrency | worker pool (pipeline scheduler), stage/present terminal lifecycle |

## Implementation Roadmap (dependency-ordered, not calendar-ordered)

The milestones below are sequenced strictly by dependency and risk retirement. No calendar duration is implied.

1. **M0 — Contract spike**: vendor crates; parse a real deck; dump AST; compose background + title; stage/present via the Kitty driver; verify scrollback placement survives scrolling. *Retires the highest-risk assumptions (parser integration, protocol lifecycle) before breadth is built.*
2. **M1 — Seam contracts landed**: `RgbaSlide`, `RenderTargetSpec`, driver and pipeline traits with L3 protocol golden tests ([PRD 07](./07-quality-and-testing.md) §2). *Everything after this point implements against frozen interfaces.*
3. **M2 — Composition core**: compositor, transform pipeline, T1 geometry, solid/gradient fills, text layout with substitution, pictures; composition golden harness running on the synthetic corpus.
4. **M3 — Fidelity breadth & honesty**: T2 stand-ins, tables, bullets/autonumbering, autofit, fidelity matrix fully wired with placeholders and warning events; export subcommand (display-free build invariant enforced).
5. **M4 — Presenter TUI**: layouts, navigation, notes, timer, sorter with thumbnail target; stage/present prefetch pipeline meeting the < 16 ms flip contract.
6. **M5 — Fallback drivers**: Sixel, iTerm2, half-block, capability probing with timeouts, failure semantics, degraded-mode UX.
7. **M6 — Quality regime complete**: fidelity corpus + perceptual scoring, fuzz integration, NFR measurement harnesses, live-terminal matrix; release readiness per PRD 07 §6.
8. **Post-v1 tracks** (each with its own PRD when started): full chart rendering; shape effects; OMML equation layout; WordArt warps; vector export.
