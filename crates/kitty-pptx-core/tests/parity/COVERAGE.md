# Consolidated parity coverage (core test port)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only).
Scope: every `parity_*` test target under `crates/kitty-pptx-core/tests/` plus the one
`parity_*` target in `crates/kitty-pptx-tui/tests/` (the viewer batch's nav half).
Sources merged: `tests/parity/coverage/{elements,fonts,misc,shapes-a,shapes-b,text-box-a,text-box-b,text-flow-a,text-flow-b,viewer}.md`.

Verification (wave-2 close, `CARGO_TARGET_DIR=target/agent-verify`):

```
cargo test -p kitty-pptx-core --test <target>   # one target at a time, all 32 core targets
cargo test -p kitty-pptx-tui  --test parity_viewer_nav_tests
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
```

Result: **33/33 targets compile, 422 tests, 422 pass, 0 ignored, 0 failed; clippy clean.**
103 blocked cases are archived under `tests/parity/pending/` (not compiled); every
compiled test runs. Current verification: `cargo test --workspace` is green (2024 tests,
0 failed, 0 ignored) and `cargo clippy --workspace --all-targets -- -D warnings` is clean.
The wave-2 homography failure noted below was resolved by its owner (the test is now
`a_degenerate_quad_matches_the_reference`). No golden output changed.

`cases` counts reference runtime cases (every `it.each` row / loop expansion once).
`pass`/`fail` count Rust test functions in the batch's targets. The compiled tree has no
`#[ignore]`: blocked cases are archived under `tests/parity/pending/` and every compiled
test runs.

## Summary

| batch | reference files | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | pass | fail |
| :--- | --: | --: | --: | --: | --: | --: | --: | --: |
| `elements` | 11 | 89 | 3 | 25 | 8 | 53 | 30 | 0 |
| `fonts` | 13 | 104 | 9 | 13 | 78 | 4 | 22 | 0 |
| `misc` | 11 | 37 | 1 | 8 | 8 | 20 | 10 | 0 |
| `shapes-a` | 5 | 36 | 6 | 15 | 6 | 9 | 24 | 0 |
| `shapes-b` | 8 | 66 | 9 | 6 | 3 | 48 | 22 | 0 |
| `text-box-a` | 6 | 112 | 22 | 32 | 1 | 57 | 41 | 0 |
| `text-box-b` | 11 | 129 | 55 | 11 | 6 | 57 | 72 | 0 |
| `text-flow-a` | 9 | 70 | 44 | 8 | 0 | 18 | 53 | 0 |
| `text-flow-b` | 15 | 86 | 54 | 17 | 0 | 15 | 68 | 0 |
| `viewer` | 72 | 455 | 11 | 44 | 153 | 247 | 53 | 0 |
| **total** | **161** | **1184** | **214** | **179** | **263** | **528** | **395** | **0** |

Every batch row is self-consistent (`EXACT + BEHAVIOURAL + N/A + BLOCKED == cases`).
393 of 1184 cases are ported and asserted (214 EXACT + 179 BEHAVIOURAL); 102 further
cases are blocked, and their stubs are archived under `tests/parity/pending/` (not
compiled) with the missing symbol in each archive header.

`pass` counts batch test functions that ran green; every compiled test runs, so the
per-target table's `ignored` column is 0 throughout.

## Targets and run results

| batch | target | tests | pass | ignored | fail |
| :--- | :--- | --: | --: | --: | --: |
| `elements` | `parity_elements_tests` | 20 | 20 | 0 | 0 |
| `elements` | `parity_elements_table_tests` | 7 | 7 | 0 | 0 |
| `elements` | `parity_elements_chart_tests` | 3 | 3 | 0 | 0 |
| `fonts` | `parity_fonts_tests` | 12 | 12 | 0 | 0 |
| `fonts` | `parity_fonts_slot_tests` | 10 | 10 | 0 | 0 |
| `misc` | `parity_misc_tests` | 10 | 10 | 0 | 0 |
| `shapes-a` | `parity_shapes_a_tests` | 24 | 24 | 0 | 0 |
| `shapes-b` | `parity_shapes_b_tests` | 22 | 22 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_tests` | 10 | 10 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_spacing_tests` | 19 | 19 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_autofit_tests` | 12 | 12 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_tests` | 24 | 24 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_bullet_image_tests` | 10 | 10 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_highlight_tests` | 7 | 7 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_eavert_tests` | 19 | 19 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_stacked_tests` | 7 | 7 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_warp_tests` | 5 | 5 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests` | 20 | 20 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_tabs` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_wrap` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_draw_tests` | 7 | 7 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_tests` | 35 | 35 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_bidi_tests` | 15 | 15 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_sea_tests` | 12 | 12 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_indent_tests` | 6 | 6 | 0 | 0 |
| `viewer` | `parity_viewer_nav_tests` (core) | 13 | 13 | 0 | 0 |
| `viewer` | `parity_viewer_nav_tests` (tui) | 12 | 12 | 0 | 0 |
| `viewer` | `parity_viewer_selection_tests` | 4 | 4 | 0 | 0 |
| `viewer` | `parity_viewer_lifecycle_tests` | 9 | 9 | 0 | 0 |
| `viewer` | `parity_viewer_worker_tests` | 9 | 9 | 0 | 0 |
| `viewer` | `parity_playwright_tests` | 6 | 6 | 0 | 0 |
| | **batch subtotal (31 targets)** | **395** | **395** | **0** | **0** |

Out-of-batch `parity_*` targets (pre-existing, tracked in git, no coverage report):

| target | tests | pass | ignored | fail | note |
| :--- | --: | --: | --: | --: | :--- |
| `parity_fidelity_tests` | 1 | 1 | 0 | 0 | VRT deck fidelity (`parity_support::vrt::DECKS`) |
| `parity_text_tests` | 26 | 26 | 0 | 0 | lead's text kernel suite; the 1 blocked case (`bulleted_first_line_ignores_the_positive_indent`, "not expressible: `BreakOptions` has no bullet flag") is archived in `tests/parity/pending/parity_text_tests.rs.txt` |
| | **grand total (33 targets)** | **422** | **422** | **0** | **0** |

Non-target modules: `parity_support/{mod,deck,pixelmatch,scores,vrt}.rs`,
`parity_text_flow_a_support/mod.rs`,
`shapes_a_support/{mod,image_fill_cases,image_fill_paint_cases,gradient_cases}.rs`,
`shapes_b_support/{line_end_cases,bevel_effect_geometry}.rs`, `text_box_a_support/mod.rs`.

## BLOCKED cases and the missing API

528 cases are blocked. Each row enumerates the blocked cases of one reference file
(or a named subset) and the symbol that would unblock them. `n of m` means the
complement of the file's ported + N/A cases.

### `elements` — 53 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| Per-image decode error surface (`TiffDecodeError`, optional-codec, archive `OoxmlResourceLimitError`) and the bitmap placeholder painter | `renderer-picture-raster-target.test.ts` | 16 of 19 | all but the ported `fillRect` destination + device-pixel-derivation cases and the canvas-chunking N/A: SVG-effect routing, `PICT image unsupported` label, 8 archive-failure sites, TIFF fatal/optional (×3), pixel-effect source grid (×2) |
| Worker-decoded SVG display target (`svgDecoder` callback off the `AssetStore` seam) and the live-byte lease across slides | `renderer-adaptive-image-budget.test.ts` | 2 of 9 | the SVG display-target row and the superseded-canvas row; the decode planner, budget, adaptive quality level, 109 MP bound, TIFF exclusion and strict quota are ported |
| Poster raster path (`getPosterBitmap`) with header-sniffed sizing + shared live-byte ceiling | `poster-decode-bomb.test.ts` | 6 of 6 | every case; `MediaElement` compiles to a vector placeholder and `poster_path` is never painted |
| Multi-column body (`TextBody::num_col`/`spc_col` consumed by `text::emit`) | `multi-column-overflow.test.ts` | 2 of 2 | both `bodyPr@numCol="2"` cases |
| Table-only blip plan (shared decode + "full background before alpha bands" paint order + `srcRect` cell clip). `compile_fill(Fill::Image)` now yields `VectorPaint::Image`, so a cell image fill reaches the display list; the table-only ordering/plan and their assertions are still unwritten | `renderer-table-image-fill.test.ts` | 3 of 3 | shared blip decode, paint order, `srcRect` cell clip |
| Text-run selection info (`PptxTextRunInfo`: `shapeId`, `elementIndex`, `origin`, per-cell frame math) | `table-text-selection.test.ts` | 7 of 14 | cell identity/geometry, empty cells as gaps, selection transform math, rotation+`flipH`, `flipV`, 30° frame mapping, element identity through the pipeline |
| `ChartFamily::Bubble` renderer + `ChartExRenderer` injection point | `renderer-chart-bubble3d.test.ts` | 2 of 2 | both cases (`bubble3D` radial gradients, `source-atop` composites) |
| Chart image-fill pipeline (marker/fill preload before the synchronous chart paint, crop+DPR dedup, SVG twin, duotone fail-closed, invalid-frame rejection). `compile_fill(Fill::Image)` now yields the chart brush, but the preload/dedup plan and its assertions are unwritten | `renderer-chart-picture-marker.test.ts` | 15 of 15 | every case |

### `fonts` — 4 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| Typed resource-limit error taxonomy for embedded fonts | `embedded-fonts.test.ts` | 1 of 9 | "preserves other typed resource-limit errors" |
| Typed (non-aborting) load rejection from `pptx_parser::{extract_font, PptxArchive}` | `embedded-fonts-load.test.ts` | 3 of 3 | limit `4096`/observed `4097`, face release, rejection surface |

### `misc` — 20 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `apply_dim_overlay(&mut VectorSlide, color, opacity)` | `dim-overlay.test.ts` | 1 of 1 | the single overlay case |
| `Slide::parse_error` placeholder emission + `RenderOptions::error_placeholder` | `renderer.parse-error.test.ts` | 4 of 7 | placeholder title/detail variants over a parse-error slide |
| `background_luminance`, `smart_art_fallback_text_color`, `is_smart_art_fallback_shape`, run identity (`shape_id`/`element_index`/`origin`), theme + caller `defaultTextColor`, hyperlink text colour | `smartart-fallback-contrast.test.ts` | 15 of 19 | all contrast branches except the ported `ED7D31`/dark-background/black-default/explicit-run-colour cases |

### `shapes-a` — 9 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `ShapeElement::cust_geom_paint` (`PathPaint`) consumed by `build_shape_paths` | `custgeom-path-paint.test.ts` | 1 of 2 | per-path `fill`/`stroke` flags |
| ~~Gradient options in the IR (`VectorGradient::rot_with_shape`, gradient paint on `VectorStroke`)~~ **landed** (`paint-b`) | `renderer.gradient.test.ts` | 0 of 3 | closed: `GradientFrame` + `VectorStroke::paint` |
| Equation-fallback branch selection (`ShapeElement::math_fallback`, `Paragraph::math_fallbacks`, `TableElement::math_fallback`) | `renderer-shape-image-fill.test.ts` | 8 of 27 | #16, #21-27; the image-fill rows (#2-8, #10-15, i.e. 13 BEHAVIOURAL) and `pattern-thumbnail` are ported (per-case table in `coverage/shapes-a.md`) |

### `shapes-b` — 48 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| ~~`build_bevel_inputs`/`bevel_band_px` without the 48 px clamp and with a `w>0 && h>0` gate~~ **landed** (`3d`: `scene3d::bevel_inputs`) | `bevel-band-conversion.test.ts` | 0 of 4 | closed: devScale 1/4/8 at the API + the zero-size omission |
| `reflection_blur_bands(bounds, max_blur)` | `reflection-blur.test.ts` | 3 of 3 | distance-aware radius ramp, band clipping, 24-pass cap |
| ~~`projected_effect_geometry` + `VectorScene3d::{zoom, rot{lat,lon,rev}}`~~ **landed** (`3d`: `scene3d::{camera,homography,projection}`) | `shape-effect-transform.test.ts` | 0 of 9 | closed: cases 3-6 (rear-plane selection, device-px pad, camera-crossing face) |
| Homography warp + rear plane + wall sweep + rig/material compositor and a PNG oracle (`draw_project`, `apply_extrusion`, `bevel-light-rigs`, wireframe rims). The *geometry* now exists (`scene3d::camera`, `scene3d::projected_effect_geometry`, `bevelB`/`prst_material` in the IR) | `shape-orthographic-bevel.test.ts` | 38 of 39 | every case except the ported inner-shadow outline; the back-bevel case's geometry half is pinned by `bottom_bevel_inputs_sit_at_the_extrusion_base_behind_the_front_face` |
| `reflected_shape_text_rotation(rotation, flip_h, flip_v)` | `shape-text-reflection.test.ts` | 7 of 7 | all `it.each` rows |

### `text-box-a` — 57 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `powerPointAscentShare` face catalogue + `OBSERVED_SUBSTITUTES`, `SHARE_CACHE_LIMIT` LRU, `excelDrawingMlLineRatios` (`compatLnSpc="0"`), `powerPointFontAlgn*`, `a:br`/`endParaRPr` face accounting | `powerpoint-line-metrics.test.ts` | 49 of 60 | A2-A3, A10, B2-B5, C1-C2, C3-C6 |
| `natural_width_exceeds_bbox` preflight, `a:ea` metric slot, tab start pen `= marL + textXOffset` | `firstline-indent-autofit.test.ts` | 4 of 12 | F1, F3, F4, F8 |
| Resolved canvas font box (`useResolvedFontMetrics`) | `sp-autofit-top-anchor.test.ts` | 4 of 19 | S7-S9 plus the block's remaining resolved-font-box assertion |

### `text-box-b` — 57 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `powerPointFaceMetrics` glyph box for the #1896 highlight height | `text-highlight.test.ts` | 1 of 6 | the usWin-ascent glyph-box row; style-identity split/coalesce and the `top = baseline − 0.85 em`, `h = 1.1 em` rect are ported |
| `vert`/`vrt2` font-feature model, FE14 semicolon tracking, per-glyph `letterSpacingPx` | `eavert-glyph-orientation.test.ts` | 4 of 23 | fullwidth-colon rotation, `vert` long-marks, fallback coverage, helper advance |
| Stacked cell pitch (`StackedGlyph::thickness`), glyph-framed strokes, inline equations, prefix cells for markers | `stacked-text.test.ts` | 32 of 36 | every case except 3 EXACT + 1 BEHAVIOURAL; picture markers now paint (`decor::push_marker_image`) |
| Per-line warp envelope expansion + `a:lnSpc` between warp lines | `wordart-follow-path.test.ts` | 14 of 19 | multi-line arch/circle/radial, winding, mixed-size boxes, 50/100/150/200 % `lnSpc`, spcBef/spcAft, single measure |
| ~~`Stroke::{head_end, tail_end}` + `Stroke::cmpd` → arrowhead geometry and shaft trim~~ **landed** (`paint-b`: `compiler::style::{line_end,stroke}`) | `line-end-shaft-trim.test.ts` | 6 of 6 | every case's behaviour is implemented and 5 are pinned by supplementary tests in `parity_shapes_b_tests`; the 6 ledger cases stay here (no double count) because this target's own stubs still need their fixture (`9525.0` floats) and `shaft_ends` recorder (skips decorations) fixed by its owner |

(`east-asian-default.test.ts`, 8 cases, is no longer counted here: it moved to the
`fonts` ledger, where 6 are EXACT and 2 BEHAVIOURAL.)

### `text-flow-a` — 18 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `rPr@spc` letter spacing (`StyleDef::letter_spacing`, fold into `TextMeasure::advance`/`measure_items`, per-piece spacing on `PositionedGlyphRun`) | `tab-stop-spc.test.ts` (3), `spc-wrap.test.ts` (2), `spc-cluster-advance.test.ts` (12) | 17 | tab-stop-spc 1-3, spc-wrap 1-2, spc-cluster-advance 1-12 |
| RTL/bidi tab-cell mirroring in `pptx-multi-tab.test.ts` (visual-order pass; mirrored stop `x`) — the 7 `rtl-tab-stops` cases it shares are ported in `text-flow-b` | `pptx-multi-tab.test.ts` | 1 of 4 | case 2 |

### `text-flow-b` — 15 blocked

| missing API | reference file | blocked | cases |
| :--- | :--- | --: | :--- |
| `fit_cjk_line(chars, start_width, max_width, rules)` (`BreakOptions` has no `start_width`) | `cjk-wrap.test.ts` | 1 of 10 | `fitCjkLine("、あ", 15, 20) == 0` |
| `rPr@spc` letter spacing | `cjk-spc-justify.test.ts` | 4 of 6 | cases 1-4 |
| `BreakOptions::kinsoku_across_runs` + language-slot seam (`run.lang`, `fontFamilyEa`) | `ea-line-break.test.ts` | 4 of 13 | ja-JP two-run seam, en-US seam, "decides by language", measured-language slot seam |
| `sea_word_break_offsets` (ICU Thai/Lao/Khmer dictionary) | `sea-line-break.test.ts` (1 of 6), `sea-justified-fit.test.ts` (4 of 8) | 5 | dictionary-boundary break; justified-fit rules 1-2 + over-full control |
| `natural_width_exceeds_bbox` autofit preflight | `superscript-citation-wrap.test.ts` | 1 of 3 | the bbox-preflight case |

### `viewer` — 247 blocked

The reference viewer is a DOM application, so its blocked cases are almost all
`<canvas>`/`Promise`/`ImageBitmap` surfaces. Per-file rows and the full stub→symbol
table live in `coverage/viewer.md`; grouped by the missing API:

| missing API | blocked | reference files |
| :--- | --: | :--- |
| Hidden-slide navigation (`HiddenSlideMode`, `next_visible_index`/`resolve_visible_index`, `setHiddenSlideMode`, `countVisible`, hidden-aware layout) | 17 | `viewer-hidden`, `viewer-hidden-skip`, `scroll-viewer-hidden` |
| Viewer zoom (`PresentationState::{set_scale,zoom_in,zoom_out,fit_width,fit_page}`, per-scale slot cache, pre-load latch, scroll anchor) | 23 | `viewer-zoom-contract`, `scroll-viewer-zoom-contract`, `tests/visual/scroll-zoom-settle` |
| Element/text selection + find (`hit_test_slide_context`, `hit_test_slide_element`, `limit_element_context`, `PptxElementContext`, `PptxFindController`, `paint_find_highlights`) | 31 | `element-selection`, `find`, `find-highlight-layer`, `presentation-element-context`, `viewer-*selection-context`, `viewer-find-destroy`, `tests/visual/worker-selection-equivalence` |
| Internal-link delegation (`on_hyperlink_click` hand-off, scroll-mode resolution) | 2 | `viewer-internal-nav`, `scroll-viewer-internal-nav` |
| Worker pull protocol + init timeout (`load_slide_from_cursor`, `BoundedPullSession`, `pull_with_credit_retry`, worker init timeout, render rejection, embedded-font registration) | 61 | `slide-pull-worker`, `slide-pull-client`, `worker-init-hang`, `render-worker-init-hang`, `presentation-destroy`, `viewer-concurrent-load`, `scroll-viewer-concurrent-load`, `viewer-destroy-during-render`, `viewer-render-error` |
| Progressive load + reload/orphan tracking | 17 | `viewer-progressive`, `presentation-progressive`, `scroll-viewer-progressive`, `viewer-reload-orphan`, `scroll-viewer-reload-orphan` |
| Container / part failures (CFB guard, `decrypt_ooxml`, `WarningEvent::MediaFailed`, placeholder painting for a broken slide, bounded structural-JSON projection, `loadPptxSlideFromCursor`, layout-error routing, no PDF backend) | 28 | `presentation-cfb-guard`, `presentation-decrypt`, `presentation-handle-media-errors`, `presentation.image`, `presentation-bounded-pipeline`, `viewer-part-failures`, `presentation-part-failures`, `viewer-layout-error-routing`, `slide-cursor-operation`, `tests/visual/{part-failure,pdf-acceptance}` |
| Preflight / load options / markdown projection (`PresentationPreflightBuilder`, `collect_deck_fonts`, `LoadOptions.fonts`, `to_markdown`, font-source demand facts) | 25 | `presentation-preflight`, `presentation-font-options`, `presentation.markdown`, `viewer-model-source-options`, `tests/visual/font-source-worker-equivalence` |
| `scroll-viewer` wrapper features (render epoch/stale bitmap, virtualization, comment cards, padding gutters, page shadow) | 38 | `scroll-viewer` |
| Pixel-oracle acceptance specs (table image/pattern fill plan, rotated-tile coverage, inline-equation fallback, bevel rim) | 5 | `tests/visual/{table-image-fill,rotated-tile-coverage,inline-equation-fallback,bevel-ring}` |

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
  `table-text-selection` (+1 supplementary clip assertion), `text-box-b` 66
  ported cases → 72 tests.
- 102 blocked/N-A cases are archived under `tests/parity/pending/`
  (`<target-stem>.rs.txt`, not compiled); their bodies are documentation only and the
  archive header names the missing symbol. Every compiled test runs.
- `east-asian-default.test.ts` (8 cases) moved from the `text-box-b` ledger to
  `fonts`: the API it needed (`text::east_asian_default` + the `a:ea`/`a:cs` slot
  dispatch) landed in the slot pass, and the ported cases live in
  `parity_fonts_slot_tests.rs`. The 8 blocked stubs for the same spec stay that
  target's and are archived in
  `tests/parity/pending/parity_text_box_b_eavert_tests.rs.txt`; four of them would
  pass if reinstated as-is and four compare against their own `None`-returning
  scaffolding (`serif_latin_face`, `ea_default_first`, …, archived with them), so
  they need their owner to wire the helpers before they can be enabled.
- `rtl-tab-stops.test.ts` (7 cases) moved from `text-flow-a` to `text-flow-b`
  (`parity_text_flow_b_bidi_tests.rs`); `text-flow-a`'s `c25_*` test still
  asserts that spec's case-5 kernel half.

## Follow-up (production, out of scope here)

Grouped symbol list, in rough dependency order: ~~image-fill compile path
(`Fill::Image` → `Paint::Image`, tile sampler + budget, decode planner)~~ (landed by `paint-a`:
`VectorPaint::{Image, Pattern}`, `rasterizer::image_fill`, `rasterizer::decode_plan`,
`svg::image` paint servers, `compiler::pattern`; the decode-error/placeholder surface is still open); `rPr@spc` letter spacing end-to-end; ~~`text::bidi` + RTL tab mirroring~~ (landed);
~~`a:ea`/`a:cs` slot dispatch + `east_asian_default`~~ (landed; `a:sym` still open); `Bullet::Blip`/`size_pts`/
`size_pct`/`AutoNum::color` + list counters; glyph-framed strokes and highlight on
`PositionedGlyphRun`; `Stroke::{head_end,tail_end,cmpd}`; 3D bevel/reflection/
projected-effect geometry; `sea_word_break_offsets`; `cust_geom_paint`;
`rot_with_shape` + gradient strokes + pattern paint; multi-column bodies;
`apply_dim_overlay`; parse-error placeholder; SmartArt fallback contrast;
`fit_cjk_line`/`kinsoku_across_runs`; typed resource-limit errors off-wasm.

## Reconciliation: `paint-b` (gradient extras + stroke paint + line ends)

- `shapes-a` `renderer.gradient` cases 1 and 2 are un-ignored and asserted
  (the counter-rotation now lives in `GradientFrame`; the line's complete gradient
  paint in `VectorStroke::paint`). 2 BLOCKED → 2 EXACT; the batch row and totals move
  by +2 EXACT / −2 BLOCKED, +4 tests, −1 ignored.
- `shapes-b` gained 5 tests but no ledger cases: they are supplementary ports of
  `line-end-shaft-trim.test.ts`, whose 6 cases stay in the `text-box-b` ledger so no
  case is double counted. `parity_text_box_b_highlight_tests`' 6 stubs are archived
  (`tests/parity/pending/parity_text_box_b_highlight_tests.rs.txt`) for the reasons
  above; they are now blocked on that file, not on production code.
- Rendering impact: `parity_shapes_a_tests`, `parity_shapes_b_tests`, `parity_misc_tests`,
  `parity_text_box_b_highlight_tests`, `stroke_gradient_tests`, `vector_pipeline_tests`
  and `golden_tests` all pass; no golden output changed (`UPDATE_GOLDENS` not run).

## Reconciliation: `3d` (bevel inputs / camera geometry / effect bounds)

* `shapes-b`: `bevel-band-conversion` 2 BLOCKED → 4 EXACT,
  `shape-effect-transform` 4 BLOCKED → 4 EXACT, and
  `scene3d-transform-order.test.ts` (1 case) enters the ledger as a new row.
  Batch row moves from `65/0/8/3/54` to `66/9/6/3/48` (−6 BLOCKED, −2 BEHAVIOURAL,
  +9 EXACT, +1 case); totals move by the same delta
  (+1 reference file, +1 case, +9 EXACT, −2 BEHAVIOURAL, −6 BLOCKED).
* Tests: `parity_shapes_b_tests` 13 → 22 passing, 0 ignored, 0 failed. Nine new
  tests, eight of them ports of the reference cases above and one (raster band
  clamp) a regression guard for the closed divergence.
* Production: `scene3d::{camera,homography,projection,bevel_inputs}` plus
  `compiler::shape_effects::split_scene3d_shape_transform`; new IR fields
  `VectorScene3d::{zoom,rot}`, `VectorRot3d`, `VectorShape3d::{bevel_bottom_*,
  prst_material}`. `tests/shape_effects_tests.rs` needed the two new IR fields in
  its struct literals (compile-only change, assertions untouched).
* Rendering: no golden changed (`UPDATE_GOLDENS` not run; no committed deck has
  `a:sp3d`). The raster bevel band lost its 48 px clamp and now uses `bevelT@w`
  with the `w > 0 && h > 0` gate; `coverage/shapes-b.md` records the still-open
  integrations (paint path still affine / still crops from path bounds, no
  rear-plane or rig/material compositor).

## Wave 2 — consolidated close

Verified with `CARGO_TARGET_DIR=target/agent-verify` (all 32 core `parity_*`
targets one at a time + the TUI target, then `cargo test --workspace
--no-fail-fast` and `cargo clippy --workspace --all-targets -- -D warnings`).

### BLOCKED before → after

| batch | cases | BLOCKED before | BLOCKED after | delta | what moved |
| :--- | --: | --: | --: | --: | :--- |
| `elements` | 89 | 53 | 53 | 0 | — |
| `fonts` | 104 | 4 | 4 | 0 | east-asian-default (8) moved in from `text-box-b`: 6 EXACT + 2 BEHAVIOURAL |
| `misc` | 37 | 20 | 20 | 0 | — |
| `shapes-a` | 36 | 13 | 9 | −4 | `renderer-shape-image-fill` re-classed from its own per-case table (13 BEHAVIOURAL / 8 BLOCKED, not the row's 10/11): the tile sampler and budget now land |
| `shapes-b` | 66 | 48 | 48 | 0 | — |
| `text-box-a` | 112 | 57 | 57 | 0 | — |
| `text-box-b` | 129 | 84 | 57 | −27 | `empty-bullet` 8, `bullet-image` 7, `buszpts` 3, `autonum-color` 2, `rtl-bullet-indent` 2, `text-highlight` 5 |
| `text-flow-a` | 70 | 18 | 18 | 0 | — |
| `text-flow-b` | 86 | 15 | 15 | 0 | — |
| **9-batch total** | **729** | **312** | **281** | **−31** | +25 EXACT, +6 BEHAVIOURAL, +31 ported |
| `viewer` (new) | 455 | — | 247 | +247 | first enumeration: 72 reference files, 11 EXACT / 44 BEHAVIOURAL / 153 N/A |
| **combined** | **1184** | **312** | **528** | **+216** | the +247 is a newly opened ledger, not a regression |

### Remaining blockers (528)

Full per-file rows and `n of m` splits are in the sections above; this is the
short list of what actually unblocks them, in rough dependency order:

1. **Equation fallbacks** — `ShapeElement::math_fallback`, `Paragraph::math_fallbacks`,
   `TableElement::math_fallback` (shapes-a 8, elements/text-box-b inline-equation rows).
2. **Decode / placeholder surface** — per-image decode error taxonomy
   (`TiffDecodeError`, optional codec, archive `OoxmlResourceLimitError`), the
   bitmap placeholder painter, the poster raster path, the SVG display-target
   callback and the cross-slide live-byte lease (elements 16+2+6, viewer part-failure rows).
3. **`powerPointFaceMetrics` / `powerPointAscentShare`** — the installed-face
   catalogue, `OBSERVED_SUBSTITUTES`, `SHARE_CACHE_LIMIT`, `excelDrawingMlLineRatios`,
   `powerPointFontAlgn*` (text-box-a 49, text-box-b 1).
4. **`natural_width_exceeds_bbox` preflight** (text-box-a 4, text-flow-b 1).
5. **`rPr@spc` letter spacing exposed on the draw IR** — the metric is folded into
   `Measurer::advance`, but `tab-stop-spc` 1-3, `spc-wrap` 1-2, `spc-cluster-advance`
   1-12 and `cjk-spc-justify` 1-4 assert canvas letter-spacing state (text-flow-a 17, text-flow-b 4).
6. **`sea_word_break_offsets`** (ICU Thai/Lao/Khmer dictionary) — text-flow-b 5.
7. **`fit_cjk_line(start_width)` + `BreakOptions::kinsoku_across_runs` + a `run.lang`
   seam** — text-flow-b 5.
8. **`cust_geom_paint`** (per-path fill/stroke) — shapes-a 1.
9. **3-D compositor** — bevel/extrusion wall sweep, light rigs, rear-plane compositor,
   PNG oracle, `reflection_blur_bands`, `reflected_shape_text_rotation` — shapes-b 48.
10. **Stacked/EA text geometry** — `StackedGlyph::thickness` cell pitch, glyph-framed
    strokes, prefix cells, `vert`/`vrt2` features, FE14 tracking, multi-line warp
    envelopes with `a:lnSpc` — text-box-b 32+4+14.
11. **Chart image fills + bubble3D** — preload/dedup plan, `ChartFamily::Bubble`,
    `ChartExRenderer` — elements 17.
12. **Multi-column bodies (`numCol`/`spcCol`)** — elements 2.
13. **`apply_dim_overlay`, parse-error placeholder, SmartArt fallback contrast** — misc 20.
14. **Typed resource-limit errors off-wasm** — fonts 4.
15. **Viewer platform surface** — hidden-slide mode (17), viewer zoom (23), element/text
    selection + find (31), worker pull protocol / init timeouts (61), container guards
    and media warnings (28), preflight + load options (25), progressive load (17),
    `scroll-viewer` wrapper (38), internal-link delegation (2), pixel-oracle specs (5).

### Merge notes (verifier)

- `text-box-b`: the batch report's 12-file table totalled 137 cases / 65 BLOCKED
  and included `east-asian-default` (8 cases), which is now owned by `fonts`.
  Both sides of that move are applied (batch report and this file), so the batch
  is `11 files / 129 cases / 57 BLOCKED` and `fonts` is
  `13 files / 104 cases / 4 BLOCKED` — no case is counted twice.
- `shapes-a`: the report's row split for `renderer-shape-image-fill` said
  `10 BEHAVIOURAL / 11 BLOCKED` while its per-case table enumerates
  `13 BEHAVIOURAL / 8 BLOCKED` (cases #2-8, #10-15 vs #16, #21-27). The per-case
  table is used and the batch report was corrected to match; the batch is now
  `6 EXACT / 15 BEHAVIOURAL / 6 N/A / 9 BLOCKED = 36`.
- `fonts`: the Totals line (104) was 8 cases above the sum of its listed rows
  (96) because the moved `east-asian-default` row was not listed. The row is now
  in `coverage/fonts.md` with its 6 EXACT + 2 BEHAVIOURAL, and the table sums to
  104.
- `text-flow-a` still carries 10 reference rows, one of which (`rtl-tab-stops`,
  7 cases) is marked `moved` to `text-flow-b`; it is excluded from the file count (9).
- `viewer` adds the only test target outside `kitty-pptx-core`
  (`crates/kitty-pptx-tui/tests/parity_viewer_nav_tests.rs`, 15 tests).

### Failure report (production, resolved after wave 2)

`cargo test --workspace` failed exactly one unit test at the wave-2 close:
`scene3d::homography::tests::a_degenerate_quad_has_no_map`
(`crates/kitty-pptx-core/src/scene3d/homography.rs:127`). The all-collinear quad
`[(0,0); 4]` has `dx3 == dy3 == 0`, so `unit_square_to_quad` takes the affine
branch, never reaches the `den` singularity guard, and returned `Some` — which
contradicted its own doc comment. The branch was owned by the `3d` pass: the test is
now `a_degenerate_quad_matches_the_reference` and `cargo test --workspace` is green.

## Wave 2.5 — zero-ignore policy (blocked stubs archived, not compiled)

Project rule: **a test that is included must run**. The compiled test tree now contains no
`#[ignore]`; every blocked case's source (doc comment, `#[test]` + `#[ignore = "<reason>"]`
attributes, body, and any helper used only by it) is archived as plain text under
`tests/parity/pending/<target-stem>.rs.txt` (core) and
`crates/kitty-pptx-tui/tests/parity/pending/` (tui), so cargo never compiles it. The
coverage rows above are unchanged: only the carrier moved.

