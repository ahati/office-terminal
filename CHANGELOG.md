# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed
- Rebranded the project to **Office Terminal** (`ot` / `office-terminal`). Legacy
  `kitty-pptx` config paths, filenames and environment variables are still accepted
  as fallbacks.
- Raised the declared MSRV to Rust 1.93, matching the DOCX engine
  (`rdocx-layout` 0.17).
- Replaced the uncleared upstream `sample-1` parity deck and its reference PNGs
  with the synthetic `synthetic-1` fixture. The old assets were purged from git
  history.
- Marked the XLSX track as a proposal; the loader rejects `.xlsx` input.

### Added
- `THIRD_PARTY_NOTICES.md`, the full OFL 1.1 text for the bundled fonts, and
  license/notice files in release archives.
- `CODE_OF_CONDUCT.md`, `SECURITY.md`, and screenshot provenance notes.
- MSRV and first-party `cargo fmt` checks in CI.

### Fixed
- Release workflow: current macOS runners, portable checksum generation, and
  `contents: write` permission for release uploads.
- `cargo run -p office-terminal-cli` now selects the `ot` binary by default.
- Release archives now include the changelog, third-party notices, font license,
  and vendoring provenance.

## [0.1.0] - 2026-11-22

### Added
- **Core Presentation Engine**: High-fidelity OOXML PowerPoint (`.pptx`) parser and resolution-independent vector display list (`VectorSlide`).
- **PDF Document Support**: Direct PDF presentation and document reading with automatic slide-vs-document aspect detection.
- **Word Document Input**: `.docx` / `.dotx` / `.docm` pagination through the vendored `rdocx` layout engine, rendered in Reader Mode with headings outline, full-text search, and PNG/SVG export parity.
- **Kitty Graphics Protocol Driver**: Native PNG transmission, direct cell placement, and terminal dimension probing with no terminal fallback.
- **Dual-Mode Interactive TUI Presenter**:
  - Full-bleed normal view with aspect ratio compensation.
  - Split-pane presenter view with current slide, next slide preview thumbnail, speaker notes, and presentation timer.
  - Interactive 2D slide sorter grid (`Tab` or `s`) with instant slide navigation.
  - Multi-page document reader mode with continuous scroll and side-by-side spread.
  - Interactive table of contents / fuzzy search navigation overlay.
- **Headless Vector & Raster Export**:
  - Batch SVG export with selectable text or converted glyph outlines.
  - High-resolution PNG export at custom DPI or explicit pixel widths.
- **Document Inspection**: CLI commands for extracting presentation structure, slide dimensions, speaker notes, and comments (`dump` and `info`).
- **Shell Completions**: Shell script generation for Bash, Zsh, Fish, Elvish, and PowerShell.
