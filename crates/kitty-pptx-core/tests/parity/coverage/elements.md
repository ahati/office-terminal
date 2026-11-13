# Parity coverage — batch `elements`

Reference: `/tmp/oooxml/packages/pptx/src/*.test.ts` @ `1f5c6e1d` (read-only).
Rust targets:
`crates/kitty-pptx-core/tests/parity_elements_tests.rs`,
`crates/kitty-pptx-core/tests/parity_elements_support/decode_plan_cases.rs` (its decode-plan module),
`.../parity_elements_table_tests.rs` (132), `.../parity_elements_chart_tests.rs` (316).
Verification: `CARGO_TARGET_DIR=target/agent-paint-a cargo test -p kitty-pptx-core
--test parity_elements_tests --test parity_elements_table_tests --test parity_elements_chart_tests
--test golden_tests` → **30 tests, 30 passing**; `cargo clippy -p kitty-pptx-core --lib --tests
-- -D warnings` clean.

`cases` counts runtime cases, i.e. every `it.each` row separately.

## Reason keys

- **R1** (closed by `rasterizer::decode_plan`) display-sized decode planner: `scene_decode_demands`
  derives the full-source device grid from frame × scale × `srcRect`, and
  `plan_decoded_image_targets` returns the `imageResources.resolution` `'display'` axes plus the
  2× native-admission supersample. `DecodePlan::size_hint` is the `AtMost`/`Native` seam the
  rasterizer now passes to the store; the master/target/layout *lease* (live-byte accounting across
  slides) is still absent.
- **R2** (closed by `rasterizer::decode_plan`) decoded-byte budget/admission: `ImageResourceOptions`
  (`decodedByteBudget`, `strategy`, `resolution`), the adaptive binary-searched uniform scale, and
  the typed `RenderError::DecodedImageLimit { metric: "active-decoded-bytes" }` crossing.
- **R3** per-image decode error surface (`TiffDecodeError`, `OptionalImageCodecUnavailableError`,
  archive/`OoxmlResourceLimitError`) absent: `render_image` fails soft on `AssetStore` errors.
- **R4** bitmap placeholder painter (`PICT image unsupported`, `TIFF image unavailable`) absent;
  media is a vector placeholder (`placeholders::compile_media_placeholder`), pictures drop.
- **R5** text-run selection info (`PptxTextRunInfo` / `TextRunCallback`, `shapeId`,
  `elementIndex`, `origin`, per-cell frame math) absent.
- **R6** multi-column bodies: `TextBody::num_col`/`spc_col`/`rtl_col` parse, but `text::emit`
  lays out one column and ignores them.
- **R7** non-background image fills: `compile_fill(Fill::Image)` → `None`, so table and chart
  blip fills never reach the display list (no shared blip cache, no cell-image clip).
- **R8** chart family/seam gaps: `ChartFamily::Bubble` is unsupported (placeholder), and there is
  no `ChartExRenderer` injection point.
- **R9** browser/canvas-only mechanics (`ImageBitmap`/`createImageBitmap`, canvas task-boundary
  recording, promise supersession).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| renderer-picture-raster-target.test.ts | 19 | 0 | 2 | 1 | 16 | `background_fill_rect_paints_the_cropped_destination_rectangle`, `decode_plan_cases::full_source_device_pixels_follow_frame_dpr_and_crop` | BEHAVIOURAL: (a) the *destination* half of "applies background fillRect to both decode sizing and cropped destination paint" — `fillRect {l:.1,t:-.1,r:.2,b:.05}` on a 9144000×6858000 slide at 960 CSS px paints exactly (96,−72,672,756); (b) "derives full-source device pixels from the frame, effective DPR, and srcRect crop" — `scene_decode_demands` expands the 960×720 device frame by the 50 % visible source width to 1920×720 and the default plan retains the 2× admission grid (3840×1440), the reference's own numbers. N/A R9: "paints nothing until all decodes settle…" (canvas chunking). BLOCKED R3/R4: SVG-effect routing, PICT label, 8 archive-failure sites, TIFF fatal/optional (×3), pixel-effect source grid (×2) |
