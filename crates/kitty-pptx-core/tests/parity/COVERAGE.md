# Consolidated parity coverage (core test port)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only).
Scope: every `parity_*` test target under `crates/kitty-pptx-core/tests/`.
Sources merged: `tests/parity/coverage/{elements,fonts,misc,shapes-a,shapes-b,text-box-a,text-box-b,text-flow-a,text-flow-b}.md`.

Verification (this pass, `CARGO_TARGET_DIR=target/agent-verify`):

```
cargo test -p kitty-pptx-core --test <target>        # one target at a time, all 25 targets
cargo clippy -p kitty-pptx-core --tests -- -D warnings
```

Result: **25/25 targets compile, 378 tests, 284 pass, 94 ignored, 0 failed; clippy clean.**
No production code was modified in this pass.

`cases` counts reference runtime cases (every `it.each` row / loop expansion once).
`pass`/`fail` count Rust test functions in the batch's targets (`ignore`d stubs are compile-only).

## Summary

| batch | reference files | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | pass | fail |
| :--- | --: | --: | --: | --: | --: | --: | --: | --: |
| `elements` | 11 | 89 | 3 | 20 | 8 | 58 | 24 | 0 |
| `fonts` | 12 | 96 | 3 | 9 | 78 | 6 | 12 | 0 |
| `misc` | 11 | 37 | 1 | 8 | 8 | 20 | 10 | 0 |
| `shapes-a` | 5 | 36 | 4 | 4 | 6 | 22 | 11 | 0 |
| `shapes-b` | 7 | 65 | 0 | 8 | 3 | 54 | 8 | 0 |
| `text-box-a` | 6 | 112 | 22 | 32 | 1 | 57 | 41 | 0 |
| `text-box-b` | 12 | 137 | 30 | 9 | 6 | 92 | 45 | 0 |
| `text-flow-a` | 10 | 77 | 44 | 9 | 0 | 24 | 53 | 0 |
| `text-flow-b` | 14 | 79 | 41 | 15 | 0 | 23 | 53 | 0 |
| **total** | **88** | **728** | **148** | **114** | **110** | **356** | **257** | **0** |

Every batch row is self-consistent (`EXACT + BEHAVIOURAL + N/A + BLOCKED == cases`).
262 of 728 cases are ported and asserted (148 EXACT + 114 BEHAVIOURAL); 93 further
cases carry an `#[ignore]`d stub with the missing symbol in its reason.

## Targets and run results

