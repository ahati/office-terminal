# PRD 00: System Overview & Architecture

## 1. Product Vision
`kitty-pptx` is a fast, standalone Rust CLI and TUI presentation viewer that renders PowerPoint (`.pptx`) decks with high visual fidelity directly inside modern terminal emulators using the **Kitty Graphics Protocol** (and compatible protocols).

By leveraging the pure-Rust parsing engine from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer), a headless 2D vector rasterizer (`tiny-skia`), modern text shaping (`cosmic-text`), and terminal UI primitives (`ratatui`), `kitty-pptx` turns the terminal into a first-class presentation environment for developers, remote SSH presenters, and command-line power users.

**Fidelity posture**: `kitty-pptx` is a *viewer with an honest contract* — what renders fully, what renders approximately, what renders as a labeled placeholder, and what is omitted is specified, testable, and visible to the user ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md)). It does not claim pixel-perfect equivalence with PowerPoint.

---

## 2. Target User Personas & Use Cases

1. **The Terminal-Centric Developer / Engineer**:
   - Inspects slides received in chat or downloaded from email without leaving the terminal or launching heavy GUI suites.
   - Runs `kitty-pptx deck.pptx`, or previews inline: `kitty-pptx view deck.pptx -n 3`.
2. **The Conference / Meetup Presenter**:
   - Presents from a lightweight terminal window (Kitty, WezTerm, Ghostty).
   - Uses the Presenter View: speaker notes, elapsed timer, next-slide thumbnail, seamless keyboard navigation.
3. **The Remote SSH Worker**:
   - Works over SSH where forwarding X11/Wayland is slow or unavailable.
   - The Kitty graphics protocol passes through SSH natively; slides render at native terminal speed.
4. **Automation & AI Pipelines**:
   - Extracts high-resolution slide thumbnails (`export`), text outlines and Markdown (`dump`) for indexing, visual diffing, or multimodal LLM input.

---

## 3. Component Architecture

Two engines — **Composition** (slides → pixels) and **Display** (pixels → terminal) — joined by exactly two shared types (`RgbaSlide`, `RenderTargetSpec`; [PRD 06](./06-component-interface-contracts.md) §2). Composition never touches the terminal; Display never interprets slides. `export` links without any display code — the enforced proof of the seam.

```
┌─ APPLICATION LAYER ─────────────────────────────────────────────────┐
│ B1 CLI Shell (clap, mode dispatch)      B2 Config Loader            │
│ B17 TUI Presenter (ratatui)             B18 Export Pipeline         │
│ B19 Dump/Extract    B20 Info Metadata                                 │
└──────────────┬───────────────────────────────────────────────────────┘
               │ consumes
┌─ PIPELINE ───────────────────────────────────────────────────────────┐
│ B11 Render Pipeline/Scheduler — cache window, prefetch, staging      │
│     coordination, invalidation, eviction (the only orchestrator)     │
└──────┬──────────────────────────────────────┬────────────────────────┘
       │                                      │
┌─ COMPOSITION ENGINE ────────────────┐  ┌─ DISPLAY ENGINE ────────────┐
│ B10 Slide Compositor (orchestrator) │  │ B13 Geometry Negotiation    │
│ B6  Style Resolver                  │  │ B14 Capability Probe        │
│ B7  Geometry Engine                 │  │ B15 Protocol Drivers        │
│ B8  Text Layout Engine              │  │    (kitty / sixel / iterm2 /│
│ B9  Element Renderers +             │  │     halfblock)              │
│     Fidelity Policy                 │  │ B16 Image Lifecycle Manager │
│ B5  Asset Store                     │  │    (stage/present/release,  │
│ B3  Parser Facade → B4 pptx-model   │  │     pool, cleanup)          │
└─────────────────────────────────────┘  └─────────────────────────────┘
        │                  ▲                       ▲
        └──────────────────┼───────────────────────┘
              hard contract: RgbaSlide + RenderTargetSpec (B12)
```

### 3.1 Component ↔ PRD Traceability

| Block | Component | Specified in |
| :--- | :--- | :--- |
| B1, B2 | CLI shell, config | [PRD 05](./05-cli-and-export.md) |
| B3, B4 | Parser facade, data model | [PRD 01](./01-parser-and-data-model.md) |
| B5 | Asset store | PRD 01 §5, [PRD 06](./06-component-interface-contracts.md) §5 |
| B6, B7 | Style resolver, geometry engine | [PRD 02b](./02b-vector-style-and-geometry-engine.md) |
| B8 | Text layout engine | [PRD 02c](./02c-typography-and-text-layout.md) |
| B9 | Element renderers & fidelity policy | [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) |
| B10, B11 | Slide compositor, render pipeline | [PRD 02a](./02a-composition-core-and-render-pipeline.md) |
| B12 | Seam contracts (RgbaSlide, RenderTargetSpec) | [PRD 06](./06-component-interface-contracts.md) §2 |
| B13, B14, B15, B16 | Geometry negotiation, probe, drivers, lifecycle | [PRD 03](./03-terminal-graphics-protocol.md) |
| B17 | TUI presenter | [PRD 04](./04-interactive-tui-presenter.md) |
| B18, B19, B20 | Export, dump, info | [PRD 05](./05-cli-and-export.md) |
| — | Error taxonomy, threading model, TtyWriter | [PRD 06](./06-component-interface-contracts.md) §8–§10 |
| — | Testing, measurement protocols | [PRD 07](./07-quality-and-testing.md) |