| renderer-adaptive-image-budget.test.ts | 9 | 0 | 5 | 2 | 2 | `picture_raster_requests_the_native_source_grid_by_default`, `decode_plan_cases::{display_plan_requests_both_planned_raster_axes, one_bounded_quality_level_covers_every_raster, huge_picture_gets_a_bounded_supersampled_target, tiff_sources_stay_outside_the_raster_plan, strict_mode_surfaces_the_typed_quota_crossing}` | BEHAVIOURAL: native-if-fit default (one `SizeHint::Native` per `DrawImage`, native quadrants exact); the 2816-byte display plan (32×22); one uniform quality level (equal targets, total ≤ 1 MiB); the 109 MP source at the 2× supersampled 1920×2880 grid bounded by `MAX_RASTER_PIXELS`; TIFF outside the cap (no demand); strict mode's typed `active-decoded-bytes` crossing. N/A R9: decode pipelining, canvas supersession. BLOCKED R1/R3: the worker-decoded SVG display target (no SVG decode callback in `AssetStore`) and the live-byte lease |
| poster-decode-bomb.test.ts | 6 | 0 | 0 | 0 | 6 | — | BLOCKED R2/R4: every case needs a poster raster path (`getPosterBitmap`) with header-sniffed decode sizing, the shared live-byte ceiling, and the fail-open SVG branch. `MediaElement` is compiled to a vector media placeholder; `poster_path` is never painted |
| tile-fill.test.ts | 12 | 0 | 12 | 0 | 0 | `tile_algn_{tl,t,tr,l,ctr,r,bl,b,br}_anchors_at_*`, `tile_unknown_algn_defaults_to_top_left`, `tile_duplicates_the_cropped_window_at_its_cropped_native_extent`, `tile_retains_negative_source_outsets_as_transparent_extent` | All 9 `algn` rows (0/30/60 × 0/35/70 px), the unknown→`tl` default, and both `tileSourceExtent` results (480×420, and 1000×600 for negative outsets). Each `algn` case asserts a 2×2 corner marker at the exact device pixel *and* the exact EMU pattern phase (`x = ax·9525`, `y = ay·9525`) of the serialized `<pattern>` |
| multi-column-overflow.test.ts | 2 | 0 | 0 | 0 | 2 | — | BLOCKED R6: both cases drive `bodyPr@numCol="2"` and assert which column each paragraph lands in (`x` of the `fillText` calls). `text::emit` never reads `num_col`/`spc_col` |
| tabular-text.test.ts | 4 | 0 | 0 | 4 | 0 | — | N/A R9: `tabularDigitWidth`/`tabularTextWidth`/`drawTabularText` are Canvas overlay helpers for the browser clock; the port draws its timer as Ratatui monospace cells, where the proportional-digit jitter cannot occur |
| renderer-table-image-fill.test.ts | 3 | 0 | 0 | 0 | 3 | — | BLOCKED R7 (+R2 for the budget case): cell/table `Fill::Image` is dropped by `compile_fill`, so there is no shared blip decode, no "full background before alpha bands" paint order, no `srcRect` cell clip, and no table-only plan |
| table-text-selection.test.ts | 14 | 0 | 6 | 1 | 7 | `table_cell_clips_for_model_direction_horz`, `table_cell_clips_for_padded_horz_direction`, `table_cell_does_not_clip_for_vertical_model_direction`, `table_cell_does_not_clip_when_nbsp_wraps_horz`, `table_cell_does_not_clip_for_internal_whitespace_horz`, `table_cell_does_not_clip_for_unknown_model_direction` (+ `table_cell_horz_overflow_disables_the_horizontal_clip`, a supplementary assertion from the same block, not a spec case) | BEHAVIOURAL: the 6 representable `vert` rows of `uses model direction %j for horizontal clipping`, observed as the emitted `PushClip` (the port's form of `cellHorzOverflow === 'clip'`): `horz`, `' \t horz\r\n'` → clip; `vert`, `'\u00a0horz\u00a0'`, `'ho\trz'`, `unknown` → no clip. N/A: the `undefined` row — `TextBody::vert` is a required `String` (an absent `bodyPr@vert` is folded to `horz` at parse time), so "absent" has no model representation. BLOCKED R5: cell identity/geometry in row-major order, empty cells as gaps, "skips selection transform math", rotation+`flipH`, `flipV`, 30° frame mapping, element identity through the pipeline (7) |
| chart-nested-emit.test.ts | 3 | 3 | 0 | 0 | 0 | `rich_combo_chart_nested_emit_lands_on_the_typed_core_fields`, `sparse_pie_chart_nested_emit_is_nullish_equivalent_to_its_pruned_form`, `null_data_points_inside_series_values_survive_pruning` | EXACT: the frozen legacy literal's field-name map is the port's serde wire map, so the oracle comparison becomes typed-field equality after `stripNullish`. Rich fixture pins all six renamed axis-title run props (1000/true/`404040`, 1100/false/`595959`), the pass-through set, the secondary axis and both series; the sparse fixture pins `null` ≡ absent (`stripNullish` fixed point, pruned keys gone, no `catAxisTitleSize` key); `[120,null,240]` keeps its gap. `plotAreaManualLayout` is omitted from the fixture because `ChartManualLayout` requires `wMode`/`hMode`, which the reference literal does not carry |
| renderer-chart-bubble3d.test.ts | 2 | 0 | 0 | 0 | 2 | — | BLOCKED R8: `bubble3D` needs the bubble renderer (3 radial gradients with 4/5/6 stops, 3 `source-atop` composites, frame rotation π/2) and the `ChartExRenderer` pass-through. `ChartFamily::Bubble` paints the "unsupported chart" placeholder |
| renderer-chart-picture-marker.test.ts | 15 | 0 | 0 | 0 | 15 | — | BLOCKED R1/R2/R7/R8: every case preloads `markerFillPaint`/`chartFill` document images through the shared decode plan (cache warm before the synchronous chart paint, crop- and DPR-aware dedup targets, SVG-twin vs raster, duotone fail-closed, tiled-vs-stretched source retention, invalid frame rejection). The chart compiler never paints image fills |

**Total: 89 cases → 3 EXACT + 25 BEHAVIOURAL = 28 ported (all passing), 8 N/A, 53 BLOCKED.**
26 Rust test functions (20 elements + 7 table + 3 chart, plus the 6 decode-plan cases inside
`parity_elements_tests`); 1 supplementary assertion. The `decode_plan_cases` module lives in
`tests/parity_elements_support/decode_plan_cases.rs`.
