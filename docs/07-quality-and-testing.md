# PRD 07: Quality & Testing

## 1. Purpose & Core Testing Invariants

`kitty-pptx` guarantees both fidelity and speed. To ensure these qualities are verifiable and non-regressive, every non-functional requirement (NFR) is backed by an automated measurement protocol.

> **Testing Seam Invariant**:
> Tests at Levels L2 and L3 execute strictly against the **contracts of [PRD 06](./06-component-interface-contracts.md)**:
> - **L2 (Composition)** tests verify bit-identical `RgbaSlide` pixel outputs without touching a terminal.
> - **L3 (Protocol)** tests verify exact byte output received by `TtyWriter` without requiring a physical terminal emulator.
> This ensures that all CI gates run hermetically in headless containers.

---

## 2. Test Hierarchy (L1–L5)

| Level | Scope | Execution Framework | Merge Gate |
| :--- | :--- | :--- | :--- |
| **L1 — Unit** | Math, geometry equations, dash arrays, color parsing, chunk slicing | Standard `cargo test` | Blocking |
| **L2 — Composition Goldens** | Full-slide `RgbaSlide` rendered frames hashed/compared against committed references | Custom test runner with pinned fonts & Cairo/Skia baseline | Blocking |
| **L3 — Protocol Goldens** | Byte-exact escape sequence snapshots per driver (`U=1` blocks, Sixel bands, `d=a` exits) | In-memory `TtyWriter` byte inspection | Blocking |
| **L4 — Live Terminal Smoke** | Verification on physical terminals (Kitty, Ghostty, WezTerm, Foot): probe queries, scrollback survival, resize repositioning | Scripted PTY integration test suite | Release-blocking |
| **L5 — CLI End-to-End** | CLI subcommands (`export`, `dump`, `info`, `view`), stdin piping, exit codes | Integration test suite executing binary | Blocking |

---

## 3. Test Fixture Corpora

### 3.1 Synthetic Composition Corpus (`tests/corpus/composition/`)
Generated deterministically via `tests/corpus/generate.rs`:
1. **Geometry Suite**: One test slide per T1 preset shape ([PRD 02b App](./02b-appendix-preset-geometries.md) §1) testing default and boundary `adj` values.
2. **Typography Suite**: Rich-run styling, hanging indents, autonumbering sequences, CJK fallback glyphs, Wingdings/Symbol bullet mappings.
3. **Fidelity Suite**: Dedicated slide for each DEGRADED and PLACEHOLDER matrix entry ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2).
4. **Boundary Cases**: Zero-width shapes, off-slide elements, single-pixel lines, overlapping z-orders.

### 3.2 Real-World Fidelity Corpus (`tests/corpus/fidelity/`)
A curated set of 50 permissively licensed real-world presentations compared against upstream browser reference renders.

### 3.3 Robustness & Fuzz Corpus (`tests/corpus/robustness/`)
- Seeded from upstream fuzz targets (`fuzz/fuzz_targets/fuzz_parse.rs`).
- Includes zip-bombs, truncated XML parts, 300-level nested groups, password-protected files, 500-slide decks, and slides with 1,000 shapes.
- **Panic Rule**: A Rust panic on any corpus deck is an immediate release blocker.

---

## 4. Perceptual Diff Metric & Golden Image Scoring

For real-world fidelity scoring, `kitty-pptx` evaluates Structural Dissimilarity (DSSIM):
$$\text{DSSIM}(x, y) = \frac{1.0 - \text{SSIM}(x, y)}{2.0}$$

### 4.1 Scoring Thresholds
- **PASS**: $\text{DSSIM} \le 0.02$ (visually indistinguishable at presentation distance).
- **REVIEW**: $0.02 < \text{DSSIM} \le 0.06$ (minor font substitution or antialiasing differences).
- **FAIL**: $\text{DSSIM} > 0.06$. If $> 10\%$ of corpus slides fail, the build is blocked.

### 4.2 Determinism Environment
To guarantee bit-identical goldens across development workstations and CI:
1. Pinned Rust compiler toolchain and committed `Cargo.lock`.
2. Bundled fonts compiled into binary (Carlito & Caladea).
3. Linux container pinned to Debian Bookworm with fixed fallback font packages (`fonts-dejavu-core`, `fonts-noto-cjk`).
4. Strict fixed render targets (`1920×1080` or `320×180`).

---

## 5. NFR Measurement Protocols

Every numeric NFR target from [PRD 00](./00-system-overview.md) §5 is verified by an automated protocol:

| Metric | Target | Measurement Method | Fixture Deck | Pass Threshold |
| :--- | :--- | :--- | :--- | :--- |
| **Startup to 1st Slide** | $< 120\text{ ms}$ | `execve` $\to$ first `present()` write complete; cold/warm cache | `typical-20.pptx` (20 slides, images, tables) | p95 $< 120\text{ ms}$ ($n=20$) |
| **Slide Flip Latency** | $< 16\text{ ms}$ | Keypress event poll read $\to$ `present()` escape write complete | Pre-staged `typical-20.pptx` slides | p99 $< 16\text{ ms}$ ($n=200$) |
| **Render Latency** | $< 15\text{ ms}$ / $< 40\text{ ms}$ | Compositor invocation $\to$ `RgbaSlide` returned | Simple deck (title + text) / Complex deck (tables + images) | p95 $< 15\text{ ms}$ (simple), $< 40\text{ ms}$ (complex) |
| **Peak Memory RSS** | $< 150\text{ MB}$ | `getrusage(RUSAGE_SELF).ru_maxrss` during full 50-slide walkthrough + sorter grid | `heavy-media-50.pptx` | Peak RSS $< 150\text{ MB}$ |
| **Stripped Binary Size**| $< 25\text{ MB}$ | `stat -c %s target/release/kitty-pptx` (stripped) | Linux `x86_64-unknown-linux-gnu` target | Size $< 25,000,000\text{ bytes}$ |

---

## 6. Pre-Release Manual Test Matrix

| Environment | TUI Presentation | Split Presenter View | Sorter Thumbnails | `view` Scrollback | Resize Scaling |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Kitty** (Linux/macOS) | Pass | Pass | Pass | Pass (U=1) | Pass (no re-render) |
| **Ghostty** (Linux/macOS) | Pass | Pass | Pass | Pass (U=1) | Pass (no re-render) |
| **WezTerm** (Linux/macOS) | Pass | Pass | Pass | Pass (U=1) | Pass (no re-render) |
| **Foot** (Sixel) | Pass | Pass | Pass | Paged fallback | Pass (re-emit) |
| **Plain VT100 / SSH** | Pass (Half-block) | Pass (Half-block) | Pass (Half-block) | Paged fallback | Pass (re-emit) |
