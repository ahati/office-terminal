# Coverage — batch `shapes-b`

Reference: `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d`.
Rust target: `crates/office-terminal-core/tests/parity_shapes_b_tests.rs` (532 lines, 8 `#[test]`) +
modules `tests/shapes_b_support/line_end_cases.rs` (8 tests: line ends / compound lines) and
`tests/shapes_b_support/bevel_effect_geometry.rs` (11 tests: bevel inputs, scene3d transform
order, projected effect geometry, camera homography). 27 `#[test]`, all pass, 0 ignored.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| `bevel-band-conversion.test.ts` | 4 | 4 | 0 | 0 | 0 | `bevel_effect_geometry` | full: `scene3d::build_bevel_inputs` at devScale 1/4/8 ± 1 px + the zero-size omission |
| `reflection-blur.test.ts` | 3 | 0 | 0 | 0 | 3 | — | no band-blur pass |
| `scene3d-transform-order.test.ts` | 1 | 1 | 0 | 0 | 0 | `bevel_effect_geometry` | `split_scene3d_shape_transform` |
| `shape-effect-transform.test.ts` | 9 | 4 | 2 | 3 | 0 | `bevel_effect_geometry` | cases 3–6 via `projected_effect_geometry`; 1–2 assert the rasterized AABB |
| `shape-inner-shadow.test.ts` | 1 | 0 | 1 | 0 | 0 | `parity_shapes_b_tests` | complete |
| `shape-orthographic-bevel.test.ts` | 39 | 0 | 1 | 0 | 38 | `parity_shapes_b_tests` | 19 `it()`, 39 expanded; the back-bevel *geometry* is pinned by `bottom_bevel_inputs_sit_at_the_extrusion_base_behind_the_front_face` (pixels still blocked) |
| `shape-text-reflection.test.ts` | 7 | 0 | 0 | 0 | 7 | — | 2 `it.each`, 7 expanded |
| `shape-effect-line-decoration.test.ts` | 2 | 0 | 2 | 0 | 0 | `parity_shapes_b_tests` | full: the leader stroke *and* its `tailEnd` decoration are asserted (arrowheads now compile) |
| **total** | **66** | **9** | **6** | **3** | **48** | | 17 ports, 22 passing |

## Ported (17)

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
* `bevel_inputs_match_the_declared_emu_width_at_every_device_scale` — devScale
  1/4/8: `widthPx = w(EMU) × scale × devScale` ±1 px and `widthPx/devScale`
  constant, i.e. no clamp and no second scale factor
  (`scene3d/bevel_inputs.rs`, ref `renderer.ts:6296-6358`).
