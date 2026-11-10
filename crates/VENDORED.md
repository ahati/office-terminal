# Vendored Upstream Crates

The following crates in this directory are vendored from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer):

| Crate | Upstream Package Path | License |
| :--- | :--- | :--- |
| `crates/ooxml-common` | `packages/ooxml-common` | MIT |
| `crates/pptx-model` | `packages/pptx/model` | MIT |
| `crates/pptx-parser` | `packages/pptx/parser` | MIT |

- **Pinned Commit**: `4d6828bb85b3be58c26e83bd7f4ef025a118ead6`
- **Vendored Date**: 2026-11-10
- **Policy**: Per [PRD 01](../docs/01-parser-and-data-model.md) §2.2, re-vendoring is performed deliberately, gated by the full test suite. Any local modifications must be maintained as numbered patches in `crates/patches/`.

## Applied Patches

1. `crates/patches/0001-fix-lexical-depth-test-include-path.patch`:
   - Adjusts `include!` path in `pptx-parser/src/lib.rs` for peer `crates/ooxml-common` directory layout.

