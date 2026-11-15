# Metafile Coverage Ledger (PRD 13a §2.4)

Status legend: ✅ fixture + assertion + AST golden + PNG golden · ⚠️ fixture
and assertion only (AST/PNG goldens land with the M6 corpus) · ❌ unsupported.

The M3–M5 fixtures are generated in code by
`crates/kitty-pptx-core/tests/metafile_support/mod.rs`; real-world binary
fixtures live beside this file and are listed in `README.md`.

## Classic EMF (MS-EMF)

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Header / bounds / frame | `windows-emf-paths.emf` | `metafile_skeleton_tests::real_emf_paths_play_into_commands` | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Lines / polylines / polygons / paths | `windows-emf-paths.emf` | same | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Rectangles / ellipses / arcs | `synthetic-wmf-placeable.wmf` | command presence | `ast/synthetic-wmf-placeable.json` | `png/synthetic-wmf-placeable-960x720.png` | ✅ |
| Solid pens / brushes | `windows-emf-paths.emf` | command presence | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Asymmetric start/end caps | `synthetic-emf-pen-caps.emf` | frame stroke emitted (patch 0004) | `ast/synthetic-emf-pen-caps.json` | `png/synthetic-emf-pen-caps-960x720.png` | ✅ |
| Dashed / styled pens | `windows-emf-paths.emf` | adapter mapping | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Hatched brushes | `synthetic-emf-hatch.emf` | `metafile_hatch`, preset `horz` | `ast/synthetic-emf-hatch.json` | `png/synthetic-emf-hatch-960x720.png` | ✅ |
| Pattern brushes | `synthetic-emf-hatch.emf` | `metafile_pattern_brush` | `ast/synthetic-emf-hatch.json` | `png/synthetic-emf-hatch-960x720.png` | ✅ |
| Text output | `synthetic-emf-text.emf` | runs, alignment, `dx`, escapement, raster ink | `ast/synthetic-emf-text.json` | `png/synthetic-emf-text-960x720.png` | ✅ |
| Text with glyph indices | `synthetic-emf-text.emf` | `metafile_glyph_index_text` | `ast/synthetic-emf-text.json` | `png/synthetic-emf-text-960x720.png` | ✅ |
| Opaque background | `synthetic-emf-opaque-text.emf` | `DrawPath` before `DrawText` | `ast/synthetic-emf-opaque-text.json` | `png/synthetic-emf-opaque-text-960x720.png` | ✅ |
| Font selection / substitution | `synthetic-emf-text.emf` | `FontSubstituted` | `ast/synthetic-emf-text.json` | `png/synthetic-emf-text-960x720.png` | ✅ |
| Bitmaps | `windows-emf-bitmap.emf` | `DrawBitmap` raster/SVG | `ast/windows-emf-bitmap.json` | `png/windows-emf-bitmap-960x720.png` | ✅ |
| Clipping (rect/path/region) | `windows-emf-paths.emf` | clip stack balance | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Clipping combine (OR/XOR/DIFF) | `windows-emf-paths.emf` | exact pixels for simple closed operands; curved/Complement fallback | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| ROP2 | `windows-emf-paths.emf` | `metafile_rop` | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| World transform | `windows-emf-paths.emf` | adapter rotation | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Window/viewport mapping | `synthetic-wmf-placeable.wmf` | bounds mapping | `ast/synthetic-wmf-placeable.json` | `png/synthetic-wmf-placeable-960x720.png` | ✅ |
| Unknown records | `windows-emf-paths.emf` | `metafile_record_ignored` | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |
| Picture crop / silhouette / opacity | `windows-emf-paths.emf` | expansion + `metafile_opacity` | `ast/windows-emf-paths.json` | `png/windows-emf-paths-960x720.png` | ✅ |