---

## 4. Key Architectural Decisions

### 4.1 Parser Reuse (vendored, pinned)
Vendored crates `pptx-parser` / `pptx-model` (paths, features, native entry points — all verified upstream; see PRD 01 §2). The upstream repo is fast-moving and single-maintainer; integration therefore uses **pin + numbered patch series + deliberate re-sync with full golden-corpus gating** (PRD 01 §2.2).

### 4.2 Rasterization: `tiny-skia` + `cosmic-text`
`tiny-skia` (pure Rust, thread-safe, production-proven in `resvg`) for vector output; `cosmic-text` (`fontdb` + `rustybuzz` + `bidi`) for typography; metric-compatible bundled fonts (Carlito/Caladea) so default decks lay out correctly even on fontless servers (PRD 02c §2).

### 4.3 Composition/Display Separation (the load-bearing decision)
One frame type and one target-spec type cross the seam (PRD 06 §2). All benefits follow from this: export builds display-free, drivers are testable against pure encoders, resize never re-renders, and the flip-latency contract (stage in idle time, present in O(1)) is structural rather than best-effort.

### 4.4 Graphics Output: Kitty Protocol Primary, Pluggable Fallbacks
Kitty protocol as the reference driver; Sixel, iTerm2, and Unicode half-blocks behind one trait with explicit capability flags and failure semantics (PRD 03 §6, §8, §10).

### 4.5 Fidelity Policy as a First-Class Contract
Rendering scope is governed by the normative fidelity matrix (FULL/DEGRADED/PLACEHOLDER/OMITTED) — charts, SmartArt, equations, and effects are explicit v1 placeholders, not surprises (PRD 02d §2).

### 4.6 Platform Scope
Linux, macOS, WSL2 — pure TTY/POSIX I/O, zero X11/Wayland dependencies. Native Windows is a non-goal for v1 (§7).

---

## 5. Non-Functional Requirements

Every target below has a defined measurement protocol and fixture in [PRD 07](./07-quality-and-testing.md) §5; a target without a protocol is not an NFR.

| Metric | Target | Measured by |
| :--- | :--- | :--- |
| **Startup to first slide** | < 120 ms (median & p95, `typical-20` deck, warm cache, 4-core reference class) | PRD 07 §5 |
| **Slide flip latency** | < 16 ms p99 (staged path: keypress → `present` written) | PRD 07 §5 |
| **Slide render latency** | < 15 ms simple / < 40 ms complex (p95) | PRD 07 §5 |
| **Memory footprint** | < 150 MB peak RSS across a full walkthrough (≈41.5 MB 5-frame window + 64 MB asset LRU + decoder headroom) | PRD 07 §5 |
| **Binary size** | < 25 MB stripped release (includes bundled fonts) | PRD 07 §5 |
| **Cross-platform** | Linux, macOS, WSL2 | manual matrix, PRD 07 §7 |
| **Robustness** | Zero panics on the entire test corpus (incl. fuzz-derived robustness decks) | PRD 07 §3.3 |

---

## 6. Document Directory Map

| PRD | Title |
| :--- | :--- |
| [01](./01-parser-and-data-model.md) | Core Parser & Data Model Integration |
| [02a](./02a-composition-core-and-render-pipeline.md) | Composition Core & Render Pipeline |
| [02b](./02b-vector-style-and-geometry-engine.md) | Vector Style & Geometry Engine |
| [02c](./02c-typography-and-text-layout.md) | Typography & Text Layout Engine |
| [02d](./02d-element-renderers-and-fidelity-policy.md) | Element Renderers & Fidelity Policy |
| [03](./03-terminal-graphics-protocol.md) | Terminal Graphics Protocol & Display Adapter |
| [04](./04-interactive-tui-presenter.md) | Interactive TUI & Presenter Mode |
| [05](./05-cli-and-export.md) | CLI Interface, Export & Integration |
| [06](./06-component-interface-contracts.md) | Component Interface Contracts *(foundational — read second, after this document)* |
| [07](./07-quality-and-testing.md) | Quality & Testing |

---

## 7. Non-Goals (v1)

Explicitly out of scope — each of these is a deliberate, documented decision, not an oversight:

- **Editing** of any kind (view-only product; aligned with upstream's scope).
- **Chart rendering** — data-rich placeholder only (PRD 02d §2.4); full chart rendering is a post-v1 track with its own PRD.
- **SmartArt diagram rendering** — placeholder only.
- **OMML equation typesetting** — authoring-time fallback previews render; live equation layout is out of scope.
- **Embedded audio/video playback** — poster placeholders only.
- **Slide transitions and animations** — static rendering only.
- **Shape effects** (shadow, glow, reflection, bevel, 3-D) — omitted in v1.
- **Legacy `.ppt` / `.odp` input; password-protected decks** — typed error, exit 2.
- **SVG export** — removed (would require a second, vector renderer).
- **Native Windows** — POSIX platforms only for v1.
- **Editing-adjacent features** (comments, tracked changes) — not applicable to viewing.
