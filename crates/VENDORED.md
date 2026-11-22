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
2. `crates/patches/0002-fix-reflection-default-direction.patch`:
   - Fixes `parse_reflection_effect` to use the ECMA-376 default `dir = 5400000`
     (90 degrees); the upstream default of 0 mirrored reflections sideways. The
     defaults unit test is updated to the spec value.
6. `crates/patches/0006-pptx-parser-svg-only-placeholder-pictures.patch`:
   - `parse_pic_node`'s inherited-placeholder branch resolves the blip through
     `resolve_blip_source` instead of reading only `<a:blip r:embed>`. Picture
     placeholders whose blip carries only the Microsoft 2016 `asvg:svgBlip`
     extension (an SVG inserted as a picture, with no raster fallback) were
     dropped from the element list entirely, so the slide rendered without its
     image. The shared resolver already handles SVG-only blips, intrinsic PNG
     sizes and the SVG twin.


## Metafile Playback Crates (`metafile-rs`)

Playback engine for EMF / EMF+ / WMF pictures ([PRD 13](../docs/13-metafile-pictures-and-vector-playback.md)).

- **Upstream**: [`Dantusaikamal/metafile-rs`](https://github.com/Dantusaikamal/metafile-rs)
- **Pinned Commit**: `619d8c464cdd7eb21354b3a9ecf541f56cc7f955` (v1.0.1)
- **License**: Apache-2.0 (NOTICE retained in each crate directory)
- **Local patched copies** (overridden via `[patch.crates-io]`, excluded from workspace lints/tests):
  `crates/metafile-core`, `crates/metafile-dib`, `crates/metafile-emf`, `crates/metafile-emfplus`, `crates/metafile-wmf`
- `metafile-svg` is not used (the project renders into `VectorSlide`; the SVG backend is not linked).

### Applied Patches

3. `crates/patches/0003-metafile-clamp-bitmap-crops.patch`:
   - `metafile-dib::crop_bitmap` and `metafile-emfplus::crop_bitmap` clamp
     source rectangles to the bitmap bounds instead of failing playback.
     Windows GDI/GDI+ clamp in the same situation, and Office-authored
     metafiles routinely overhang by a fraction of a pixel; without this,
     real EMFs abort with `source crop lies outside bitmap` /
     `image source rectangle is empty or outside bitmap bounds`.
   - Upstreamed separately; when a release contains the fix, delete the two
     local copies and drop the `[patch.crates-io]` section.
4. `crates/patches/0004-metafile-tolerate-asymmetric-caps.patch`:
   - `metafile-emfplus::parse_pen` keeps the start cap when the record declares
     different start and end caps instead of returning
     `UnsupportedCriticalFeature`. The renderer-neutral `Stroke` carries one
     cap, and real Office/GDI+ streams routinely pair round start caps with
     flat end caps; rejecting the pen drops every path that selects it (chart
     frames, tick marks, axis spines), leaving large holes in otherwise valid
     EMF+ pictures. The single-cap vocabulary cannot distinguish the two ends,
     so keeping the start cap is the closest faithful stroke.
5. `crates/patches/0005-metafile-commit-continued-objects-at-total-size.patch`:
   - `metafile-emfplus` commits a split object when its accumulated bytes reach
     `TotalObjectSize`, not only when the final fragment clears the
     continuation flag. MS-EMFPLUS 2.3.5.1 makes the total size authoritative,
     and some producers leave the C bit set on the final fragment; without this
     the object never enters the object table and the first record that
     references it (for example `EmfPlusDrawImagePoints`) aborts playback with
     `invalid or empty EMF+ object id`, blanking the whole picture.
7. `crates/patches/0007-metafile-approximate-path-gradients.patch`:
   - `metafile-emfplus::parse_brush` handles `EmfPlusPathGradientBrush`
     (brush type 3) instead of rejecting it as an unsupported brush. The
     engine's paint vocabulary has no radial/path-gradient shape, so the
     surrounding colours become stops and the centre colour closes a linear
     ramp; the picture keeps painting and the adapter emits
     `metafile_gradient`. Object parsing now threads the diagnostic sink so the
     approximation is reported per picture instead of only being visible as a
     whole-picture placeholder.

8. `crates/patches/0008-metafile-glyph-index-text.patch`:
   - `metafile-core::TextRun` adds `glyph_indices: Vec<u16>` to preserve low-level glyph IDs.
   - `metafile-emf` maps `ETO_GLYPH_INDEX` records into `TextRun.glyph_indices` and emits non-fatal `glyph_index_text` diagnostic instead of aborting playback.
   - `metafile-emfplus` implements `EmfPlusDrawDriverString` (0x4036), parsing glyph arrays, brush paints, and transform matrices per MS-EMFPLUS §2.3.4.15.

## Bundled test/parity assets (not vendored crates)

- `crates/office-terminal-core/assets/fonts/` — Carlito, Caladea and Nunito Sans (SIL OFL 1.1), bundled so
  layout, rasterisation and SVG outlines resolve the same faces in a headless container. Sources and
  weights are listed in `assets/fonts/NOTICE.md`.
- `crates/office-terminal-core/src/compiler/shapes/presets.json` — ECMA-376 `presetShapeDefinitions.xml`
  content, copied verbatim from upstream `packages/core/src/shape/preset-geometry/presets.json`
  (MIT, © Yuki Yokotani). Evaluated, never translated by hand.
- `crates/office-terminal-core/tests/fixtures/parity/sample-1.pptx` and
  `.../reference/sample-1/slide-*.png` — the reference repo's demo deck (MIT, © Yuki Yokotani; the
  deck embeds eight unattributed JPEGs) and its committed VRT renders. The renders are adopted
  renderer outputs, not Office exports, so they are used as a drift oracle only; see
  `tests/fixtures/parity/README.md`.
- `crates/office-terminal-core/tests/fixtures/presets/` — an independent Python oracle
  (`gen_presets.py`) and its 6138-case guide-value JSON, generated from `presets.json`.