| target | archived cases | tests before → after |
| :--- | --: | --: |
| `parity_playwright_tests` | 12 | 18 → 6 |
| `parity_viewer_nav_tests` (core) | 4 | 17 → 13 |
| `parity_viewer_nav_tests` (tui) | 3 | 15 → 12 |
| `parity_viewer_selection_tests` | 9 | 13 → 4 |
| `parity_viewer_lifecycle_tests` | 6 | 15 → 9 |
| `parity_viewer_worker_tests` | 3 | 12 → 9 |
| `parity_shapes_a_tests` | 1 | 25 → 24 |
| `parity_text_box_b_bullet_image_tests` | 4 | 14 → 10 |
| `parity_text_box_b_highlight_tests` | 7 | 14 → 7 |
| `parity_text_box_b_eavert_tests` | 14 | 33 → 19 |
| `parity_text_box_b_stacked_tests` | 25 | 32 → 7 |
| `parity_text_box_b_warp_tests` | 14 | 19 → 5 |
| `parity_text_tests` | 1 | 27 → 26 |
| **total** | **103** | **525 → 422** |

- 13 archives hold 103 blocked cases plus 11 scaffolding items used only by them: the five
  `east_asian_default` stubs and `default_face_runs`
  (`parity_text_box_b_eavert_tests`), and `EMU_PER_PX`, `LINE_EMU`, `line_shape`,
  `shaft_ends`, `expect_stops_at_vertex` (`parity_text_box_b_highlight_tests`).
- No non-blocked test body changed — every surviving assertion is byte-identical to the
  wave-2 close. The `pass` column is unchanged because ignored stubs never counted as
  passing; only the `tests` column moves.
- `parity_shapes_a_tests` and `parity_text_box_b_stacked_tests` also lost imports that only
  the archived cases used (`HyperlinkTarget`, `SlideCompiler`); `cargo clippy --workspace
  --all-targets -- -D warnings` is clean.
- Reinstatement procedure (Wave 3) and the per-archive counts:
  `tests/parity/pending/README.md`.
