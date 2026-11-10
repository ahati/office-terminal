# kitty-pptx — Product Requirements & Architecture Specifications

Welcome to the design documentation and Product Requirements Documents (PRDs) for **`kitty-pptx`**, a modern Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks directly inside the terminal using the **Kitty Graphics Protocol** and compatible terminal graphics standards.

---

## Document Index

| Document | Title | Description |
| :--- | :--- | :--- |
| **[00-system-overview.md](./00-system-overview.md)** | **System Overview & Architecture** | Executive summary, target personas, non-functional requirements, component diagram, and high-level architecture. |
| **[01-parser-and-data-model.md](./01-parser-and-data-model.md)** | **Core Parser & Data Model** | Upstream integration with `yukiyokotani/office-open-xml-viewer`, OPC/ZIP container handling, AST data structures, and media caching. |
| **[02-slide-rendering-engine.md](./02-slide-rendering-engine.md)** | **Slide Rendering Engine** | Pure-Rust 2D rasterizer using `tiny-skia`, DrawingML shapes, theme color math, typography with `cosmic-text`, and image compositing. |
| **[03-terminal-graphics-protocol.md](./03-terminal-graphics-protocol.md)** | **Terminal Graphics Protocol & Display Adapter** | Kitty Graphics Protocol escape sequences, base64 chunking, terminal geometry negotiation, and fallback drivers (Sixel, iTerm2, half-blocks). |
| **[04-interactive-tui-presenter.md](./04-interactive-tui-presenter.md)** | **Interactive TUI & Presenter Mode** | Presenter interface built on `ratatui`, speaker notes drawer, elapsed timer, visual slide sorter grid, speculative pre-rendering, and keybindings. |
| **[05-cli-and-export.md](./05-cli-and-export.md)** | **CLI Interface, Export & Integration** | `clap` CLI flags, one-shot inline pager (`view`), headless batch exporter (`export`), Markdown/notes dump (`dump`), config file, and Unix piping. |

---

## Core Technology Stack

```
                        ┌───────────────────────────────┐
                        │      kitty-pptx Binary        │
                        └───────────────┬───────────────┘
                                        │
        ┌──────────────────────┬────────┴──────────────┬──────────────────────┐
        ▼                      ▼                       ▼                      ▼
┌───────────────┐      ┌───────────────┐       ┌───────────────┐      ┌───────────────┐
│     clap      │      │    ratatui    │       │   pptx-model  │      │   tiny-skia   │
│ CLI Arguments │      │ TUI Presenter │       │  pptx-parser  │      │ 2D Rasterizer │
│ Configuration │      │   crossterm   │       │ (from silurus)│      │  cosmic-text  │
└───────────────┘      └───────┬───────┘       └───────────────┘      └───────┬───────┘
                               │                                              │
                               ▼                                              ▼
                        ┌───────────────┐                             ┌───────────────┐
                        │ Terminal Keys │                             │  RGBA Pixmap  │
                        │ & Event Loop  │                             │ 1920x1080 RAM │
                        └───────────────┘                             └───────┬───────┘
                                                                              │
                                                                              ▼
                                                                      ┌───────────────┐
                                                                      │ Kitty Protocol│
                                                                      │ Base64 Escapes│
                                                                      └───────────────┘
```

---

## Implementation Roadmap (Milestones)

1. **Milestone 1 — Parser Harness & Image Display (Week 1)**:
   - Vendor `pptx-parser` and `pptx-model` as native Rust dependencies.
   - Implement basic Kitty protocol escape writer (`\x1b_G...`).
   - Extract slide background images and verify display in Kitty / Ghostty / WezTerm.
2. **Milestone 2 — Vector & Shape Rasterizer (Week 2)**:
   - Implement `tiny-skia` canvas pipeline.
   - Add preset geometries (`rect`, `roundRect`, `ellipse`, arrows) and custom `PathCmd` paths.
   - Implement DrawingML theme color math (tint, shade, luminance modulations).
3. **Milestone 3 — Typography & Text Body (Week 3)**:
   - Integrate `cosmic-text` and `fontdb`.
   - Implement multi-run styled paragraphs, word wrapping, alignment, and bullet points.
4. **Milestone 4 — TUI Presenter & Navigation (Week 4)**:
   - Build `ratatui` presenter layout: slide viewport, speaker notes drawer, elapsed timer.
   - Implement keyboard navigation and background sliding-window pre-rendering.
5. **Milestone 5 — CLI Tools, Sorter & Fallbacks (Week 5)**:
   - Add `view` (inline), `export` (PNG batch), `dump` (Markdown/notes) subcommands.
   - Implement visual thumbnail slide sorter.
   - Implement Sixel and Unicode half-block fallbacks for non-Kitty terminals.
