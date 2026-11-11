# PRD 00: System Overview & Architecture

## 1. Product Vision

`kitty-pptx` is a fast, standalone Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks with high visual fidelity directly inside modern terminal emulators using the **Kitty Graphics Protocol** and compatible standards.

By pairing the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer) with a **resolution-independent vector display list (`VectorSlide`)**, a headless 2D vector rasterizer (`tiny-skia`), advanced text shaping (`cosmic-text`), and terminal UI primitives (`ratatui`), `kitty-pptx` turns the terminal into a first-class presentation environment for developers, remote SSH presenters, and command-line power users while enabling clean vector SVG and high-resolution PNG batch export.

**Fidelity Posture**: `kitty-pptx` is a *viewer with an honest contract* — what renders fully, what renders approximately, what renders as a labeled placeholder, and what is omitted is explicitly classified, testable, and visible to the user ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md)).

---

## 2. Target User Personas & Use Cases

1. **The Terminal-Centric Developer / Engineer**:
   - Inspects presentations received in chat or downloaded from email without launching heavyweight GUI office suites.
   - Runs `kitty-pptx deck.pptx` or previews inline: `kitty-pptx view deck.pptx -n 3`.
2. **The Conference / Meetup Presenter**:
   - Presents directly from a lightweight terminal emulator (Kitty, Ghostty, WezTerm).
   - Utilizes the Presenter Console: speaker notes, elapsed timer, next-slide thumbnail, seamless keyboard navigation.
3. **The Remote SSH Worker**:
   - Presents over remote SSH sessions where X11/Wayland forwarding is sluggish or unavailable.
   - Kitty graphics pass through SSH natively; slides render at native speed.
4. **Automation & AI Pipelines**:
   - Batch-extracts vector SVG or high-resolution PNG slide images (`export`), Markdown, or JSON ASTs (`dump`) for LLM summarization, indexing, and visual diffing.

---

## 3. Two-Stage Vector Pipeline & Architecture

The architecture decouples layout resolution from backend output using an intermediate 2D display list ([`VectorSlide`](./06-component-interface-contracts.md) §2.1):

```
┌─ APPLICATION LAYER ────────────────────────────────────────────────────────┐
│ B1 CLI Shell (clap, mode dispatch)           B2 Config Loader              │
│ B17 TUI Presenter (ratatui)                  B18 Export (PNG & Vector SVG) │
│ B19 Dump/Extract AST                         B20 Info Metadata             │
└──────────────┬─────────────────────────────────────────────────────────────┘
               │ consumes
┌─ PIPELINE ───┴─────────────────────────────────────────────────────────────┐
│ B11 Render Pipeline/Scheduler — VectorSlide cache, raster cache,           │
│     priority prefetch, staging coordination, eviction                      │
└──────┬────────────────────────────────────────────┬────────────────────────┘
       │                                            │
┌─ COMPOSITION CORE ────────────────────────┐  ┌─ DISPLAY ENGINE ────────────┐
│ B10 Slide Compiler (DrawingML -> Vector)  │  │ B13 Geometry Negotiation    │
│     B6 Style Resolver · B7 Geometry Engine│  │ B14 Capability Probe        │
│     B8 Text Layout · B9 Element Compilers │  │ B15 Protocol Drivers        │
│ B5  Asset Store                           │  │     (Kitty graphics only)   │
│ B3  Parser Facade ──► B4 pptx-model       │  │                             │
└─────────────────────┬─────────────────────┘  │ B16 Image Lifecycle Manager │
                      │ VectorSlide            └──────────────┬──────────────┘
       ┌──────────────┴──────────────┐                        ▲
       ▼                             ▼                        │
┌──────────────┐              ┌──────────────┐                │
│ tiny-skia    │              │ Vector SVG   │                │
│ Rasterizer   │              │ Serializer   │                │
└──────┬───────┘              └──────┬───────┘                │
       │ RgbaSlide (Pixels)          │ slide.svg              │
       └─────────────────────────────┼────────────────────────┘
                       hard seam: RgbaSlide + RenderTargetSpec
```