## EMF+ (MS-EMFPLUS)

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| EMF+ Only | `windows-gdiplus-emfplus-only.emf` | `real_emfplus_only_plays` | `ast/windows-gdiplus-emfplus-only.json` | `png/windows-gdiplus-emfplus-only-960x720.png` | ✅ |
| EMF+ Dual (GetDC fallback) | `synthetic-emfplus-dual.emf` | `metafile_emfplus_fallback` | `ast/synthetic-emfplus-dual.json` | `png/synthetic-emfplus-dual-960x720.png` | ✅ |
| Split objects (`TotalObjectSize`) | `synthetic-emfplus-split.emf` | object committed before first draw (patch 0005) | `ast/synthetic-emfplus-split.json` | `png/synthetic-emfplus-split-960x720.png` | ✅ |
| Solid / linear gradients | `synthetic-emfplus-linear-gradient.emf` | angle + stops | `ast/synthetic-emfplus-linear-gradient.json` | `png/synthetic-emfplus-linear-gradient-960x720.png` | ✅ |
| Path / radial gradients | `synthetic-emfplus-path-gradient.emf` | linear approximation + `metafile_gradient` (patch 0007) | `ast/synthetic-emfplus-path-gradient.json` | `png/synthetic-emfplus-path-gradient-960x720.png` | ✅ |
| Texture brushes | `windows-gdiplus-emfplus-only.emf` | average colour + `metafile_pattern_brush` | `ast/windows-gdiplus-emfplus-only.json` | `png/windows-gdiplus-emfplus-only-960x720.png` | ✅ |
| Hatched brushes | `synthetic-emf-hatch.emf` | preset pattern + `metafile_hatch` | `ast/synthetic-emf-hatch.json` | `png/synthetic-emf-hatch-960x720.png` | ✅ |
| Text (DrawString) | `synthetic-emfplus-text-transform.emf` | alignment, escapement | `ast/synthetic-emfplus-text-transform.json` | `png/synthetic-emfplus-text-transform-960x720.png` | ✅ |
| DrawString world/container transform | `synthetic-emfplus-text-transform.emf` | anchor, rotation | `ast/synthetic-emfplus-text-transform.json` | `png/synthetic-emfplus-text-transform-960x720.png` | ✅ |
| Images | `windows-emf-bitmap.emf` | `DrawBitmap` | `ast/windows-emf-bitmap.json` | `png/windows-emf-bitmap-960x720.png` | ✅ |
| Clipping / regions | `windows-gdiplus-emfplus-only.emf` | `metafile_clip_combine` | `ast/windows-gdiplus-emfplus-only.json` | `png/windows-gdiplus-emfplus-only-960x720.png` | ✅ |
| Timing / animation records | `windows-gdiplus-emfplus-only.emf` | reported as ignored records | `ast/windows-gdiplus-emfplus-only.json` | `png/windows-gdiplus-emfplus-only-960x720.png` | ✅ |

## WMF (MS-WMF) and containers

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Placeable header / units-per-inch | `synthetic-wmf-placeable.wmf` | playback + ink | `ast/synthetic-wmf-placeable.json` | `png/synthetic-wmf-placeable-960x720.png` | ✅ |
| Standard header | `windows-gdi-vector.wmf` | `real_wmf_plays` | `ast/windows-gdi-vector.json` | `png/windows-gdi-vector-960x720.png` | ✅ |
| Vector primitives | `windows-gdi-vector.wmf` | command presence | `ast/windows-gdi-vector.json` | `png/windows-gdi-vector-960x720.png` | ✅ |
| Text output | `synthetic-wmf-placeable.wmf` | `Hi` run | `ast/synthetic-wmf-placeable.json` | `png/synthetic-wmf-placeable-960x720.png` | ✅ |
| DIB blits | `windows-emf-bitmap.emf` | `DrawBitmap` | `ast/windows-emf-bitmap.json` | `png/windows-emf-bitmap-960x720.png` | ✅ |
| Unknown records | `windows-gdi-vector.wmf` | `metafile_record_ignored` | `ast/windows-gdi-vector.json` | `png/windows-gdi-vector-960x720.png` | ✅ |
| `.emz` gzip container | `synthetic-emz.emz` | expansion | `ast/synthetic-emz.json` | `png/synthetic-emz-960x720.png` | ✅ |
| `.wmz` gzip container | `synthetic-wmz.wmz` | expansion | `ast/synthetic-wmz.json` | `png/synthetic-wmz-960x720.png` | ✅ |
| Mislabelled container | `synthetic-mislabelled.emz` | classification by content | `ast/synthetic-mislabelled.json` | `png/synthetic-mislabelled-960x720.png` | ✅ |
| Gzip bomb | `synthetic-bomb.emz` | `ResourceLimitHit` + placeholder | `ast/synthetic-bomb.json` | `png/synthetic-bomb-960x720.png` | ✅ |

## Hardening (M6)

| Gate | Test | Status |
| :--- | :--- | :--- |
| Perf: cold playback + cache hit | `tests/metafile_perf_tests.rs` (release) | ⚠️ budget smoke only |
| Deterministic robustness | `tests/metafile_robustness_tests.rs` | ⚠️ 200-case smoke |
| Extended fuzz | `fuzz/fuzz_targets/fuzz_metafile.rs` | ⚠️ cargo-fuzz, nightly |
| Playback override | `tests/metafile_mode_tests.rs` | ⚠️ |
| Peak allocation | `tests/metafile_rss_tests.rs` (5.5× input) | ⚠️ |
| L4 warnings drawer | `overlay_tests::metafile_warnings_render_in_the_drawer` | ⚠️ |
| L5 CLI E2E | `crates/kitty-pptx-cli/tests/metafile_e2e_tests.rs` | ⚠️ |
| External golden harness | `tests/external_fixture_tests.rs` (expands metafiles) | ⚠️ diagnostic baseline |

## Picture-level suite

| Case | Test | Status |
| :--- | :--- | :--- |
| Full-bleed picture | `metafile_skeleton_tests::real_emf_paths_play_into_commands` | ✅ |
| Unexpanded marker | `metafile_skeleton_tests::unexpanded_marker_never_paints_nothing` | ✅ |
| Empty metafile | `metafile_skeleton_tests::empty_metafile_expands_to_a_placeholder` | ✅ |
| Inline bitmap raster/SVG | `metafile_skeleton_tests::draw_bitmap_*` | ✅ |
