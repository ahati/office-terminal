# AGENTS.md

Welcome to the **kitty-pptx** codebase! This document serves as the primary orientation guide, architectural reference, and engineering handbook for autonomous agents, pair programmers, and human contributors working on this repository.

---

## 1. Project Introduction & Vision

**kitty-pptx** is a high-fidelity, high-performance terminal slide deck presenter and vector export engine for Microsoft PowerPoint (`.pptx`) presentations. It brings modern presentation rendering directly to the terminal while supporting native vector exports (such as SVG and PNG).

### Key Capabilities
- **Kitty Graphics Protocol Native**: Uses the Kitty graphics protocol (PNG transmission, z-indexing, and direct placement) for crisp, GPU-accelerated terminal graphics. Terminals without Kitty graphics support are rejected at startup; there is no fallback renderer.
- **Vector-First Two-Stage Architecture**: Converts OOXML / PowerPoint shapes, text runs, and styling into an intermediate, resolution-independent vector display list (`VectorSlide`). This enables infinite scalability, native SVG export with selectable text, and pixel-perfect rasterization at arbitrary DPI/cell dimensions.
- **Dual-Mode Interactive TUI**: Built with [Ratatui](https://ratatui.rs), featuring:
  - **Normal View**: Full-bleed slide rendering centered to terminal dimensions.
  - **Presenter View**: Split console with current slide, next slide preview, timer / elapsed clock, speaker notes, and interactive thumbnail navigator.

---

## 2. Workspace & Crate Architecture

The repository is organized as a Cargo workspace with clean separation of concerns:

```
kitty-pptx/
├── Cargo.toml                      # Workspace manifest & shared dependencies
├── AGENTS.md                       # This guide
├── docs/                           # Detailed PRDs and architectural specifications
│   ├── 00-system-overview.md
│   ├── 01-parser-and-data-model.md
│   ├── 02a-composition-core-and-render-pipeline.md
│   ├── 02b-vector-style-and-geometry-engine.md
│   ├── 02b-appendix-preset-geometries.md
│   ├── 02c-typography-and-text-layout.md
│   ├── 02d-element-renderers-and-fidelity-policy.md
│   ├── 03a-kitty-graphics-protocol-driver.md
│   ├── 04-interactive-tui-presenter.md
│   ├── 05-cli-and-export.md
│   ├── 06-component-interface-contracts.md
│   └── 07-quality-and-testing.md
└── crates/
    ├── office-terminal-core/            # Vector IR, compiler, rasterizer, SVG exporter
    ├── office-terminal-display/         # Terminal graphics drivers, probe, cell mapper
    ├── office-terminal-pdf/             # PDF document engine (PageSource adapter)
    ├── office-terminal-docx/            # Word document engine (rdocx-backed PageSource)
    ├── office-terminal-tui/             # Interactive Ratatui presenter application
    ├── office-terminal-cli/             # Command-line interface & subcommands
    ├── pptx-parser/                # (Vendored) Low-level OOXML PPTX parser
    ├── pptx-model/                 # (Vendored) Abstract PowerPoint DOM model
    ├── ooxml-common/               # (Vendored) OOXML package, relationship & XML utilities
    └── patches/                    # Reproducible patches for vendored crates
```

### Crate Responsibilities

| Crate | Role & Key Components |
| :--- | :--- |
| **`office-terminal-core`** | **Display List & Render Core**: Defines `VectorSlide`, `DrawCommand`, `GeometryPath`, `Paint`, and `TextRun`. Compiles `pptx-model` slides into vector display lists. Rasterizes via `tiny-skia` into RGBA framebuffers. Exports native standalone SVG. Manages embedded media asset extraction (`AssetStore`). |
| **`office-terminal-display`** | **Kitty Graphics Driver**: Implements the `TerminalGraphicsDriver` trait for the Kitty graphics protocol, probes for Kitty support via environment variables and the active graphics query, computes cell-to-pixel aspect ratios, and handles cleanup. |
| **`office-terminal-pdf`** | **PDF Document Engine**: Parses and renders PDF input through a `PageSource` adapter (native raster path, vector bridge groundwork), page labels, outlines and the reading-order search corpus. |
| **`office-terminal-docx`** | **Word Document Engine**: Paginates WordprocessingML packages through the pinned `rdocx`/`oxml-layout` backend behind a `DocxEngine` trait, bridges positioned page frames into `VectorSlide` with exact-face glyph painting, media/metafile routing, search corpus and hyperlinks. |
| **`office-terminal-tui`** | **Interactive Presenter**: Ratatui-based TUI state machine (`PresenterApp`, `PresenterMode`). Handles keyboard navigation (Space/Enter/Arrows, 'p' presenter toggle, 't' thumbnail grid, 'b' blank screen). Manages slide timer, notes, and aspect-ratio-preserving layout widgets. |
| **`office-terminal-cli`** | **CLI Entrypoint**: Implements commands via `clap`: `view`, `present`, `export` (SVG/PNG), `dump` (JSON/AST), and `info`. Loads configuration and coordinates pipeline execution. |
| **Vendored Crates** | **Upstream Parser Stack**: `pptx-parser`, `pptx-model`, `ooxml-common`. Kept pristine in `crates/` to provide robust, spec-compliant PPTX parsing without network dependencies. Any essential fixes must be tracked in `crates/patches/` and recorded in `crates/VENDORED.md`. |

---

## 3. Strict Coding Conventions & Quality Rules

All agents and contributors must strictly follow these engineering conventions:

### A. Strict File Size Limit (< 600 Lines)
- **Hard Limit**: **NO source file may exceed 600 lines**.
- **Proactive Modularization**: When a file approaches 400–500 lines, split it into logically coherent submodules.
  - *Example*: Large shape preset tables are split into `compiler/shapes/presets_t1.rs`, `presets_t2.rs`, etc.
  - *Example*: CLI commands are partitioned into `commands/view.rs`, `commands/export.rs`, `commands/present.rs`, etc.
  - *Example*: TUI views are split into `app/ui/normal.rs`, `app/ui/presenter.rs`, `app/ui/thumbnails.rs`.

### B. Two-Stage Vector Pipeline Discipline
- **Vector-First Rule**: Never render directly from `pptx-model` to pixels.
- All presentation elements must compile to the intermediate representation (`VectorSlide` and `DrawCommand`):
  1. **Stage 1 (Compile)**: `pptx-model::Slide` + `AssetStore` $\rightarrow$ `VectorSlide`.
  2. **Stage 2 (Render/Export)**:
     - For Terminal/PNG: `VectorSlide` $\rightarrow$ `tiny-skia` $\rightarrow$ `RgbaImage`.
     - For SVG: `VectorSlide` $\rightarrow$ standalone `<svg>` XML with CSS styles and selectable text runs.
- This ensures feature parity, testability, and resolution independence across all target outputs.

### C. Terminal Protocol Hygiene & Clean Restoration
- Terminal applications must leave the user's terminal clean and functional under all circumstances (normal exit, error exit, panic, SIGINT/Ctrl-C, SIGTERM).
- Always install panic hooks and signal handlers that:
  - Disable raw mode (`crossterm::terminal::disable_raw_mode()`).
  - Show the cursor (`crossterm::cursor::Show`).
  - Leave alternate screen buffer (`crossterm::terminal::LeaveAlternateScreen`).
  - Send graphics cleanup sequences (e.g. Kitty clear: `\x1b_Ga=d,d=a,q=2\x1b\\`).

### D. Safety and Robustness
- **Forbid Unsafe**: Library code should adhere to `#![forbid(unsafe_code)]` unless interacting with low-level termios/ioctl for cell dimension probing, which must be strictly encapsulated, audited, and isolated in `office-terminal-display`.
- **Robust Error Handling**:
  - Library crates use typed errors with `thiserror` (e.g. `RenderError`, `DriverError`).
  - Avoid `.unwrap()` and `.expect()` in non-test production code. Bubble errors using `?` or provide graceful fallback defaults.
  - Handle malformed or partially corrupt PPTX files gracefully (e.g. render placeholders for missing shapes/images instead of crashing the presenter).

### E. Vendored Crates Policy & Licensing
- The crates `pptx-parser`, `pptx-model`, and `ooxml-common` are vendored from upstream.
- **Do not make ad-hoc edits** to files in these directories unless creating a documented patch in `crates/patches/`.
- Locally patched crates.io dependencies (`[patch.crates-io]`): the `metafile-*` playback crates and the DOCX layout engine `rdocx-layout` (`crates/rdocx-layout`, carrying `crates/patches/0009`). They are pristine package copies plus numbered patches, excluded from workspace lints/tests, and must be dropped from the tree and the `[patch]` section as soon as an upstream release carries the fix.
- Pinned dependency note: `zip = "=9.0.4"` is required in workspace `Cargo.toml` to guarantee compatibility with `ooxml-common` NTFS extra-field parsing tests.
- **Licensing Notice**: This project is licensed under the **MIT License**. All upstream and vendored components carry their own respective licenses and copyright notices from their upstream authors, as tracked in `crates/VENDORED.md`.

### F. Strict Zero-Warning Policy (Compiler & Clippy)
- **Zero Warnings Enforcement**: The entire workspace must compile cleanly with **zero compiler warnings and zero Clippy warnings**.
- Always verify with:
  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  ```
- No commit or PR should ever introduce dead code warnings, unused import warnings, or clippy lints. Address lints proactively or use narrow, documented `#[allow(...)]` attributes only when technically justified.

### G. SOLID Principles for Design
- **Follow SOLID principles for design**:
  - **Single Responsibility**: Each module, struct, and function must have a single, well-defined responsibility (e.g. separate geometry calculation from styling from rasterization).
  - **Open/Closed**: Code should be open for extension but closed for modification through clear traits (`TerminalGraphicsDriver`, `SlideRasterizer`, `SlideCompiler`).
  - **Liskov Substitution**: Driver and renderer implementations must be interchangeable through their traits without violating contract invariants.
  - **Interface Segregation**: Keep interfaces small, cohesive, and targeted (e.g. `StageSink`, `AssetStore`).
  - **Dependency Inversion**: High-level modules must depend on abstractions and traits rather than low-level concrete implementations.

---

## 4. Coordinate Systems & Mathematical Conventions

When implementing compilers, geometry calculations, or drivers, adhere to these domain constants:

- **English Metric Units (EMU)**:
  - PowerPoint coordinates and dimensions are expressed in integer EMUs.
  - $1 \text{ inch} = 914,400 \text{ EMUs}$.
  - $1 \text{ cm} = 360,000 \text{ EMUs}$.
  - $1 \text{ pt} = 12,700 \text{ EMUs}$.
  - Standard 96 DPI pixel: $1 \text{ px} = 9,525 \text{ EMUs}$.
- **Angles in OOXML**:
  - Expressed in $60,000\text{ths}$ of a degree ($360^\circ = 21,600,000\text{ units}$).
- **Terminal Cell Aspect Ratio**:
  - Monospace terminal character cells are rectangular, typically having a 1:2 width-to-height ratio (aspect factor $\approx 0.5$).
  - Layout algorithms must compensate using cell aspect ratios when mapping slide bounding boxes (`EmuRect`) to terminal character cells (`CellRect`).
- **Kitty Direct Placement**:
  - Images are transmitted as PNG (`f=100`) and presented with `a=p,c=<cols>,r=<rows>,z=-1`.
  - Unicode placeholder cells (`U=1`, `U+10EEEE`) are not used.

---

## 5. Development & Testing Workflow

### Essential Commands

```bash
# Check compilation across all crates
cargo check --workspace

# Lint entire workspace with zero warnings permitted
cargo clippy --workspace --all-targets -- -D warnings

# Run complete test suite (must pass 100%)
cargo test --workspace

# Check line counts to enforce the < 600 line limit
find crates/office-terminal-* -name "*.rs" | xargs wc -l | sort -nr

# Run the CLI
cargo run -p office-terminal-cli -- view presentation.pptx
cargo run -p office-terminal-cli -- export presentation.pptx --format svg -o out/
```

- `external_libreoffice_renderer_comparison` is cache-gated, not `#[ignore]`: with `target/external/` populated it renders 418 decks / 543 slides on every `cargo test`.
- After changing `COMPARE_WIDTH` or re-rendering references, refresh the width-tagged baseline with `UPDATE_EXTERNAL_SCORES=1 cargo test -p office-terminal-core --test external_fixture_tests -- --nocapture`.

### Git Commit Guidelines & User Approval
- **Mandatory User Approval**: **Always request and receive explicit approval from the user before committing changes**, presenting the exact proposed commit message and changed files for review.
- Use clear, conventional commit prefixes:
  - `feat:` New features or subcommands.
  - `fix:` Bug fixes or correction of rendering artifacts.
  - `refactor:` Modular domain splits or non-breaking architectural restructuring.
  - `test:` Additional tests or test fixtures.
  - `docs:` Updates to documentation or specs.
- Always run `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before proposing a commit.

---

## 6. Subagent Collaboration Guidelines

When working in multi-agent swarms or subagent delegations:
1. **Assign Clear Domain Ownership**:
   - Agent 1: Vector Pipeline & Shape Compilers (`office-terminal-core`)
   - Agent 2: Terminal Display Drivers & Probing (`office-terminal-display`)
   - Agent 3: Presenter TUI State & Layout (`office-terminal-tui`)
   - Agent 4: CLI Interface & Subcommands (`office-terminal-cli`)
2. **Respect Component Contracts**: Contracts in `crates/office-terminal-core/src/contracts/` are shared ground. If modifying a contract data structure, coordinate with all dependent crates.
3. **Verify Line Counts Proactively**: After adding or expanding features, verify line counts with `wc -l` and split files before marking assignments complete.
