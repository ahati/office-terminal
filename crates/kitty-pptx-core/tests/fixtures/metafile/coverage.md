# Metafile Coverage Ledger (PRD 13a §2.4)

Status legend: ✅ fixture + assertion + AST golden + PNG golden · ⚠️ fixture
and assertion only (AST/PNG goldens land with the M6 corpus) · ❌ unsupported.

The M3–M5 fixtures are generated in code by
`crates/kitty-pptx-core/tests/metafile_support/mod.rs`; real-world binary
fixtures live beside this file and are listed in `README.md`.

## Classic EMF (MS-EMF)

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Header / bounds / frame | `windows-emf-paths.emf` | `metafile_skeleton_tests::real_emf_paths_play_into_commands` | — | — | ⚠️ |
| Lines / polylines / polygons / paths | `windows-emf-paths.emf` | same | — | — | ⚠️ |
| Rectangles / ellipses / arcs | `metafile_milestone_tests::placeable_wmf_plays_with_window_mapping` (rect) | command presence | — | — | ⚠️ |
| Solid pens / brushes | `windows-emf-paths.emf` | command presence | — | — | ⚠️ |
| Dashed / styled pens | engine unit tests | adapter mapping | — | — | ⚠️ |
| Hatched brushes | `metafile_milestone_tests::hatched_brush_keeps_ink_and_reports_degradation` | `metafile_hatch`, preset `horz` | — | — | ⚠️ |
| Pattern brushes | `metafile_milestone_tests::hatched_brush_keeps_ink_and_reports_degradation` | `metafile_pattern_brush` | — | — | ⚠️ |
| Text output | `metafile_milestone_tests::emf_text_*` | runs, alignment, `dx`, escapement, raster ink | — | — | ⚠️ |
| Text with glyph indices | engine emits `glyph_index_text` | `metafile_glyph_index_text` | — | — | ⚠️ |
| Opaque background | `metafile_milestone_tests::emf_opaque_text_emits_a_background_fill` | `DrawPath` before `DrawText` | — | — | ⚠️ |
| Font selection / substitution | `metafile_milestone_tests::emf_text_plays_with_font_substitution_and_decorations` | `FontSubstituted` | — | — | ⚠️ |
| Bitmaps | `windows-emf-bitmap.emf` | `DrawBitmap` raster/SVG | — | — | ⚠️ |
| Clipping (rect/path/region) | `windows-emf-paths.emf` | clip stack balance | — | — | ⚠️ |
| Clipping combine (OR/XOR/DIFF) | engine diagnostics | `metafile_clip_combine` | — | — | ⚠️ |
| ROP2 | engine diagnostics | `metafile_rop` | — | — | ⚠️ |
| World transform | engine unit tests | adapter rotation | — | — | ⚠️ |
| Window/viewport mapping | `metafile_milestone_tests::placeable_wmf_plays_with_window_mapping` | bounds mapping | — | — | ⚠️ |
| Unknown records | engine diagnostics | `metafile_record_ignored` | — | — | ⚠️ |
| Picture crop / silhouette / opacity | `metafile_skeleton_tests` | expansion + `metafile_opacity` | — | — | ⚠️ |

## EMF+ (MS-EMFPLUS)

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| EMF+ Only | `windows-gdiplus-emfplus-only.emf` | `real_emfplus_only_plays` | — | — | ⚠️ |
| EMF+ Dual (GetDC fallback) | `metafile_milestone_tests::emfplus_dual_getdc_fallback_is_reported` | `metafile_emfplus_fallback` | — | — | ⚠️ |
| Solid / linear gradients | `metafile_milestone_tests::linear_gradient_paint_maps_to_the_display_list` | angle + stops | — | — | ⚠️ |
| Path / radial gradients | — | engine rejects → §3 placeholder | — | — | ❌ (engine) |
| Texture brushes | engine adapter | average colour + `metafile_pattern_brush` | — | — | ⚠️ |
| Hatched brushes | `hatch` mapping | preset pattern + `metafile_hatch` | — | — | ⚠️ |
| Text (DrawString) | `metafile_milestone_tests::emf_text_*` (classic path) | alignment, escapement | — | — | ⚠️ |
| Images | `windows-emf-bitmap.emf` | `DrawBitmap` | — | — | ⚠️ |
| Clipping / regions | engine diagnostics | `metafile_clip_combine` | — | — | ⚠️ |
| Timing / animation records | — | reported as ignored records | — | — | ⚠️ |

## WMF (MS-WMF) and containers

| Family | Fixture | Assertions | AST | PNG | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Placeable header / units-per-inch | `metafile_milestone_tests::placeable_wmf_plays_with_window_mapping` | playback + ink | — | — | ⚠️ |
| Standard header | `windows-gdi-vector.wmf` | `real_wmf_plays` | — | — | ⚠️ |
| Vector primitives | `windows-gdi-vector.wmf` | command presence | — | — | ⚠️ |
| Text output | `metafile_milestone_tests::placeable_wmf_plays_with_window_mapping` | `Hi` run | — | — | ⚠️ |
| DIB blits | engine unit tests | `DrawBitmap` | — | — | ⚠️ |
| Unknown records | engine diagnostics | `metafile_record_ignored` | — | — | ⚠️ |
| `.emz` gzip container | `metafile_milestone_tests::emz_inflates_a_real_emf` | expansion | — | — | ⚠️ |
| `.wmz` gzip container | `metafile_milestone_tests::wmz_inflates_a_real_wmf` | expansion | — | — | ⚠️ |
| Mislabelled container | `metafile_milestone_tests::mislabelled_emz_is_classified_by_content` | classification by content | — | — | ⚠️ |
| Gzip bomb | `metafile_milestone_tests::emz_bomb_degrades_to_a_placeholder` | `ResourceLimitHit` + placeholder | — | — | ⚠️ |

## Picture-level suite

| Case | Test | Status |
| :--- | :--- | :--- |
| Full-bleed picture | `metafile_skeleton_tests::real_emf_paths_play_into_commands` | ⚠️ |
| Unexpanded marker | `metafile_skeleton_tests::unexpanded_marker_never_paints_nothing` | ⚠️ |
| Empty metafile | `metafile_skeleton_tests::empty_metafile_expands_to_a_placeholder` | ⚠️ |
| Inline bitmap raster/SVG | `metafile_skeleton_tests::draw_bitmap_*` | ⚠️ |
