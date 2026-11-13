# Coverage — batch `shapes-a`

Reference: `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d` (read-only).
Rust target: `cargo test -p kitty-pptx-core --test parity_shapes_a_tests`
(`CARGO_TARGET_DIR=target/agent-shapes-a`).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | ---: | ---: | ---: | ---: | ---: | :--- | :--- |
| `custgeom-path-paint.test.ts` | 2 | 1 | 0 | 0 | 1 | `parity_shapes_a_tests` | combined-path case asserted on the compiled `VectorSlide` (1 `DrawPath`, fill+stroke, three replayed contours, origin transform). Per-path `custGeomPaint` is dropped by production code. |
| `frame-transform.test.ts` | 3 | 3 | 0 | 0 | 0 | `parity_shapes_a_tests` | `it.each` table/chart/media rows collapse into one assertion set: the element type is only a TS parameter, all three rows call the same function with the same frame. Canvas `translate/rotate/scale` trace is asserted as the composed `AffineTransform` (ref `renderer.ts:7574-7589`). |
| `renderer.gradient.test.ts` | 3 | 2 | 1 | 0 | 0 | `parity_shapes_a_tests` (module `gradient_cases`) | Case 1 `rotWithShape="0"` asserted on the IR frame plus the projected axis `(0,50)→(100,50)` (and the rotating twin `(50,0)→(50,100)`), then painted (raster + SVG). Case 2 asserts the line's complete gradient paint (`VectorStroke::paint`, axis, raster direction, `stroke="url(#sgrad_1)"`). Case 3 (table cell) is the pre-existing behavioural port (one `LinearGradient`, never the `Solid` fallback, axis `(0,25)→(100,25)`). |
| `pattern-thumbnail.test.ts` | 1 | 0 | 1 | 0 | 0 | `pattern_cells_use_the_shared_point_grid` | BEHAVIOURAL: `Fill::Pattern` → `VectorPaint::Pattern { preset, fg, bg }` (never the solid fallback), the shared `PATTERN_CELL_EMU = 12700/8` step equals the reference's `(240/9_144_000)·12_700/8` matrix, and the SVG twin emits the 8 pt (`12700` EMU) cell with `1587.5` EMU pixels. The reference's `chartFill` (chart-area background) has no `pptx-model` field, so the chart half is unobservable. |
| `renderer-shape-image-fill.test.ts` | 27 | 0 | 13 | 6 | 8 | `parity_shapes_a_tests` (modules `image_fill_cases`, `image_fill_paint_cases`) | `Fill::Image` now compiles to `VectorPaint::Image` (srcRect/fillRect/tile/alpha/rotWithShape/DPI/`CT_Blip` chain) and paints inside the shape path; see the per-case table (13 BEHAVIOURAL vs 8 equation-fallback BLOCKED). |
| **total** | **36** | **6** | **15** | **6** | **9** | | 21 cases ported with assertions (6 EXACT + 15 BEHAVIOURAL); 24 tests pass, 0 ignored; the 1 blocked stub (`custGeom` per-path paint) is archived under `tests/parity/pending/` — the remaining 8 BLOCKED cases have no stub. |


## Findings (production gaps)

1. ~~`Fill::Image` on an ordinary shape is silently dropped.~~ **Closed** by this pass:
   `compile_shape_fill` resolves `a:blipFill` to `VectorPaint::Image` (§20.1.8.14 mode
   validity, `srcRect`, `fillRect`, `a:tile`, `alphaModFix`, `rotWithShape`, blip DPI and the
   ordered `CT_Blip` chain), `rasterizer::image_fill` paints it through a `tiny-skia`
   pattern shader (`Pad` for stretch, `Repeat` for tile) clipped by the shape's own path, and
   `svg::image` emits the equivalent `<pattern>` — so crop, insets, tiling, counter-rotation,
   opacity and effects all reach every backend.
2. **`ShapeElement::cust_geom_paint` (`pptx_model::PathPaint`) is parsed but unused**:
   `compiler::shapes::build_shape_paths` hard-codes `PathFill::Norm` / `stroke: true`
   for `custGeom`, so `<a:path fill stroke>` is lost for custom geometry only (presets do
   thread it — covered by `preset_path_flags_shade_and_stroke_per_path`).
