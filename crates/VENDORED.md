# Vendored Upstream Crates

The following crates in this directory are vendored from [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer):

| Crate | Upstream Package Path | License |
| :--- | :--- | :--- |
| `crates/ooxml-common` | `packages/ooxml-common` | MIT |
| `crates/pptx-model` | `packages/pptx/model` | MIT |
| `crates/pptx-parser` | `packages/pptx/parser` | MIT |

- **Pinned Commit**: `2da9dd7a08eec50d58c4a3ba2b31500c52ea03dc`
- **Vendored Date**: 2026-11-24
- **Copy rule**: verbatim from the pinned commit; the parser package's standalone
  `Cargo.lock` is dropped (the workspace lock governs). `pptx-model`'s
  `examples/wire-schema.rs` is kept — upstream gates it behind the dev-only `schema`
  feature via `required-features`, so workspace builds skip it.
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
11. `crates/patches/0011-ooxml-common-typed-resource-violation.patch`:
   - `resource.rs` grows the owned, `pub` `ResourceViolation` with its `violation()`
     builder, and `ResourceUsage` derives `PartialEq, Eq`. Upstream's private
     borrowed wire struct `ResourceViolation<'a>` is renamed
     `WireResourceViolation<'a>` to free the name; `package_session.rs` adds
     `first_violation()` so a latched package-wide ceiling is propagated typed
     instead of being flattened to its wire string. `office-terminal-core` and
     `office-terminal-docx` map the violation onto their `ResourceLimit` error
     arms.
12. `crates/patches/0012-pptx-parser-typed-native-errors.patch`:
   - adds `src/error.rs` (`PptxError::{Resource, Format}`), the reader-private
     `first_violation()` accessor, and the typed entry points
     `parse_pptx_model_native_checked` / `extract_font_checked`, so a governed
     resource crossing surfaces as `PptxError::Resource(violation)` instead of
     a formatted string (ref `core/src/errors/ooxml-error.ts`
     `OoxmlResourceLimitError`). Previously carried undocumented since the
     workspace scaffold; re-vendoring at `2da9dd7` made it a numbered patch.

   Patches 11 and 12 predate the numbering regime (they landed with the typed
   resource-limit error waves) and were re-captured as patches during the
   2026-11-24 re-vendor; the full sequence 0001, 0002, 0006, 0011, 0012 applies
   cleanly, in order, onto the pristine pinned tree.


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

## DOCX Layout Engine (`rdocx-layout`)

Pagination engine behind the DOCX input track ([PRD 15](../docs/15-docx-input-support.md)).

- **Upstream**: [`tensorbee/rdocx`](https://github.com/tensorbee/rdocx), crate `rdocx-layout`
- **Pinned version**: `0.17.0` (crates.io package `rdocx-layout-0.17.0`, `Cargo.toml.orig` retained)
- **License**: MIT OR Apache-2.0; the package ships no license text, so `crates/rdocx-layout/LICENSE`
  is the upstream `LICENSE` at tag `v0.17.0` (MIT, © 2025 tensorbee).
- **Local patched copy** (overridden via `[patch.crates-io]`, excluded from workspace lints/tests):
  `crates/rdocx-layout`. The package's own test suite is not built by the workspace; run it from a
  scratch copy with `[workspace]` added to its `Cargo.toml` and `../../oxml-layout` symlinked to the
  `oxml-layout-0.15.0` package (two tests `include_bytes!` its bundled fonts). Patch 0010 removes the
  one monorepo-only file dependency that could not be satisfied that way.
- The rest of the `rdocx` family (`rdocx`, `rdocx-layout`'s siblings, `oxml-*`) is consumed from crates.io.

### Applied Patches

9. `crates/patches/0009-rdocx-layout-continuous-column-band.patch`:
   - `paginator.rs`: a **continuous section whose column tracks or page margins differ from the
     section above it stays on the current page**, as a new body band below the content already
     placed (`Pager::begin_band`), with the member's own margins taking effect from its next page.
     Upstream `continuous_geometry_matches` grouped continuous sections only when tracks *and*
     margins were identical, and the pager fixes a page's tracks from the first member that places
     content, so a continuous one-column → two-column change always started a new page. In Word
     (`sample3.docx`, Word 2-page render) the two-column section begins under its heading on the
     same page; upstream rendered the heading stranded on one page and the section — stacked in
     column 1 only — on the next.
   - Verified against the engine's own suite run outside the monorepo (see above): 422 passed /
     0 failed / 1 ignored (the engine's own pre-existing `#[ignore]`).
   - Still open upstream and **not** addressed by this patch: columns are never balanced when a
     section ends mid-page (Word splits 8 lines 4 / 4; the engine fills column 1 first), and column
     breaks (`<w:br w:type="column"/>`) are not modelled (`paginator.rs`: "F-269 does not model
     it"). Both are tracked as `#[ignore]`d tests in `engine_roundtrip_tests.rs`.
   - To upgrade: copy the new package over `crates/rdocx-layout`, re-apply 0009 and 0010 in order with
     `patch -p1 < crates/patches/00NN-*.patch`, re-run `cargo test -p office-terminal-docx`, and
     drop the copy and the `[patch.crates-io]` entry once a release carries the fix (PR to follow).
10. `crates/patches/0010-rdocx-layout-drop-monorepo-oracle-manifest-assert.patch`:
   - `math.rs`: the Word-oracle OMML test keeps its 13 geometry assertions but no longer
     `include_str!`s the monorepo's `scripts/officemath_oracle_manifest.json` to assert provenance
     bindings (Word build, poppler version, source SHA-256). The published package does not carry
     that file, so outside the monorepo the test suite could not compile without a stub and this one
     test failed on `missing oracle binding`; the provenance check guards nothing about layout and
     remains in place upstream.

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
