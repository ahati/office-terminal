# PRD 00: System Overview & Architecture

## 1. Product Vision
`kitty-pptx` (working title) is a blazing-fast, standalone Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks with high visual fidelity directly inside modern terminal emulators using the **Kitty Graphics Protocol** (and compatible protocols).

By leveraging the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer), a headless 2D vector rasterizer (`tiny-skia`), modern text shaping (`cosmic-text`), and terminal UI primitives (`ratatui`), `kitty-pptx` transforms the terminal into a first-class presentation environment for developers, remote SSH presenters, and command-line power users.

---

## 2. Target User Personas & Use Cases

1. **The Terminal-Centric Developer / Engineer**:
   - Wants to quickly inspect slides received in chat or downloaded from email without leaving the terminal or launching heavy GUI suites (PowerPoint, LibreOffice, Keynote).
   - Runs `pptx-view deck.pptx` or previews a specific slide inline like `pptx-view deck.pptx -n 3`.
2. **The Conference / Meetup Presenter**:
   - Presents slides directly from a lightweight terminal window (Kitty, WezTerm, Ghostty).
   - Needs a "Presenter View" with speaker notes, an elapsed presentation timer, and seamless keyboard navigation.
3. **The Remote SSH Worker**:
   - Works over SSH on remote workstations where forwarding X11/Wayland is slow or unavailable.
   - Kitty graphics over SSH works natively and delivers crisp slides at native terminal speed.
4. **Automation & AI Pipelines**:
   - Needs to extract high-resolution slide thumbnails, text outlines, or Markdown representations of decks for indexing, visual diffing, or feeding into multimodal LLMs.

---

## 3. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                CLI Entry Point                              │
│             (clap: argument parsing, config resolution, subcommands)        │
└──────────────────────┬────────────────────────────────┬─────────────────────┘
                       │                                │
          [Interactive TUI Mode]              [One-Shot Inline / Export]
                       │                                │
                       ▼                                ▼
       ┌───────────────────────────────┐  ┌──────────────────────────────────┐
       │   Presenter TUI (Ratatui)     │  │ Inline Pager / Batch Exporter    │
       │ - Slide Canvas                │  │ - Dump to terminal scrollback    │
       │ - Presenter Notes Drawer      │  │ - Batch export PNG / SVG / MD    │
       │ - Outline & Thumbnail Sorter  │  │                                  │
       │ - Elapsed Timer & Status Bar  │  │                                  │
       └───────────────┬───────────────┘  └─────────────────┬────────────────┘
                       │                                    │
                       └─────────────────┬──────────────────┘
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Presentation Session & Pipeline                     │
│  - Slide Cache & Pre-rendering Worker (N-1, N, N+1 async pipeline)          │
│  - Asset Store (extracted images / BLIPs in memory)                         │
└──────────────────────┬────────────────────────────────┬─────────────────────┘
                       │                                │
                       ▼                                ▼
┌──────────────────────────────────────────┐ ┌────────────────────────────────┐
│   OOXML Parser (from office-open-xml)    │ │   2D Slide Rasterizer Engine   │
│ - OPC / ZIP Container Reader             │ │ - tiny-skia 2D Vector Canvas   │
│ - PresentationML & DrawingML Parsing     │ │ - cosmic-text + fontdb shaping │
│ - Master, Layout, & Theme Inheritance    │ │ - Theme color tint/shade math  │
│ - pptx-model intermediate AST            │ │ - Preset & custom PathCmds     │
└──────────────────────────────────────────┘ └────────────────┬───────────────┘
                                                              │
                                                              ▼ RGBA Pixmap
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Terminal Graphics & Protocol Adapter                     │
│ - Geometry negotiation (TIOCGWINSZ: cell cols/rows to pixel dimensions)     │
│ - Kitty Graphics Protocol Serializer (chunked base64 escape sequences)      │
│ - Optional Protocol Fallbacks (Sixel, iTerm2, half-block ASCII)             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Key Architectural Decisions

### 4.1 Parser Reuse vs. Fork
- **Decision**: Re-export and vendor the pure-Rust crates `packages/pptx/parser` and `packages/pptx/model` from `yukiyokotani/office-open-xml-viewer`.
- **Rationale**: OOXML PresentationML is exceptionally complex (ECMA-376 spec). Yukiyokotani’s parser is already written in modern, audited Rust with strict resource safety limits and complete master/layout inheritance logic. Compiling it with `default-features = false` decouples it from WebAssembly and exposes native typed Rust methods (`parse_pptx_model_native`).

### 4.2 Rasterization: `tiny-skia` + `cosmic-text`
- **Decision**: Render slides headlessly directly to an in-memory RGBA buffer using `tiny-skia` for vector drawing and `cosmic-text` for text shaping.
- **Rationale**: 
  - `tiny-skia` is pure Rust, zero-C, thread-safe, and used in production by `resvg`.
  - `cosmic-text` combines `fontdb` (system font discovery), `rustybuzz` (HarfBuzz port in Rust), and `bidi` for true cross-platform typography.

### 4.3 Graphics Output: Primary Kitty Protocol with Extensible Fallbacks
- **Decision**: Implement the Kitty Graphics Protocol natively as the primary high-fidelity driver, while structuring the display layer behind an abstract `TerminalRenderer` trait to support Sixel and iTerm2 protocols.

---

## 5. Non-Functional Requirements & Performance Goals

| Metric | Target | Rationale |
| :--- | :--- | :--- |
| **Startup to First Slide** | `< 120 ms` (typical 20-slide deck) | Must feel instant for CLI one-shot previews. |
| **Slide Flip Latency** | `< 16 ms` (60 FPS feel) | Pre-rendering neighboring slides (N-1, N+1) guarantees zero perceptual lag when advancing slides. |
| **Memory Footprint** | `< 150 MB` peak RSS | Suitable for low-spec developer laptops and memory-constrained servers. |
| **Binary Size** | `< 25 MB` stripped release build | Easily distributable via `cargo install`, Homebrew, Pacman, or Debian `.deb`. |
| **Cross-Platform** | Linux, macOS, WSL2 | Zero X11/Wayland dependencies; pure TTY / POSIX I/O. |

---

## 6. Document Directory Map

The specifications for `kitty-pptx` are organized into the following PRD modules:

- **[01-parser-and-data-model.md](./01-parser-and-data-model.md)**: Upstream integration, OOXML extraction, memory model, and media caching.
- **[02-slide-rendering-engine.md](./02-slide-rendering-engine.md)**: Vector canvas, DrawingML geometry, theme color math, typography, and image compositing.
- **[03-terminal-graphics-protocol.md](./03-terminal-graphics-protocol.md)**: Kitty Graphics protocol mechanics, placement, image ID caching, and terminal negotiation.
- **[04-interactive-tui-presenter.md](./04-interactive-tui-presenter.md)**: Presenter interface (Ratatui), speaker notes, timer, thumbnails, and keyboard control.
- **[05-cli-and-export.md](./05-cli-and-export.md)**: CLI subcommands, one-shot inline viewing, batch PNG/SVG export, and configuration files.