3. ~~Gradient options are not in the IR…~~ **Closed** by `paint-b`:
   `VectorGradient::frame` (`GradientFrame`) carries `rotWithShape`, the authored
   rotation/flip, `scaled`, `path`, `fillToRect`, `tileRect` and `flip`, projected by
   `compiler::style::gradient` at paint time (raster shader + SVG paint server use the
   identical axis); `VectorStroke::paint` carries the line's complete gradient/pattern
   fill, which both backends paint instead of the flat `color`. The repeated-`tileRect`
   materialisation (reference `tiledGradient`, paint.ts:26-112) is still missing: an
   authored `tileRect` currently frames a single tile.
4. ~~No pattern paint~~ **Closed**: `Fill::Pattern` → `VectorPaint::Pattern` (measured 64×64
   cell from `compiler::pattern`, `PATTERN_CELL_EMU = 12700/8`) with a rasterizer pattern
   shader and an SVG `<pattern>` on the same grid; an unknown preset keeps the reference's
   solid-foreground fallback.
5. **`ShapeElement::math_fallback` / `Paragraph::math_fallbacks` are unused**: the native
   typesetter cannot fail, so the reference's "authored preview branch replaces a failed
   equation conversion" choice has no Rust counterpart.

## `renderer-shape-image-fill.test.ts` per-case classification

| # | case | class | reason / symbol needed |
| ---: | :--- | :--- | :--- |
| 1 | clips asymmetric source to the current path | N/A | Canvas op trace (`clip` `evenodd`, 10-arg `drawImage`, `globalAlpha` restore). The Rust paint is the path fill itself, so the clip is the geometry; the `srcRect`/`fillRect` half is asserted by #14-15 and `full_source_device_pixels_follow_frame_dpr_and_crop`. |
| 2 | fails closed, fill mode **omitted** | BEHAVIOURAL | `image_fill_fails_closed_when_the_fill_mode_is_omitted`: brush retained but `!mode_is_paintable()`, 0 `DrawImage`, 1 stroked `DrawPath` (1pt black), no blip pixels. |
| 3 | fails closed, fill mode **conflicting** (tile+stretch) | BEHAVIOURAL | `image_fill_fails_closed_when_tile_and_stretch_conflict`: same contract plus no decode request at all. |
| 4-5 | per-shape blip effects for a shared source (`reversed` false/true) | BEHAVIOURAL | `per_shape_blip_effects_survive_a_shared_source`: per-shape ordered `CT_Blip` chains (`[]`, `[grayscale]`, `[biLevel, grayscale]`), one shared decode, painted pixels differ per owner (untouched / grey / black-or-white) in both consumer orders. |
| 6 | shared resource prepared once, image before stroke | BEHAVIOURAL | `shared_blip_is_prepared_once_and_painted_before_its_stroke`: two native requests (one per owner, no invented resize target), both `DrawPath`s carry `fill: Image` + `stroke`, text run intact. |
| 7 | contains an ordinary decode failure, keeps stroke + text | BEHAVIOURAL | `image_fill_failure_retains_stroke_and_text`: 0 `DrawImage`, stroke `[0,0,0,255]`, text `"Picture fill text"` at 18pt. Failure injection itself is not exercisable (no per-fill decode). |
| 8 | tiled effect chain sampled at fill DPI | BEHAVIOURAL | `tiled_effect_chain_is_sampled_at_the_fill_dpi`: the authored `dpi=192` reaches the brush and `tile_source_extent_emu` yields `16 px × 9525·96/192 = 76 200` EMU, independent of the shape box; the effect chain is retained. The reference's `widthPt/heightPt` option bag has no Rust analogue. |
| 9 | tiled effect fill whose inspection fails natively | N/A | Asserts the rejection code `unsupported-image-effect` from a failed *source inspection*; `AssetStore` has no inspection stage (it returns bytes or a typed error), so the browser-side failure cannot be injected. The typed variant itself now exists (`RenderError::UnsupportedImageEffect`). |
| 10-11 | shared tile native-sized regardless of order | BEHAVIOURAL | `shared_tile_keeps_the_native_grid_regardless_of_consumer_order`: both consumer orders issue only `SizeHint::Native` requests (shape blips never derive a resize target) while the tiled/stretched placement facts survive. |
| 12 | rejects an unbounded tile auxiliary, restores state | BEHAVIOURAL | `unbounded_tile_auxiliary_is_rejected`: `prepare_image_fill` returns `RenderError::UnsupportedImageEffect` for `sx = ∞` and the slide still rasterizes with no blip. The canvas save/restore half is canvas-only. |
| 13 | in-budget tile sampling / `16_777_216` cell budget | BEHAVIOURAL | `oversized_repeating_cells_keep_the_bitmap_grid_and_the_authored_period`: `MAX_TILE_CELL_PIXELS == 16_777_216` and an oversized authored cell keeps the bitmap grid, with the brush transform restoring the authored period (the scratch-cell allocation trace itself has no Rust counterpart: the pattern shader samples the retained bitmap). |
| 14-15 | counter-rotates an image fill after clipping (`flipH` false/true) | BEHAVIOURAL | `image_fill_counter_rotates_the_brush_for_rot_with_shape_false`: `counter_rotation_radians()` is `(flipH ? 1 : -1)·π/2` and the brush linear part is that quarter turn (`|cos| = 0`) about the fill-box centre. |
| 16 | selects the complete authored equation fallback branch | BLOCKED | `ShapeElement::math_fallback` (`EquationBranchFallback`) is never consulted by `compiler::math`. |
| 17-20 | propagates equation resource failures (2 errors × load/conversion) | N/A | No MathJax load/convert stage, so there is no equation resource failure to propagate. (`RenderError::DecodedImageLimit` now covers the decoded-image quota; the equation stage is still absent.) |
| 21-24 | inline equation branch replaced once after all/first/last/none failures | BLOCKED | `Paragraph::math_fallbacks` (`InlineEquationFallback`) unused; native typesetting never fails. |
| 25-26 | complete equation `Choice` after a table-cell failure (table/mixed) | BLOCKED | Same as above, plus no `TableElement::math_fallback` consumption. |
| 27 | source locators survive preview replacement | BLOCKED | No preview replacement, and `Slide::element_sources` is not surfaced on compiled text runs. |