* `bevel_inputs_omit_a_zero_sized_bevel` — `w = 0` / `h = 0` and "no `sp3d`"
  produce no band (ref `buildBevelInputs`'s `w > 0 && h > 0` gate).
* `raster_bevel_band_is_not_clamped` — a 60 px authored bevel shades 60 px of
  the painted layer (the old 48 px clamp is gone; divergence 1 closed).
* `scene3d_transform_order_keeps_rotation_outside_the_face` —
  `split_scene3d_shape_transform(25, true, false)` = `{25, h, ¬v}`.
* `projected_effect_anchor_is_distinct_from_the_aabb_corner` — bbox
  `{25,−25,50,100}`, anchor `(75,75)` ≠ AABB `tr` corner (ref case 3).
* `projected_effect_bounds_round_the_edge_pad_to_device_pixels` — 1.1 CSS px pad
  → 3 device px, camera zoom 4: `bbox.x = −312`, `bbox.w = 824` (ref case 4).
* `projected_effect_bounds_use_the_extrusion_base_back_surface` — back-facing
  `lon = 150`: `bbox.x = 70 − 50·cos30`, `bbox.w = 100·cos30`, anchor on the far
  end of the back plane (ref case 5).
* `projected_effect_bounds_keep_a_camera_crossing_rear_face_finite` —
  `perspectiveFront` fov 90 / `lon = 120` / 8-in extrusion base against an
  independent pinhole bound, ±1e-7 (ref case 6).
* `bottom_bevel_inputs_sit_at_the_extrusion_base_behind_the_front_face` —
  `bevelB` → `bottom: true`, `depthPx = extrusionH × scale`; back-face selection
  and the 12 px `lon = 150` base offset (geometry half of the reference's
  back-bevel pixel case).

## Supplementary ports (not counted as cases)

`tests/shapes_b_support/line_end_cases.rs` ports five of the six
`line-end-shaft-trim.test.ts` cases, whose reference behaviour is the retraction
`compiler::style::stroke` now emits (`arrow-geometry.ts:107-333`):

* `connector_tail_end_trims_the_shaft_and_paints_the_open_arrow` — `line` 300×200: the shaft
  stops `6/sin(atan2(2.5,4.5)) = 12.35 px` short of `(400,300)`, on the leader's slope; the
  open arrow is two arms 4.5 e / 2.5 e behind that vertex and its vertex sits exactly one
  retraction behind the tip; raster inks the tip and nothing past it; the SVG emits the same
  two stroked paths.
* `curved_connector_shaft_stops_at_the_vertex_on_its_curve` — `curvedConnector3`: the cubic is
  split (de Casteljau) and the cut point is one retraction from the tip.
* `bent_connector_short_leg_stops_at_the_vertex` — `bentConnector2` with a 1 px terminal leg:
  the trim crosses onto the preceding run, still one retraction from the tip.
* `compound_double_connector_paints_two_trimmed_rails` — `cmpd="dbl"` paints two `w/3` rails at
  ∓`w/3` (centre line erased), each retracted; both rails and the decoration are in the
  display list.
* `both_arrow_ends_consume_a_short_compound_run` — a 15 px run with an arrow at each end paints
  no shaft at all, only the two decorations.

The sixth case (`arcTo` custom geometry) is covered by the same cubic trim path: the preset
engine already flattens authored arcs, so a custom `arcTo` reaches the trimmer as cubics.
All six reference cases now run as ledger cases in
`parity_text_box_b_highlight_tests` (the `stroke` pass reinstated them; the recorder reads
`DrawPath` only, so the `DrawLineEnd` decorations are never counted as shafts). The five
tests here stay as the painted twin and as the source of the compound-rail assertions.

## BLOCKED (missing symbol)

* `reflection_blur_bands(bounds, max_blur) -> Vec<Band>` — the reflection pass
  applies one Gaussian with `sigma = blurRad` (`rasterizer/shape_effect/reflection.rs:158`),
  with no distance-aware radius ramp, band clipping or 24-pass cap (3 cases).
* `reflected_shape_text_rotation(rotation, flip_h, flip_v) -> f32` — `text::vertical`
  has frame rotation only; flipped shapes reuse the mirrored shape transform, so the
  readable-rotation fold does not exist (7 cases).
* `shape-orthographic-bevel` (38 of 39): the reference assertions are Playwright
  PNG oracles over a full homography warp of the *painted* body — extrusion side
  walls, wireframe rims, per-material shading (`bevel-shading.ts`,
  `bevel-light-rigs.ts`), picture `prstGeom` bevels and overflowing projected
  text. Their **geometry** now exists (`scene3d::camera` +
  `scene3d::projected_effect_geometry` + `scene3d::build_bevel_inputs` with
  `bevelB`/`extrusionH`), but `rasterizer::shape_effect` still paints a single
  planar layer through the affine `camera_affine` and has no homography warp,
  rear plane, wall sweep or rig/material compositor, so no pixel assertion can be
  honoured yet. Missing symbols: `draw_projected` (homography warp),
  `apply_extrusion`, `apply_bevel_shading`'s material/rig terms,
  `preset_material`-driven lighting, wireframe rim painter.

## N/A (3)

* `shape-effect-transform` 7 and 9 — `OffscreenCanvas` / `drawImage` call-count
  spy on a device-space paint cache; the Rust backend has no such cache.
* `shape-effect-transform` 8 — `clampCanvasSize` DOM surface limit; the Rust
  effect layer is clipped to the target pixmap (`shape_effect.rs:100-110`), so
  there is no oversized-surface branch.

## Divergences found (not fixable from tests)

1. ~~bevel band clamped to 48 px; uses `max(w, h)` and shades when `w` or `h` is 0.~~
   **Closed** by the `3d` pass: `scene3d::build_bevel_inputs` converts
   `w(EMU) × scale × devScale`, gates on `w > 0 && h > 0` and has no clamp; the
   band is `bevelT@w` (not `max(w, h)`).
2. inner shadow loses its offset when `|dist|` leaves the target pixmap
   (`mask::shift_mask:41` returns zeros); the reference pads its aux canvas.
3. ~~`VectorStroke` carries no line-end decoration…~~ **Closed**: the `stroke` pass
   finished **C2** — `VectorStroke::{head_end,tail_end,compound,miter_limit}`,
   `LineEnd*`/`CompoundLine` in `contracts/paint.rs` and `VectorCommand::DrawLineEnd` —
   and `compiler::style::stroke::compile_stroked_paths` emits the trimmed shaft, the
   `a:ln@cmpd` rails and the `a:headEnd`/`a:tailEnd` decorations (the latter as
   `DrawLineEnd`, the IR form of the reference's translated tip frame; both backends
   dispatch it and honour `a:ln@miterLim`).
   One divergence remains: the reference composites the decorations into the same paint
   target, so an outer shadow follows the arrowhead; the IR gives every command its own
   effect layer, so decoration paths carry no effects.

## Pass-2 note (paint-b)

Line ends and compound lines landed (`compiler::style::line_end::{mod,trim}`,
`compiler::style::stroke`). The trim distance is scale-free in EMU, so the compiler
resolves it once and both backends paint identical geometry; three new tests and the two
`shape-effect-line-decoration` cases now exercise it. Case counts are unchanged: the
`line-end-shaft-trim` row lives in the `text-box-b` ledger and is documented above as a
supplementary port instead of being double counted.

## Pass-3 note (branch `3d`)

Production landed (all reference-cited):

* `scene3d::camera` — faithful port of `scene3d-camera.ts`: the observed
  `ST_PresetCameraType` table (44 modern presets + unknown/legacy fallbacks),
  `compute_scene3d_quad` (perspective divide, oblique shear, zoom), the
  6.28-inch modern lens with `cameraDistance`'s 2 % view-Z grace gap,
  `compute_depth_offset` and `is_back_facing`.
* `scene3d::homography` — `unit_square_to_quad` / `apply_h` /
  `expand_projected_quad` / `project_quad_point` (`scene3d-draw.ts:40-97,432-478`).
* `scene3d::projection` — `transformed_effect_bounds`,
  `projected_surface` (rear-plane selection + device-px pad quantisation) and
  `projected_effect_geometry` (`renderer.ts:3596-3721`).
* `scene3d::bevel_inputs` — `build_bevel_inputs` + `BevelInput` + `PresetMaterial`
  (`renderer.ts:6296-6358`, `bevel-lighting.ts:16-89`).
* `compiler::shape_effects::split_scene3d_shape_transform` (`renderer.ts:3570-3583`)
  and the new IR fields that feed all of the above:
  `VectorScene3d::{zoom, rot}` (`VectorRot3d`), `VectorShape3d::{bevel_bottom_*,
  prst_material}` — extraction from `pptx-model`'s `a:camera@{zoom,rot}`,
  `a:bevelB` and `a:sp3d@prstMaterial`.

Documented divergences that remain:

1. The **svg** path (`svg::filters`) still projects through
   `scene3d::camera_affine`, the affine fit of the reference quad; SVG emits a
   `matrix(..)` while the quad is affine and the fitted matrix plus the
   documented `scene3d_perspective` degradation otherwise. The **raster** path
   now warps through the reference homography
   (`rasterizer::shape_effect::projection`, `projectScene3dPaint`).
2. `build_bevel_inputs` resolves its light from the IR's authored azimuth
   (`VectorScene3d::light_direction_deg`) rather than `lightDirFromRig`, because
   the IR does not carry `lightRig@rig/@dir/@rot`; the reference's per-rig light
   tables (`bevel-light-rigs.ts`, 493 lines) are not ported.
3. The raster bevel shades the front lip only. `bevelB` inputs are built (with
   `depthPx` from `extrusionH`) and the back face is selected geometrically, but
   the raster composes one planar layer, so a bottom lip has nothing to shade.
4. `compiler/shape_effects.rs` reads `a:sp3d@prstMaterial` straight through;
   `pptx-model` defaults the attribute to `warmMatte` where the reference would
   see `undefined` → `matte`. Shading only (the bevel band is unaffected).

No golden output changed (`UPDATE_GOLDENS` not run; no committed golden deck
carries `a:sp3d`), and `parity_shapes_b_tests` grew from 13 to 22 passing tests.

## Pass-4 note (S6 / contract gate C3)

`VectorScene3d` now carries every reference camera input: `zoom`, `rot`
(`VectorRot3d`), `light_dir`, `light_rot` and `units_per_inch` (914 400 for EMU
paths, 96 for CSS-px fixtures). `VectorShape3d` exposes `VectorBevel` values as
`bevel_top`/`bevel_bottom`, plus `prst_material` (`a:sp3d@prstMaterial`).
`compiler::shape_effects` extracts `a:lightRig@{rig,dir,rot}` and both bevels.

The raster paint path replaces the three-corner affine with
`project_body_layer`: the authored face is painted into a padded
device-resolution buffer (bevel/extrusion included), then warped onto the slide
through `computeScene3dQuad` + `unitSquareToQuad`/`invert_h` with bilinear
sampling (`ref renderer.ts:6400-6640`, `core/src/shape/scene3d-draw.ts`).
Identity cameras (`isScene3dNonIdentity`, including the oblique rear-plane and
perspective-depth gates) keep the flat path; a failed buffer allocation or an
oversized layer degrades to `camera_affine`.

Two supplementary tests pin the new path (not reference cases, so the case
counts are unchanged): `perspective_camera_warps_the_face_through_the_quad_homography`
and `perspective_camera_zoom_scales_the_projected_face`. Unit coverage in
`scene3d::{authored_zoom_scales_a_face_on_perspective_face,
units_per_inch_drive_the_fixed_physical_lens,
identity_gate_accounts_for_zoom_and_rear_depth,
homography::the_inverse_maps_projected_points_back_to_the_unit_square}`.

`scene3d-transform-order` and all four `shape-effect-transform` geometry cases
(cases 3-6) are live in this target; the archive has no `parity_shapes_b_tests`
entry to reinstate.
