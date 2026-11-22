# Changelog

All notable changes to this project are documented in this file.

## [0.1.0] - 2026-11-22

### Added
- **Core Presentation Engine**: High-fidelity OOXML PowerPoint (`.pptx`) parser and resolution-independent vector display list (`VectorSlide`).
- **PDF Document Support**: Direct PDF presentation and document reading with automatic slide-vs-document aspect detection.
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