## Files created

- `crates/kitty-pptx-core/tests/parity_shapes_a_tests.rs` — target: custGeom, frame transform,
  pattern grid.
- `crates/kitty-pptx-core/tests/shapes_a_support/gradient_cases.rs` — the
  `renderer.gradient` ports (counter-rotation, gradient line paint, cell gradient) plus two
  supplementary active tests (the painted counter-rotation twin and a pattern-painted line).
- `crates/kitty-pptx-core/tests/shapes_a_support/mod.rs` — shared fixtures plus the recording
  `AssetStore` and slide-level rasterize helpers.
- `crates/kitty-pptx-core/tests/shapes_a_support/image_fill_cases.rs` — the fail-closed /
  failure-containment cases and the compile pins.
- `crates/kitty-pptx-core/tests/shapes_a_support/image_fill_paint_cases.rs` — the painted
  image-fill cases (#4-6, #8, #10-15), each loaded via `#[path]` as a module of the same target.

Supplementary active tests beyond the case ports (documented, not counted as cases):
`preset_path_flags_shade_and_stroke_per_path`, the two gradient paint twins
(`rot_with_shape_false_paints_the_counter_rotated_axis_in_both_backends`,
`stroke_uses_its_authored_pattern_paint`) and the four `support_checks` pins
(`compile_fill` pattern/image projection, gradient-stroke drop, darken fold, custGeom
paint drop).

## Pass-2 note (paint-a)

`renderer-shape-image-fill` #4-6, #8, #10-15 and `pattern-thumbnail` were BLOCKED on the missing
`Fill::Image` / `Fill::Pattern` paint path. That path landed (`VectorPaint::{Image, Pattern}`,
`rasterizer::image_fill`, `svg::image` paint servers, `compiler::pattern`), so the cases are now
asserted against the compiled brush and painted pixels. The equation-fallback rows (#16, #21-27)
stay BLOCKED on `compiler::math`, which is outside this file's owner.

## Pass-3 note (paint-b: gradient extras + stroke paint)

`renderer.gradient` case 1 is un-ignored and asserted faithfully: the IR keeps the
authored angle and carries the counter-rotation in `GradientFrame` (the reference feeds
`counterRotatedFillTransform` the shape rotation and negates it, `paint.ts:307-315`,
`fill-frame.ts:4-17`), so `GradientFrame::counter_rotation_deg` is `-rotation` for an
unreflected shape and `+rotation` for a reflected one. Case 2 is a real test now (it was
only a prose comment): `VectorStroke::paint` receives the authored `a:ln` gradient, the
raster paints it along the stroked box and the SVG emits an `sgrad_*` paint server. Both
backends derive the axis from the same `compiler::style::gradient` helper, which the
tests pin by asserting the painted pixels and the SVG attributes together.
