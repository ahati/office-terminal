# Coverage — batch `shapes-a`

Reference: `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d` (read-only).
Rust target: `cargo test -p kitty-pptx-core --test parity_shapes_a_tests`
(`CARGO_TARGET_DIR=target/agent-shapes-a`).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | ---: | ---: | ---: | ---: | ---: | :--- | :--- |
| `custgeom-path-paint.test.ts` | 2 | 1 | 0 | 0 | 1 | `parity_shapes_a_tests` | combined-path case asserted on the compiled `VectorSlide` (1 `DrawPath`, fill+stroke, three replayed contours, origin transform). Per-path `custGeomPaint` is dropped by production code. |
| `frame-transform.test.ts` | 3 | 3 | 0 | 0 | 0 | `parity_shapes_a_tests` | `it.each` table/chart/media rows collapse into one assertion set: the element type is only a TS parameter, all three rows call the same function with the same frame. Canvas `translate/rotate/scale` trace is asserted as the composed `AffineTransform` (ref `renderer.ts:7574-7589`). |
| `renderer.gradient.test.ts` | 3 | 0 | 1 | 0 | 2 | `parity_shapes_a_tests` | Table-cell gradient ported (one `LinearGradient`, never the `Solid` fallback, axis `(0,25)→(100,25)`). `rotWithShape=false` counter-rotation and gradient strokes have no IR slot. |
| `pattern-thumbnail.test.ts` | 1 | 0 | 0 | 0 | 1 | — | `Fill::Pattern` degrades to `Solid(fg)`; no pattern paint and no `12700/8` pattern-cell scale exists anywhere, so there is nothing to assert. |
| `renderer-shape-image-fill.test.ts` | 27 | 0 | 3 | 6 | 18 | `parity_shapes_a_tests` (module `image_fill_cases`) | Ordinary-shape image fills (`Fill::Image`) are never compiled; see the per-case table. |
| **total** | **36** | **4** | **4** | **6** | **22** | | 8 cases ported with assertions; 11 tests pass, 2 BLOCKED stubs are `#[ignore]`d. |

## Findings (production gaps, no production code touched)

1. **`Fill::Image` on an ordinary shape is silently dropped.** `compiler::style::compile_fill`
   returns `None` for `pptx_model::Fill::Image` (`compiler/style.rs:88-91`), so no
   `VectorCommand::DrawImage` is ever produced for a shape image fill — crop (`srcRect`),
   destination insets, tiling, `rotWithShape`, opacity and `<a:blip>` effects all never
   reach the display list. This is the single cause of the 18 BLOCKED rows below (plus
   2 of the N/A rows that also need it).
2. **`ShapeElement::cust_geom_paint` (`pptx_model::PathPaint`) is parsed but unused**:
   `compiler::shapes::build_shape_paths` hard-codes `PathFill::Norm` / `stroke: true`
   for `custGeom`, so `<a:path fill stroke>` is lost for custom geometry only (presets do
   thread it — covered by `preset_path_flags_shade_and_stroke_per_path`).
3. **Gradient options are not in the IR**: `compiler::style::compile_fill` drops
   `rot_with_shape`, `scaled`, `path`, `fillToRect`, `tileRect`, `flip`; `VectorStroke`
   has no paint slot at all (gradient/pattern lines keep the flat `color`).
4. **No pattern paint**: `Fill::Pattern` → `VectorPaint::Solid(fg)`; shapes and charts
   therefore cannot reproduce the reference's `12700/8` EMU pattern cell.
5. **`ShapeElement::math_fallback` / `Paragraph::math_fallbacks` are unused**: the native
   typesetter cannot fail, so the reference's "authored preview branch replaces a failed
   equation conversion" choice has no Rust counterpart.

## `renderer-shape-image-fill.test.ts` per-case classification

