# Contributing to kitty-pptx

Thank you for your interest in contributing to **kitty-pptx**! We welcome bug reports, feature proposals, documentation improvements, and code contributions.

Please take a moment to review this guide to ensure a smooth contribution process.

---

## Code of Conduct

All contributors and maintainers are expected to follow our [Code of Conduct](./CODE_OF_CONDUCT.md).

---

## Architectural Principles & Strict Conventions

Before submitting code, please be aware of our core design tenets (detailed in [`AGENTS.md`](./AGENTS.md)):

### 1. Strict File Size Limit (< 600 Lines)
- **Hard Rule**: No Rust source file (`*.rs`) in `crates/kitty-pptx-*` may exceed **600 lines**.
- Proactively split modules when approaching 400–500 lines into logically coherent submodules.

### 2. Strict Zero-Warning Policy
- The entire workspace must compile cleanly with **zero warnings**:
  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  ```
- Unused imports, dead code, or clippy lints must not be introduced.

### 3. Two-Stage Vector Pipeline Discipline
- **Vector-First**: We never render directly from PowerPoint / PDF DOM to pixels.
- All slides and pages compile to the intermediate representation (`VectorSlide` and `DrawCommand`):
  1. **Stage 1 (Compile)**: Document/Slide DOM $\rightarrow$ `VectorSlide`.
  2. **Stage 2 (Render/Export)**:
     - For Terminal / PNG: `VectorSlide` $\rightarrow$ `tiny-skia` $\rightarrow$ `RgbaImage`.
     - For SVG: `VectorSlide` $\rightarrow$ standalone `<svg>` XML with selectable text.

### 4. Terminal Protocol Hygiene
- The interactive TUI must leave the user's terminal clean under all conditions (normal exit, error exit, panic, SIGINT/Ctrl-C).
- Always ensure panic hooks and signal handlers restore raw mode, unhide the cursor, leave alternate screens, and issue Kitty graphics cleanup sequences.

---

## Development Setup

### Prerequisites

- **Rust**: Version 1.75 or later with Cargo ([rustup.rs](https://rustup.rs/))
- **Terminal Emulator**: Kitty, Ghostty, WezTerm, or an Alacritty fork supporting Kitty graphics protocol.
- **Git**

### Build Commands

```bash
# Clone the repository
git clone https://github.com/ahati/kitty-pptx.git
cd kitty-pptx

# Check workspace compilation
cargo check --workspace

# Run Clippy (must have zero warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Run all test suites
cargo test --workspace

# Verify file line counts
find crates/kitty-pptx-* -name "*.rs" | xargs wc -l | sort -nr | head -n 20
```

> [!NOTE]
> **Dev Profile Performance**: The repository sets `[profile.dev]` to release-like optimizations (`opt-level = 3`, `debug = false`) so that `cargo run` and slide rendering are responsive during everyday development. If you need full stack traces with line numbers while debugging, run tests or builds with `debug = 1` or `debug = true` in your local override.

---

## Commit Guidelines

We use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` New features or subcommands
- `fix:` Bug fixes or rendering artifact corrections
- `refactor:` Modular domain splits or non-breaking restructuring
- `perf:` Performance optimizations
- `test:` Test additions or test fixture updates
- `docs:` Documentation updates
- `chore:` Maintenance, dependency bumps, or build updates

---

## Submitting Pull Requests

1. Fork the repository and create your branch from `master`.
2. Ensure `cargo clippy --workspace --all-targets -- -D warnings` passes cleanly.
3. Ensure all tests pass (`cargo test --workspace`).
4. Ensure no new or modified source file exceeds 600 lines.
5. Submit a pull request with a clear description of the problem solved and test results.