### 3.1 Cargo Workspace Crate Structure
- `crates/pptx-model` (vendored AST)
- `crates/pptx-parser` (vendored OOXML parser)
- `crates/ooxml-common` (vendored relationships & XML limits)
- `crates/kitty-pptx-core`: Slide compiler, `VectorSlide` display list, style/geometry/text services, asset store, `tiny-skia` rasterizer, SVG serializer, and pipeline scheduler. Zero display/terminal dependencies.
- `crates/kitty-pptx-display`: Kitty graphics driver, capability probe, geometry negotiation, and `TtyWriter`.
- `crates/kitty-pptx-tui`: Ratatui slide widgets, presenter console, sorter grid, and input event loop.
- `crates/kitty-pptx-cli`: Clap CLI shell, subcommand dispatch, config loader.

### 3.2 Component ↔ PRD Traceability

| Block | Component | Specified in |
| :--- | :--- | :--- |
| B1, B2 | CLI Shell, Config Loader | [PRD 05](./05-cli-and-export.md) |
| B3, B4 | Parser Facade, Data Model | [PRD 01](./01-parser-and-data-model.md) |
| B5 | Asset Store | PRD 01 §6, [PRD 06](./06-component-interface-contracts.md) §5.1 |
| B6, B7 | Style Resolver, Geometry Engine | [PRD 02b](./02b-vector-style-and-geometry-engine.md), [PRD 02b App](./02b-appendix-preset-geometries.md) |
| B8 | Text Layout Engine | [PRD 02c](./02c-typography-and-text-layout.md) |
| B9 | Element Compilers & Fidelity Policy | [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) |
| B10, B11 | Slide Compiler, `VectorSlide`, Pipeline | [PRD 02a](./02a-composition-core-and-render-pipeline.md) |
| B12 | Seam Contracts (`VectorSlide`, `RgbaSlide`, `RenderTargetSpec`) | [PRD 06](./06-component-interface-contracts.md) §2 |
| B15, B16 | Kitty Reference Driver, Image Lifecycle | [PRD 03a](./03a-kitty-graphics-protocol-driver.md) |
| B13, B14 | Geometry Negotiation and Kitty Capability Probe | [PRD 03a](./03a-kitty-graphics-protocol-driver.md) |
| B17 | Interactive Presenter TUI | [PRD 04](./04-interactive-tui-presenter.md) |
| B18, B19, B20 | Export (PNG & SVG), Dump, Info | [PRD 05](./05-cli-and-export.md) |
| — | Error Taxonomy, Threading, TtyWriter | [PRD 06](./06-component-interface-contracts.md) |
| — | Quality Regime & Testing Protocols | [PRD 07](./07-quality-and-testing.md) |

---

## 4. Non-Functional Requirements (NFRs)

Every target is backed by an automated measurement harness in [PRD 07](./07-quality-and-testing.md) §5:

| Metric | Target | Measurement Method |
| :--- | :--- | :--- |
| **Startup to 1st slide** | $< 120\text{ ms}$ (p95, warm cache) | Automated timing harness on `typical-20.pptx` |
| **Slide flip latency** | $< 16\text{ ms}$ (p99, pre-staged) | Keypress event poll read $\to$ `present()` escape write complete |
| **Slide compilation** | $< 8\text{ ms}$ simple / $< 20\text{ ms}$ complex | Slide AST $\to$ `VectorSlide` display list |
| **Replay rasterization**| $< 7\text{ ms}$ 1080p / $< 0.8\text{ ms}$ thumb | `VectorSlide` $\to$ `RgbaSlide` at target resolution |
| **Peak Memory RSS** | $< 150\text{ MB}$ peak RSS | Full 50-slide walkthrough + sorter grid |
| **Stripped Binary Size**| $< 25\text{ MB}$ stripped release | Linux x86_64 target (includes bundled fonts) |
| **Robustness** | Zero panics across entire test corpus | Robustness and fuzz corpus gating |

---

## 5. Non-Goals (v1)

Explicitly out of scope:
- **Presentation Editing**: View-only product.
- **Full Chart Rendering**: Data-rich placeholder only ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §4.3).
- **SmartArt Diagram Layout**: Frame and title placeholder only.
- **Live OMML Equation Layout**: Authoring-time preview images render; live OMML layout deferred.
- **Embedded Audio/Video Playback**: In-terminal playback is out of scope; launches external player (`mpv`) via `M`.
- **Slide Transitions & Animations**: Static slide renders only.
- **Shape Effects**: Drop shadow, glow, 3-D bevels omitted.
- **Legacy `.ppt` / `.odp` Formats**: Typed error (exit 2).
