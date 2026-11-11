# PRD 07: Quality & Testing

## 1. Purpose & Core Testing Invariants

`kitty-pptx` guarantees both fidelity and speed. To ensure these qualities are verifiable and non-regressive, every non-functional requirement (NFR) is backed by an automated measurement protocol.

> **Testing Seam Invariants**:
> - **L2a (Vector Scene)** tests verify the `VectorSlide` display list AST structurally against committed JSON snapshots.
> - **L2b (Composition)** tests verify bit-identical `RgbaSlide` pixel outputs without touching a terminal.
> - **L3 (Protocol)** tests verify exact byte output received by `TtyWriter` without requiring a physical terminal emulator.
> All merge-blocking CI gates run hermetically in headless containers.

---

## 2. Test Hierarchy (L1–L5)

| Level | Scope | Execution Framework | Merge Gate |
| :--- | :--- | :--- | :--- |
| **L1 — Unit** | Math, geometry equations, dash arrays, color parsing, chunk slicing | Standard `cargo test` | Blocking |
| **L2a — Vector Scene Goldens** | `VectorSlide` display list AST structural snapshot comparison | JSON AST snapshots | Blocking |
| **L2b — Composition Goldens** | Full-slide `RgbaSlide` rendered frames hashed/compared against committed PNG references | Pinned font & Skia baseline runner | Blocking |
| **L3 — Protocol Snapshots** | Kitty escape sequence unit snapshots (`a=t` chunks, `a=p` placement, `d=a` exits) | Driver unit tests | Blocking |
| **L4 — Live Terminal Smoke** | Verification on Kitty-protocol terminals (Kitty, Ghostty, WezTerm, Alacritty Kitty-graphics forks): probe queries, scrollback survival, resize repositioning | Scripted PTY integration test suite | Release-blocking |
| **L5 — CLI End-to-End** | CLI subcommands (`export --format png,svg`, `dump`, `info`, `view`), stdin piping, exit codes | Integration test suite executing binary | Blocking |

---

## 3. Test Fixture Corpora

### 3.1 Synthetic Composition Corpus (`tests/corpus/composition/`)
Generated deterministically via `tests/corpus/generate.rs`:
1. **Geometry Suite**: One test slide per T1 preset shape ([PRD 02b App](./02b-appendix-preset-geometries.md) §1) testing default and boundary `adj` values.
2. **Typography Suite**: Rich-run styling, hanging indents, autonumbering sequences, CJK fallback glyphs, Wingdings/Symbol bullet mappings.
3. **Fidelity Suite**: Dedicated slide for each DEGRADED and PLACEHOLDER matrix entry ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2).
4. **Boundary Cases**: Zero-width shapes, off-slide elements, single-pixel lines, overlapping z-orders.

### 3.2 Fidelity Corpus (`tests/corpus/fidelity/`)
Generated fixtures (PptxGenJS) compared against upstream browser reference
renders. A real-world deck may be added only when its license and attribution
are recorded in the corpus manifest.

### 3.3 Robustness & Fuzz Corpus (`tests/corpus/robustness/`)
- Seeded from upstream fuzz targets (`fuzz/fuzz_targets/fuzz_parse.rs`).
- Includes zip-bombs, truncated XML parts, 300-level nested groups, password-protected files, 500-slide decks, and slides with 1,000 shapes.
- **Panic Rule**: A Rust panic on any corpus deck is an immediate release blocker.

---

## 4. Perceptual Diff Metric & Golden Image Scoring

For real-world fidelity scoring, `kitty-pptx` evaluates Structural Dissimilarity (DSSIM):
$$\text{DSSIM}(x, y) = \frac{1.0 - \text{SSIM}(x, y)}{2.0}$$

- **PASS**: $\text{DSSIM} \le 0.02$ (visually indistinguishable at presentation distance).
- **REVIEW**: $0.02 < \text{DSSIM} \le 0.06$ (minor font substitution or antialiasing differences).
- **FAIL**: $\text{DSSIM} > 0.06$. If $> 10\%$ of corpus slides fail, the build is blocked.

### Determinism Environment
1. Pinned Rust compiler toolchain and committed `Cargo.lock`.
2. Bundled fonts compiled into binary (Carlito & Caladea).
3. Linux container pinned to Debian Bookworm with fixed fallback font packages (`fonts-dejavu-core`, `fonts-noto-cjk`).
4. Strict fixed render targets (`1920×1080` or `320×180`).

---

## 5. NFR Measurement Protocols

Every numeric NFR target from [PRD 00](./00-system-overview.md) §4 is verified by an automated protocol:

| Metric | Target | Measurement Method | Fixture Deck | Pass Threshold |
| :--- | :--- | :--- | :--- | :--- |
| **Startup to 1st Slide** | $< 120\text{ ms}$ | `execve` $\to$ first `present()` write complete; cold/warm cache | `typical-20.pptx` (20 slides, images, tables) | p95 $< 120\text{ ms}$ ($n=20$) |
| **Slide Flip Latency** | $< 16\text{ ms}$ | Keypress event poll read $\to$ `present()` escape write complete | Pre-staged `typical-20.pptx` slides | p99 $< 16\text{ ms}$ ($n=200$) |
| **Slide Compilation** | $< 8\text{ ms}$ / $< 20\text{ ms}$ | Slide AST $\to$ `VectorSlide` display list | Simple deck / Complex deck | p95 $< 8\text{ ms}$ (simple), $< 20\text{ ms}$ (complex) |
| **Replay Rasterization**| $< 7\text{ ms}$ / $< 0.8\text{ ms}$ | `VectorSlide` $\to$ `RgbaSlide` | 1080p slide / 320×180 thumbnail | p95 $< 7\text{ ms}$ (1080p), $< 0.8\text{ ms}$ (thumb) |
| **Peak Memory RSS** | $< 150\text{ MB}$ | `getrusage(RUSAGE_SELF).ru_maxrss` during full 50-slide walkthrough + sorter grid | `heavy-media-50.pptx` | Peak RSS $< 150\text{ MB}$ |
| **Stripped Binary Size**| $< 25\text{ MB}$ | `stat -c %s target/release/kitty-pptx` (stripped) | Linux `x86_64-unknown-linux-gnu` target | Size $< 25,000,000\text{ bytes}$ |

---

## 6. Pre-Release Manual Test Matrix

| Environment | TUI Presentation | Split Presenter View | Sorter Thumbnails | `view` Scrollback | Resize Scaling |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Kitty-protocol terminal** | Pass | Pass | Blank (direct placement) | Pass | Pass (re-render) |
| **No Kitty graphics** | Fail with `NoCapableTerminal` | Fail with `NoCapableTerminal` | — | Fail with `NoCapableTerminal` | — |