| batch | target | tests | pass | ignored | fail |
| :--- | :--- | --: | --: | --: | --: |
| `elements` | `parity_elements_tests` | 14 | 14 | 0 | 0 |
| `elements` | `parity_elements_table_tests` | 7 | 7 | 0 | 0 |
| `elements` | `parity_elements_chart_tests` | 3 | 3 | 0 | 0 |
| `fonts` | `parity_fonts_tests` | 12 | 12 | 0 | 0 |
| `misc` | `parity_misc_tests` | 10 | 10 | 0 | 0 |
| `shapes-a` | `parity_shapes_a_tests` | 13 | 11 | 2 | 0 |
| `shapes-b` | `parity_shapes_b_tests` | 8 | 8 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_tests` | 10 | 10 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_spacing_tests` | 19 | 19 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_autofit_tests` | 12 | 12 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_tests` | 24 | 9 | 15 | 0 |
| `text-box-b` | `parity_text_box_b_bullet_image_tests` | 14 | 3 | 11 | 0 |
| `text-box-b` | `parity_text_box_b_highlight_tests` | 14 | 2 | 12 | 0 |
| `text-box-b` | `parity_text_box_b_eavert_tests` | 33 | 19 | 14 | 0 |
| `text-box-b` | `parity_text_box_b_stacked_tests` | 32 | 7 | 25 | 0 |
| `text-box-b` | `parity_text_box_b_warp_tests` | 19 | 5 | 14 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests` | 20 | 20 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_tabs` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_wrap` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_draw_tests` | 7 | 7 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_tests` | 35 | 35 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_sea_tests` | 12 | 12 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_indent_tests` | 6 | 6 | 0 | 0 |
| | **batch subtotal (23 targets)** | **350** | **257** | **93** | **0** |

Out-of-batch `parity_*` targets (pre-existing, tracked in git, no coverage report):

| target | tests | pass | ignored | fail | note |
| :--- | --: | --: | --: | --: | :--- |
| `parity_fidelity_tests` | 1 | 1 | 0 | 0 | VRT deck fidelity (`parity_support::vrt::DECKS`) |
| `parity_text_tests` | 27 | 26 | 1 | 0 | lead's text kernel suite; the 1 ignore is `bulleted_first_line_ignores_the_positive_indent` ("not expressible: `BreakOptions` has no bullet flag") |
| | **grand total (25 targets)** | **378** | **284** | **94** | **0** |

Non-target modules: `parity_support/{mod,deck,pixelmatch,scores,vrt}.rs`,
`parity_text_flow_a_support/mod.rs`, `shapes_a_support/{mod,image_fill_cases}.rs`,
`text_box_a_support/mod.rs`.

## BLOCKED cases and the missing API

356 cases are blocked. Each row enumerates the blocked cases of one reference file
(or a named subset) and the symbol that would unblock them. `n of m` means the
complement of the file's ported + N/A cases.

### `elements` — 58 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| Display-sized decode planner (`targetWidthPx`/`targetHeightPx`, `imageResources.resolution`, `SizeHint::AtMost`); per-image decode error surface; bitmap placeholder painter | `renderer-picture-raster-target.test.ts` | 17 of 19 | all but the ported `fillRect` destination case and the canvas-chunking N/A: SVG-effect routing, `PICT image unsupported` label, 8 archive-failure sites, TIFF fatal/optional (×3), device-pixel derivation, pixel-effect source grid (×2) |
| Decode planner + decoded-byte budget/admission (`decodedByteBudget`, `strategy`, `ooxml-decoded-image-limit`, live-byte lease) | `renderer-adaptive-image-budget.test.ts` | 6 of 9 | 32×22 planned axes, one bounded quality level, 109 MP supersampled target, TIFF not capped, SVG display target, strict quota |
| Poster raster path (`getPosterBitmap`) with header-sniffed sizing + shared live-byte ceiling | `poster-decode-bomb.test.ts` | 6 of 6 | every case; `MediaElement` compiles to a vector placeholder and `poster_path` is never painted |
| Multi-column body (`TextBody::num_col`/`spc_col` consumed by `text::emit`) | `multi-column-overflow.test.ts` | 2 of 2 | both `bodyPr@numCol="2"` cases |
| Non-background image fill (`compile_fill(Fill::Image)`) | `renderer-table-image-fill.test.ts` | 3 of 3 | shared blip decode, "full background before alpha bands" order, `srcRect` cell clip |
| Text-run selection info (`PptxTextRunInfo`: `shapeId`, `elementIndex`, `origin`, per-cell frame math) | `table-text-selection.test.ts` | 7 of 14 | cell identity/geometry, empty cells as gaps, selection transform math, rotation+`flipH`, `flipV`, 30° frame mapping, element identity through the pipeline |
| `ChartFamily::Bubble` renderer + `ChartExRenderer` injection point | `renderer-chart-bubble3d.test.ts` | 2 of 2 | both cases (`bubble3D` radial gradients, `source-atop` composites) |
| Chart image-fill pipeline (decode plan + `Fill::Image` in charts) | `renderer-chart-picture-marker.test.ts` | 15 of 15 | every case (marker/fill preload, crop+DPR dedup, SVG twin, duotone fail-closed, tiled-vs-stretched retention, invalid frame) |

### `fonts` — 6 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `TextRunData::font_family_cs` consumed by `text::emit` | `font-requests.test.ts` | 2 of 11 | "retains authored CS faces in shape/table preload names" (×2) |
| Typed resource-limit error taxonomy for embedded fonts | `embedded-fonts.test.ts` | 1 of 9 | "preserves other typed resource-limit errors" |
| Typed (non-aborting) load rejection from `pptx_parser::{extract_font, PptxArchive}` | `embedded-fonts-load.test.ts` | 3 of 3 | limit `4096`/observed `4097`, face release, rejection surface |

### `misc` — 20 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `apply_dim_overlay(&mut VectorSlide, color, opacity)` | `dim-overlay.test.ts` | 1 of 1 | the single overlay case |
| `Slide::parse_error` placeholder emission + `RenderOptions::error_placeholder` | `renderer.parse-error.test.ts` | 4 of 7 | placeholder title/detail variants over a parse-error slide |
| `background_luminance`, `smart_art_fallback_text_color`, `is_smart_art_fallback_shape`, run identity (`shape_id`/`element_index`/`origin`), theme + caller `defaultTextColor`, hyperlink text colour | `smartart-fallback-contrast.test.ts` | 15 of 19 | all contrast branches except the ported `ED7D31`/dark-background/black-default/explicit-run-colour cases |

### `shapes-a` — 22 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `ShapeElement::cust_geom_paint` (`PathPaint`) consumed by `build_shape_paths` | `custgeom-path-paint.test.ts` | 1 of 2 | per-path `fill`/`stroke` flags |
| Gradient options in the IR (`VectorGradient::rot_with_shape`, gradient paint on `VectorStroke`) | `renderer.gradient.test.ts` | 2 of 3 | `rotWithShape=false` counter-rotation; gradient stroke |
| Pattern paint with the `12700/8` EMU cell | `pattern-thumbnail.test.ts` | 1 of 1 | pattern thumbnail rendering |
| `Fill::Image` compilation (crop, insets, tiling, `rotWithShape`, opacity, blip effects, tile budget) | `renderer-shape-image-fill.test.ts` | 18 of 27 | #4-6, #8, #10-13, #14-16, #21-27 (per-case table in `coverage/shapes-a.md`) |

### `shapes-b` — 54 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `build_bevel_inputs`/`bevel_band_px` without the 48 px clamp and with a `w>0 && h>0` gate | `bevel-band-conversion.test.ts` | 2 of 4 | devScale 4 (192 px) and devScale 8 (384 px); zero-size omission |
| `reflection_blur_bands(bounds, max_blur)` | `reflection-blur.test.ts` | 3 of 3 | distance-aware radius ramp, band clipping, 24-pass cap |
| `projected_effect_geometry` + `VectorScene3d::{zoom, rot{lat,lon,rev}}` | `shape-effect-transform.test.ts` | 4 of 9 | cases 3-6 |
| Full 3D bevel projection + PNG/pixel oracle (`VectorScene3d::{rot,zoom,light_rig.rot}`, `prst_material`, `bevel_b`, extrusion walls, wireframe rims) | `shape-orthographic-bevel.test.ts` | 38 of 39 | every case except the ported inner-shadow outline |
| `reflected_shape_text_rotation(rotation, flip_h, flip_v)` | `shape-text-reflection.test.ts` | 7 of 7 | all `it.each` rows |

### `text-box-a` — 57 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `powerPointAscentShare` face catalogue + `OBSERVED_SUBSTITUTES`, `SHARE_CACHE_LIMIT` LRU, `excelDrawingMlLineRatios` (`compatLnSpc="0"`), `powerPointFontAlgn*`, `a:br`/`endParaRPr` face accounting | `powerpoint-line-metrics.test.ts` | 49 of 60 | A2-A3, A10, B2-B5, C1-C2, C3-C6 |
| `natural_width_exceeds_bbox` preflight, `a:ea` metric slot, tab start pen `= marL + textXOffset` | `firstline-indent-autofit.test.ts` | 4 of 12 | F1, F3, F4, F8 |
| Resolved canvas font box (`useResolvedFontMetrics`) | `sp-autofit-top-anchor.test.ts` | 4 of 19 | S7-S9 plus the block's remaining resolved-font-box assertion |

### `text-box-b` — 92 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `paragraph_has_renderable_content`, per-level list counter (skip empty paragraphs, reset on non-list) | `empty-bullet.test.ts` | 8 of 14 | renderable-content ×3, label continuation ×2, reset, suppression ×2 |
| `Bullet::Blip` compilation (`DrawImage`, `size_pts`/`size_pct`, gutter clearance, wordArtVert) | `bullet-image.test.ts` | 7 of 14 | marker paint, size (%), size (pt), preference, first-line clearance, wordArtVert ×2 |
| `Bullet::Char::size_pts` / `size_pct` | `buszpts-bullet-size.test.ts` | 3 of 4 | absolute pts, percentage, pts-wins-over-pct |
| `Bullet::AutoNum::color` + resolved marker face/size | `autonum-bullet-color.test.ts` | 2 of 3 | `buClr`, authored face + percentage |
| RTL list-marker right-edge anchor (issue #930) | `rtl-bullet-indent.test.ts` | 2 of 3 | RTL char and auto-number markers |
| Highlight on `PositionedGlyphRun` / `paintHighlight` | `text-highlight.test.ts` | 6 of 6 | every case |
| `east_asian_default` module (`east_asian_default_faces`, `is_serif_latin_face`) + `a:ea` consumption in `text::emit` | `east-asian-default.test.ts` | 8 of 8 | every case |
| `vert`/`vrt2` font-feature model, FE14 semicolon tracking, per-glyph `letterSpacingPx` | `eavert-glyph-orientation.test.ts` | 4 of 23 | fullwidth-colon rotation, `vert` long-marks, fallback coverage, helper advance |
| Stacked cell pitch (`StackedGlyph::thickness`), glyph-framed strokes, inline equations, prefix cells for markers, `IfdImage` picture markers | `stacked-text.test.ts` | 32 of 36 | every case except 3 EXACT + 1 BEHAVIOURAL |
| Per-line warp envelope expansion + `a:lnSpc` between warp lines | `wordart-follow-path.test.ts` | 14 of 19 | multi-line arch/circle/radial, winding, mixed-size boxes, 50/100/150/200 % `lnSpc`, spcBef/spcAft, single measure |
| `Stroke::{head_end, tail_end}` + `Stroke::cmpd` → arrowhead geometry and shaft trim | `line-end-shaft-trim.test.ts` | 6 of 6 | every case |

### `text-flow-a` — 24 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `rPr@spc` letter spacing (`StyleDef::letter_spacing`, fold into `TextMeasure::advance`/`measure_items`, per-piece spacing on `PositionedGlyphRun`) | `tab-stop-spc.test.ts` (3), `spc-wrap.test.ts` (2), `spc-cluster-advance.test.ts` (12) | 17 | tab-stop-spc 1-3, spc-wrap 1-2, spc-cluster-advance 1-12 |
| RTL/bidi tab-cell mirroring (visual-order pass; mirrored stop `x`) | `pptx-multi-tab.test.ts` (1 of 4), `rtl-tab-stops.test.ts` (6 of 7) | 7 | pptx-multi-tab 2; rtl-tab-stops 1-4, 6-7 |

### `text-flow-b` — 23 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `fit_cjk_line(chars, start_width, max_width, rules)` (`BreakOptions` has no `start_width`) | `cjk-wrap.test.ts` | 1 of 10 | `fitCjkLine("、あ", 15, 20) == 0` |
| `rPr@spc` letter spacing | `cjk-spc-justify.test.ts` | 4 of 6 | cases 1-4 |
| `BreakOptions::kinsoku_across_runs` + language-slot seam (`run.lang`, `fontFamilyEa`) | `ea-line-break.test.ts` | 4 of 13 | ja-JP two-run seam, en-US seam, "decides by language", measured-language slot seam |
| `text::bidi::{segments_have_rtl, compute_line_visual_order}` (UAX #9 reorder, tab = Bidi_Class S, mirroring) | `bidi-line.test.ts` | 8 of 8 | every case |
| `sea_word_break_offsets` (ICU Thai/Lao/Khmer dictionary) | `sea-line-break.test.ts` (1 of 6), `sea-justified-fit.test.ts` (4 of 8) | 5 | dictionary-boundary break; justified-fit rules 1-2 + over-full control |
| `natural_width_exceeds_bbox` autofit preflight | `superscript-citation-wrap.test.ts` | 1 of 3 | the bbox-preflight case |

## Reconciliation notes

- `coverage/shapes-a.md` per-case table and its own row totals disagreed by one
  (`renderer-shape-image-fill` #12 "rejects an unbounded tile auxiliary" was
  listed N/A while the row counted 18 BLOCKED). It is now BLOCKED on the tiled
  image-fill sampler/scratch budget, so the file is `6 N/A / 22 BLOCKED` and the
  table above matches it. No test changed (the case has no stub).
- `text-flow-a`: `justify-bracket-overlap` case 4(b) `@spc` is mentioned in that
  report's prose as blocked but the case is counted BEHAVIOURAL (its geometry is
  asserted; only the `letterSpacing` string facet is unobservable). No double count.
- Test-function counts exceed ported-case counts where one spec case is split:
  `misc` `wordart-resource-bounds` (1 case, 2 tests), `elements`
  `table-text-selection` (+1 supplementary clip assertion), `text-box-b` 39
  ported cases → 45 tests.
- 93 blocked/N-A cases have `#[ignore]`d stubs; their bodies are documentation
  only (the ignore reason names the missing symbol).

## Follow-up (production, out of scope here)

Grouped symbol list, in rough dependency order: image-fill compile path
(`Fill::Image` → `DrawImage`, tile sampler + budget, decode planner, decode-error
surface); `rPr@spc` letter spacing end-to-end; `text::bidi` + RTL tab mirroring;
`a:ea`/`a:cs` slot dispatch + `east_asian_default`; `Bullet::Blip`/`size_pts`/
`size_pct`/`AutoNum::color` + list counters; glyph-framed strokes and highlight on
`PositionedGlyphRun`; `Stroke::{head_end,tail_end,cmpd}`; 3D bevel/reflection/
projected-effect geometry; `sea_word_break_offsets`; `cust_geom_paint`;
`rot_with_shape` + gradient strokes + pattern paint; multi-column bodies;
`apply_dim_overlay`; parse-error placeholder; SmartArt fallback contrast;
`fit_cjk_line`/`kinsoku_across_runs`; typed resource-limit errors off-wasm.
