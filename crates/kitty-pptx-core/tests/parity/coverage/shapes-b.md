# Coverage — batch `shapes-b`

Reference: `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d`.
Rust target: `crates/kitty-pptx-core/tests/parity_shapes_b_tests.rs` (519 lines, 8 `#[test]`, all pass).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| `bevel-band-conversion.test.ts` | 4 | 0 | 2 | 0 | 2 | `parity_shapes_b_tests` | devScale 1 + linearity ported |
| `reflection-blur.test.ts` | 3 | 0 | 0 | 0 | 3 | — | no band-blur pass |
| `shape-effect-transform.test.ts` | 9 | 0 | 2 | 3 | 4 | `parity_shapes_b_tests` | AABB cases 1–2 ported |
| `shape-inner-shadow.test.ts` | 1 | 0 | 1 | 0 | 0 | `parity_shapes_b_tests` | complete |
| `shape-orthographic-bevel.test.ts` | 39 | 0 | 1 | 0 | 38 | `parity_shapes_b_tests` | 19 `it()`, 39 expanded |
| `shape-text-reflection.test.ts` | 7 | 0 | 0 | 0 | 7 | — | 2 `it.each`, 7 expanded |
| `shape-effect-line-decoration.test.ts` | 2 | 0 | 2 | 0 | 0 | `parity_shapes_b_tests` | arrowhead sub-claim blocked |
| **total** | **65** | **0** | **8** | **3** | **54** | | 8 ports, 8 passing |

## Ported (8)

* `bevel_band_is_the_declared_emu_width_at_unit_device_scale` — devScale 1: band
  = `304800 × (1920/12192000)` = 48 px ±1 measured off the raster
  (`rasterizer/bevel.rs:19,44`).
* `bevel_band_scales_linearly_with_device_scale` — linearity in the device scale
  (1× vs ¼×), ref `for (const devScale of [1,4,8])`.
* `effect_bounds_include_group_translation_and_scale` — ink box = `{768,225,219,300}`.
* `effect_bounds_are_the_rotated_aabb` — ink box = `{400,50,60,80}`.
* `inner_shadow_does_not_synthesize_a_surface_for_a_no_fill_shape` — 960×540
  canvas untouched.
* `inner_shadow_covers_the_complete_mitered_outline` — every opaque outline pixel
  → `(34,57,98)` ±1.
* `outer_shadow_silhouette_includes_the_connector_leader` — shadow alpha 0.15
  (red 210…226) along the leader, none past 3σ.
* `partly_off_viewport_shape_stays_in_the_projection_path` — projected
  parallelogram, no billboard fallback.

## BLOCKED (missing symbol)

* `build_bevel_inputs(sp3d, light, material, scale, dev_scale) -> Vec<BevelInput>`
  / `bevel_band_px`. `rasterizer/bevel.rs:19` clamps the band to 48 px, so the
  reference's devScale 4 (192 px) and devScale 8 (384 px) are unreachable; the
  same file's "omits the bevel when w or h is zero" is unreachable because the
  band uses `max(w, h)` and does not gate on `w > 0 && h > 0`.
* `reflection_blur_bands(bounds, max_blur) -> Vec<Band>` — the reflection pass
  applies one Gaussian with `sigma = blurRad` (`rasterizer/shape_effect/reflection.rs:158`),
  with no distance-aware radius ramp, band clipping or 24-pass cap.
* `projected_effect_geometry(camera, transform, x, y, w, h, pad, algn, depth)`
  (cases 3–6): missing `VectorScene3d::{zoom, rot{lat,lon,rev}}`,
  `compute_scene3d_quad`, `expand_projected_quad`, back-face selection and
  `project_quad_point`. `scene3d::camera_affine` is affine-only from `prst`.
* `shape-orthographic-bevel` (38): all need Playwright/PNG pixel oracles plus
  IR absent from the contracts — `VectorScene3d::{rot, zoom, light_rig.rot}`,
  `VectorShape3d::prst_material` (translucentPowder/matte/legacyWireframe),
  `bevel_b`, extrusion side walls, wireframe rims, picture `prstGeom` bevels, or
  overflowing projected text under a camera homography.
* `reflected_shape_text_rotation(rotation, flip_h, flip_v) -> f32` (7 cases) —
  `text::vertical` has frame rotation only; flipped shapes reuse the mirrored
  shape transform, so the readable-rotation fold does not exist.

## N/A (3)

* `shape-effect-transform` 7 and 9 — `OffscreenCanvas` / `drawImage` call-count
  spy on a device-space paint cache; the Rust backend has no such cache.
* `shape-effect-transform` 8 — `clampCanvasSize` DOM surface limit; the Rust
  effect layer is clipped to the target pixmap (`shape_effect.rs:100-110`), so
  there is no oversized-surface branch.

## Divergences found (not fixable from tests)

1. bevel band clamped to 48 px; uses `max(w, h)` and shades when `w` or `h` is 0.
2. inner shadow loses its offset when `|dist|` leaves the target pixmap
   (`mask::shift_mask:41` returns zeros); the reference pads its aux canvas.
3. `VectorStroke` carries no line-end decoration (`pptx_model::Stroke::tail_end`
   is parsed but dropped by `compiler::style::compile_stroke`), so the arrowhead
   half of the line-decoration silhouette case cannot be asserted.