| # | case | class | reason / symbol needed |
| ---: | :--- | :--- | :--- |
| 1 | clips asymmetric source to the current path | N/A | Canvas op trace (`clip` `evenodd`, 10-arg `drawImage`, `globalAlpha` restore); the paint entry point (`paintPreparedShapeImageFill`) has no Rust analogue. |
| 2 | fails closed, fill mode **omitted** | BEHAVIOURAL | `image_fill_fails_closed_when_the_fill_mode_is_omitted`: 0 `DrawImage`, 1 stroked `DrawPath` (1pt black). |
| 3 | fails closed, fill mode **conflicting** (tile+stretch) | BEHAVIOURAL | `image_fill_fails_closed_when_tile_and_stretch_conflict`. |
| 4-5 | per-shape blip effects for a shared source (`reversed` false/true) | BLOCKED | Needs `Fill::Image` → `DrawImage { effects: Vec<BlipEffect> }` (blip-effect plumbing exists in `rasterizer::effects`, but the compile path drops the fill). |
| 6 | shared resource prepared once, image before stroke | BLOCKED | Same: no shape image fill, no shared-asset preparation stage. |
| 7 | contains an ordinary decode failure, keeps stroke + text | BEHAVIOURAL | `image_fill_failure_retains_stroke_and_text`: 0 `DrawImage`, stroke `[0,0,0,255]`, text `"Picture fill text"` at 18pt. Failure injection itself is not exercisable (no per-fill decode). |
| 8 | tiled effect chain sampled at fill DPI | BLOCKED | `AssetStore::get(rel_id, SizeHint)` has no `emuPerPixel`/`widthPt` decode options (`getCachedDuotoneBitmapByPath` analogue). |
| 9 | tiled effect fill whose inspection fails natively | N/A | Asserts the rejection code `unsupported-image-effect`; `RenderError` has no such variant and there is no native inspection stage. |
| 10-11 | shared tile native-sized regardless of order | BLOCKED | No tiled image fill and no decode-option (`targetWidthPx`) API. |
| 12 | rejects an unbounded tile auxiliary, restores state | BLOCKED | Tile-bound rejection needs the tiled-image sampler with a scratch-surface budget (see #13); the canvas save/restore half is N/A. |
| 13 | in-budget tile sampling / `16_777_216` cell budget | BLOCKED | No tile scratch-surface sizing code in `rasterizer` (symbol: a tiled-image sampler with a budget). |
| 14-15 | counter-rotates an image fill after clipping (`flipH` false/true) | BLOCKED | `Fill::Image.rot_with_shape` is dropped; no image-fill paint to rotate. |
| 16 | selects the complete authored equation fallback branch | BLOCKED | `ShapeElement::math_fallback` (`EquationBranchFallback`) is never consulted by `compiler::math`. |
| 17-20 | propagates equation resource failures (2 errors × load/conversion) | N/A | No MathJax load/convert stage and no resource-limit error taxonomy in core (`OoxmlDecodedImageLimitError` / `OoxmlResourceLimitError` analogues absent). |
| 21-24 | inline equation branch replaced once after all/first/last/none failures | BLOCKED | `Paragraph::math_fallbacks` (`InlineEquationFallback`) unused; native typesetting never fails. |
| 25-26 | complete equation `Choice` after a table-cell failure (table/mixed) | BLOCKED | Same as above, plus no `TableElement::math_fallback` consumption. |
| 27 | source locators survive preview replacement | BLOCKED | No preview replacement, and `Slide::element_sources` is not surfaced on compiled text runs. |

## Files created

- `crates/kitty-pptx-core/tests/parity_shapes_a_tests.rs` (353 lines) — target: custGeom,
  frame transform, gradient, pattern note.
- `crates/kitty-pptx-core/tests/shapes_a_support/mod.rs` (158 lines) — shared fixtures.
- `crates/kitty-pptx-core/tests/shapes_a_support/image_fill_cases.rs` (191 lines) — image-fill
  cases + 4 degradation-pin tests, loaded via `#[path]` as a module of the same target.
- `tests/parity/coverage/shapes-a.md` (this file).

Supplementary active tests beyond the case ports (documented, not counted as cases):
`preset_path_flags_shade_and_stroke_per_path` and the four `support_checks` pins
(`compile_fill` pattern/image projection, gradient-stroke drop, darken fold, custGeom
paint drop).
