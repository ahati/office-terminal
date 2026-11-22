# PAINT / IMAGE / STROKE gaps — implementation spec (core)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only). Source of truth for counts:
`crates/office-terminal-core/tests/parity/COVERAGE.md` + `tests/parity/coverage/*.md`.
Verify per agent: `CARGO_TARGET_DIR=/workspace/kitty-pptx/target/agent-<name> cargo test -p
office-terminal-core --test <target>` and `cargo clippy -p office-terminal-core --lib -- -D warnings`.
Every file < 600 lines. No `unwrap()` in production paths.

## 0. Snapshot — what is still BLOCKED in this domain

| gap | blocked cases | stub location |
| :--- | --: | :--- |
| shape/table image fill (`Fill::Image`) | 10 + 3 | `tests/shapes_a_support/image_fill_cases.rs` (2 `#[ignore]`d cases stay ignored, see §10) |
| tiled image fill (`a:tile`) inside a shape | (subset of the 10: #10-13) | same |
| pattern paint (`Fill::Pattern`, shape + chart) | 1 | comment-only stub `parity_shapes_a_tests.rs:343-357` |
| gradient `rotWithShape` counter-rotation | 1 | `#[ignore]` `rot_with_shape_false_counter_rotates_the_gradient` |
| gradient/pattern **stroke** paint | 1 | comment-only stub `parity_shapes_a_tests.rs:269-274` |
| arrowheads `headEnd`/`tailEnd` + shaft trim | 6 | 6 `#[ignore]` fns in `parity_text_box_b_highlight_tests.rs:432-548` |
| compound strokes (`cmpd`) — shape lines | 2 (of the 6) | same file |
| decode planner / decoded-byte budget / poster raster | 17 + 6 + 6 | `parity_elements_tests.rs` (none written yet) |
| chart image fill (depends on the IR here) | 15 | chart batch, out of this spec's workload |

Already landed (do not redo): slide-background `a:blipFill` stretch+tile (`rasterizer/background_image.rs`,
`svg/background_image.rs`, all 12 `tile-fill.test.ts` cases), picture `srcRect` projection
(`compiler/pictures.rs::project_crop`, `tests/picture_parity_tests.rs`), table `cmpd` rails
(`compiler/tables/compound.rs`), dash/cap/hairline (`compiler/style.rs`), `SizeHint::AtMost`
(`loader/asset_store.rs:8-11`).
Note: `renderer-shape-image-fill.test.ts` has **18** BLOCKED rows in `coverage/shapes-a.md`, but
#16, #21-27 (8 cases) are equation-fallback selection, not paint — they belong to the math WP.
Paint owns #4-6, #8, #10-15 → **10**; #1 and #9 are reclassified in §10.

## 1. IR additions (frozen contract — land first, everyone codes against this)

`contracts/paint.rs`

```rust
pub enum VectorPaint { Solid([u8;4]), LinearGradient(VectorGradient),
                       RadialGradient(VectorGradient), Pattern(VectorPattern) }

pub struct VectorPattern { pub preset: String, pub fg: [u8;4], pub bg: [u8;4] }
/// 8 pt: the 8x8 preset tile is painted at 1 pt per cell; PPTX user space is EMU,
/// so one period = 8 * 12_700 = 101_600 EMU. ref core/src/shape/paint.ts:487-495
pub const PATTERN_CELL_EMU: f32 = 101_600.0;

pub struct VectorGradient {
    pub angle_deg: f32,            // already counter-rotated when !rot_with_shape
    pub stops: Vec<(f32, [u8;4])>,
    pub kind: GradientKind,
    pub fill_to_rect: Option<RelativeRect>, // radial focus frame, §20.1.8.31
    pub tile_rect: Option<RelativeRect>,    // §20.1.8.59 tiling frame
    pub flip: TileFlip,                     // §20.1.8.59 @flip
}
pub enum GradientKind { Linear { scaled: bool },        // §20.1.8.41
                        Radial { path: RadialPath } }   // circle|rect|shape, §20.1.8.32
pub enum RadialPath { Circle, Rect, Shape }
/// edge insets, negative = overscan (CT_RelativeRect). l/t/r/b order.
pub struct RelativeRect { pub l: f32, pub t: f32, pub r: f32, pub b: f32 }

pub struct VectorStroke {
    /* existing: color, width_emu, dash_segments, line_cap, line_join */
    pub miter_limit: f32,                  // a:ln@miterLim/100000, default 10
    pub paint: Option<VectorPaint>,        // None => `color` (ref paint.ts:495-500)
    pub head_end: Option<LineEnd>,
    pub tail_end: Option<LineEnd>,
    pub compound: CompoundLine,            // Sng = absent/sng
}
pub struct LineEnd { pub kind: LineEndKind, pub w: LineEndSize, pub len: LineEndSize }
pub enum LineEndKind { Triangle, Stealth, Diamond, Oval, Arrow } // `none` == absent
pub enum LineEndSize { Sm, Med, Lg }        // unknown -> Med (arrow-geometry.ts:70)
pub enum CompoundLine { Sng, Dbl, ThinThick, ThickThin, Tri }
```

`contracts/vector_slide.rs`

```rust
pub enum ImagePlacement {
    Stretch { dest: RectF32 },        // crop.ts:191-231, 9-arg stretch into `dest`
    Tile(Box<TilePlacement>),         // §20.1.8.58
}
pub struct TilePlacement {
    pub tx_emu: i64, pub ty_emu: i64, // §20.1.8.58 @tx/@ty
    pub sx: f32, pub sy: f32,
    pub flip: TileFlip, pub algn: String, // unknown -> "tl" (renderer.ts:2377-2403)
    pub dpi: u32,                     // a:blip@dpi, fallback 96
    pub box_w: f32, pub box_h: f32,   // host-local fill box the grid registers against
}
VectorCommand::DrawImage {
    rel_id: String, src_rect: NormalizedRect,
    placement: ImagePlacement,        // replaces `dest_rect`
    opacity: f32, transform: AffineTransform, effects: Vec<BlipEffect>,
}
/// Line-end decorations are their own command so a shaft is never confused with
/// a tip (the reference paints them in a translated tip frame, renderer.ts:4490-4510).
VectorCommand::DrawLineEnd { path: VectorPath, fill: Option<VectorPaint>,
                             stroke: Option<VectorStroke>, transform: AffineTransform }
```

`contracts/geometry.rs`: add `AffineTransform::rotation_about(deg, cx, cy) -> Self` and
`pub fn compose(&self, other: &Self)` alias of `post_concat` (used for the image-fill
counter-rotation, §20.1.8.16).

`contracts/paint.rs` shared maths (both backends must call these; parity depends on it):

```rust
impl TilePlacement {
    /// Native period in EMU of one tile for a decoded bitmap. `src` is the logical
    /// (§20.1.8.55) window, so negative outsets stay transparent.
    /// ref renderer.ts:2408-2458 (`tileSourceExtent` + `pixelToEmu = 9525*96/dpi`)
    pub fn extent_emu(&self, bitmap_w: u32, bitmap_h: u32, src: &NormalizedRect) -> (f32, f32);
    /// Grid origin offset (algn anchor + tx/ty). ref renderer.ts:2377-2403, 2543-2559
    pub fn phase_emu(&self, tile_w: f32, tile_h: f32) -> (f32, f32);
}
/// §20.1.8.33: R F R = F, so sign = +1 when exactly one flip is authored.
/// ref renderer.ts:2640-2656, core/src/shape/fill-frame.ts:1-16
pub fn fill_counter_rotation(rotation: f32, flip_h: bool, flip_v: bool) -> f32;
/// Linear axis for a box, honouring `scaled` (§20.1.8.41) and `tile_rect`.
/// ref core/src/shape/paint.ts:283-315
pub fn linear_gradient_axis_for(grad: &VectorGradient, x: f32, y: f32, w: f32, h: f32)
    -> ((f32, f32), (f32, f32));
pub fn radial_gradient_frame_for(grad: &VectorGradient, x: f32, y: f32, w: f32, h: f32)
    -> ((f32, f32), f32);              // focus frame + radius, path=rect -> max(rx,ry)
```

`compiler/style.rs` signature change (call sites: `compiler/mod.rs:129`, `tables.rs:228,250`,
`pictures.rs:66`, `charts`):

```rust
#[derive(Clone, Copy, Default)]
pub struct FillFrame { pub rotation_deg: f32, pub flip_h: bool, pub flip_v: bool }
pub fn compile_fill(fill: &pptx_model::Fill, alpha: f32, frame: FillFrame) -> Option<VectorPaint>;
pub fn compile_stroke(stroke: &pptx_model::Stroke) -> Option<VectorStroke>; // + paint/line ends/cmpd
```

`compile_fill` keeps its current gradient rules (0 stops → `None`, 1 stop → solid,
`paint.ts:377-379`), and for `rot_with_shape == false` folds
`angle_deg -= fill_counter_rotation(frame.rotation_deg, ..)`. That is exactly what the
blocked test asserts (`angle_deg == 0` for rotation 90 / no flip, box `(0,0,100,100)`,
`renderer.gradient.test.ts:37-50`).

## 2. WP-P1 — gradient options + pattern paint + `miter_limit` (A1)

Algorithm. Linear (`paint.ts:283-315`): `tileX/Y/W/H` from `tile_rect` (default full box),
`dx,dy = cos/sin(angle)`; if `scaled` then `dx*=tileW; dy*=tileH` and re-normalise; centre =
tile centre; `half = (|dx|*tileW + |dy|*tileH)/2`; endpoints = centre ± dir*half. Radial
(`paint.ts:114-140`): focus frame from `fill_to_rect` (default full box), centre = frame centre,
`rx = max(|cx-x|, |x+w-cx|)`, `ry` likewise, radius = `path=="rect" ? max(rx,ry) : hypot(rx,ry)`;
`rotWithShape=false` rotates the **centre** about the box centre (`fill-frame.ts`), which is a
no-op for the default focus frame — assert that. `tile_rect` + `flip` for gradients
(`paint.ts:39-108`) tile a rasterised base; **out of scope here** (no blocked case, needs an aux
surface); leave a `// TODO(ref paint.ts:39-108)` and keep the untiled frame, recorded in
`coverage/shapes-a.md`.
Pattern (`paint.ts:440-500`): `sampleToUser = PT_TO_EMU / 8`, i.e. one 8×8 tile spans
`8*12700 = 101_600` EMU in slide user space, phase anchored at the user-space origin
(`renderer.ts:2358` paints the background on the same grid). Cell colours: `#` fg, `.` bg,
`+` per-channel midpoint of fg/bg including alpha (`pattern-bitmaps.ts:96-108`). Unknown preset
or empty preset → `VectorPaint::Solid(fg)` (`paint.ts:492-493`).

Rasterizer (`rasterizer/pattern.rs`, new; `rasterizer/path.rs` gets one match arm): build a 64×64
`tiny_skia::Pixmap` once per (preset,fg,bg) in a thread-local cache (`MAX_PATTERN_CACHE_ENTRIES`
= 256, `paint.ts:466`), 8×8 px per cell, then
`tiny_skia::Pattern::new(tile.as_ref(), SpreadMode::Repeat, FilterQuality::Nearest,
Transform::from_scale(k, k))` with `k = PATTERN_CELL_EMU * device_scale / 64` and no translate
(the tile is already in user-space EMU). Fill with `pixmap.fill_path`.
SVG (`svg/pattern.rs`, new): `svg/path.rs::format_fill_attr` returns `url(#pattern_N)`;
`emit_pattern_def` writes `<pattern id patternUnits="userSpaceOnUse" width="101600"
height="101600">` containing 64 `<rect>`s (crisp, resolution independent, no image encode).
Pattern data table `compiler/pattern_data.rs` (new): the 54 `PATTERN_BITMAPS` rows verbatim
(`pattern-bitmaps.ts:18-76`) + `pattern_cells(preset) -> Option<[u8;64]>`.

Tests: `parity_shapes_a_tests.rs` — replace the comment stub with
`pattern_cell_matches_the_reference_270_emu_per_px`: compile the fixture from
`pattern-thumbnail.test.ts:35-63` (shape 2_000_000×1_000_000 `pct50` + a chart with `horz`) and
assert `VectorPaint::Pattern{preset,fg:[0xD2,0x1D,0x54,255],bg:[0x12,0xCE,0xD4,255]}` on both,
plus `PATTERN_CELL_EMU == 101_600.0` and the derived matrix `k = (240/9_144_000)*12700/8`
(the reference's `matrices[0].a`). Add an `svg/pattern.rs` unit test that the emitted `<pattern>`
width/height equals `101_600` and that the raster `k` and the SVG period agree for
`width_px = 240`. LOC ≈ 300 (data 90, raster 90, svg 60, wiring+tests 60). Risk: 64-rect SVG
size (kept under control: only emitted when a pattern is painted).

## 3. WP-P2 — line ends, shaft trim, compound strokes (A2)

Algorithm (`packages/core/src/shape/arrow-geometry.ts`): `e = max(lw, 2pt)`, spans
`SPAN = {sm:2, med:3, lg:5}*e`, open-arm `along {2,3,4.5}`, `across {1.25,1.75,2.5}`
(:60-79); `lw = max(0.5, width*scale)` (`stroke-geometry.ts:21`); retraction (:113-124):
triangle `len - e/2`, stealth `{1,2,3}*e`, arrow `(lw/2)/sin(atan2(across, along))`,
diamond/oval 0. Decoration paint (:160-190): triangle/stealth/diamond/oval = filled outline,
`arrow` = two arms stroked at `lw`, round cap, miter join, **never dashed**. Path trim
(:300-343): `trim_geometry_path_ends(path, head_px, tail_px)` cuts where the path last (tail) /
first (head) crosses a circle of that radius around the terminal point, splitting Béziers with
de Casteljau and arcs by angle, dropping closed subpaths. Leaders (`renderer.ts:4395-4500`):
straight/`bentConnector*`/`curvedConnector*`/callout leaders trim the whole polyline/curve;
compound + straight uses one trimmed run and **paints nothing if both ends consume it**.
Compound rails (`renderer.ts:7450-7520`): envelope table identical to
`compiler/tables/compound.rs:30-78` — extract that `Band`/`rails()` into
`compiler/stroke/compound.rs` and have the table leg call it (no behaviour change; the table
leg keeps its `dbl` device-pixel floor, the shape leg does not).

New module `compiler/stroke/` (A2 owns): `mod.rs` (`emit_stroked_subpath(commands, path, fill,
stroke, transform, effects)` — A1 leaves the call site in `compiler/mod.rs`, A2 fills the body),
`arrow.rs` (`line_end_metrics`, `line_end_retract`, `open_arm`, `build_line_end_path` in the tip
frame, `resolve_arrow_paint`), `trim.rs` (`trim_geometry_path_ends`, `reach_parameter`), 
`compound.rs` (bands + `compound_runs(start,end,cmpd,width) -> Vec<(Path,width)>`).
Emission: trim in **EMU** (`retract_emu = retract_px * 9525`, the reference device px at 96 DPI),
emit the trimmed shaft as `DrawPath`, and each tip decoration as `DrawLineEnd` with
`transform = shape_transform.post_concat(rotation_about(angle, tip))`.
`miter_limit` is threaded from `compile_stroke`.

Tests: enable the 6 `#[ignore]` fns in `parity_text_box_b_highlight_tests.rs` (no helper change:
`shaft_ends` matches `DrawPath` only, so `DrawLineEnd` is filtered automatically). Expected
numbers: `VERTEX_PX = 6/sin(atan2(2.5,4.5)) = 12.3548` (9 pt line, `e = 12 px`), compound ends at
`400 - 12.3548`, both-ends case yields `[]`. Add unit tests for the three retraction formulas at
`lw` = 0.5/12/27 px and for `trim_geometry_path_ends` on a cubic (endpoint + tangent preserved).
LOC ≈ 420 (arrow 150, trim 150, compound 80, wiring 40). Risk: the trim circle test must use the
same `<=`/`<` convention as `reachParameter` (:268-292) or sub-pixel drift appears at
`toBeCloseTo(…,1)`; arcs must be re-emitted as `PathSegment::CubicTo` (our IR has no arc) —
document the ≤1e-3 px deviation.

## 4. WP-P3 — shape/table image fills (A3)

Algorithm. Gate (`renderer.ts:2590-2612`): paint only when exactly one of `tile` / `stretch` is
authored; omitted or conflicting ⇒ nothing (already pinned by two passing tests). Paint
(`renderer.ts:2620-2710`): clip to the shape path **first**, apply `alpha` as opacity, then when
`rotWithShape == false` rotate about the fill-box centre by `sign*rotation` (sign rule §1), then
either tile (coverage expanded to the inverse-rotated box, phase unchanged) or stretch into
`fillRect`-projected `dest`, with `srcRect` crop via the existing `project_crop`
(`compiler/pictures.rs:150-215`, ref `crop.ts:76-111`). Per-shape prepared bitmap is keyed by
path + **ordered** blip effects (`renderer.ts:2560-2588`, `shapeFillKey`), so two shapes sharing a
PNG but differing in `grayscale`/`biLevel` decode separately (case #4-5); a shared *plain* source is
prepared once (case #6). Tiled effect chains decode at the fill DPI:
`emuPerPixel = 9525*96/dpi` and `widthPt = heightPt = 0` for tiles (case #8;
`renderer.ts:2246-2266`).

New `compiler/shapes/image_fill.rs`:

```rust
pub struct ImageFillPlan { pub rel_id: String, pub src_rect: NormalizedRect,
    pub placement: ImagePlacement, pub opacity: f32, pub effects: Vec<BlipEffect>,
    pub counter_rotation_deg: f32 }
pub fn plan_image_fill(fill: &pptx_model::Fill, frame: FillFrame, box_w: f32, box_h: f32)
    -> Option<ImageFillPlan>;   // None = fail-closed / no visible source
/// PushClip + DrawImage (+ PopClip) into the element's command list.
pub fn push_image_fill(commands: &mut Vec<VectorCommand>, plan: &ImageFillPlan,
    clip: Option<VectorPath>, transform: AffineTransform);
```
`compiler/mod.rs` (A1 leaves the call site) replaces `compile_fill` for `Fill::Image` with
`plan_image_fill` + `push_image_fill(clip = Some(subpath.path))`, before text/stroke. Tables
(`compiler/tables.rs:250`) and the table background (`:228`) use the same helper clipped to the
cell rect (ref `renderer-table-image-fill.test.ts`: full background first, then alpha cell bands,
then direct cell images).

Backends. Promote the existing tile painters to shared modules: `rasterizer/tile.rs`
(`paint_tiles(...)` moved out of `rasterizer/background_image.rs:120-331`, plus
`MAX_TILE_PIXELS = 16_777_216`, `MAX_TILE_DIM = 4096`, shrink rule
`renderer.ts:2510-2532`) and `svg/tile.rs` (move `tiled_element` from
`svg/background_image.rs:100-215`). `rasterizer/image.rs` and `svg/image.rs` dispatch on
`ImagePlacement`; `Stretch` keeps today's `project_crop`/nested-`<svg>` behaviour byte for byte
(`tests/picture_parity_tests.rs` must stay green).

Tests: enable 10 cases in `tests/shapes_a_support/image_fill_cases.rs` (`#4-5,6,8,10-15`), plus
reclassify #1 (clip + 10-arg draw order) as a normal case: the compiled list is
`PushClip, DrawImage{Stretch{dest=(60,35 wide)}}, PopClip, DrawPath(stroke)`. New parity tests in
`tests/picture_parity_tests.rs` style: raster vs SVG for one stretched fill and one tiled fill
(same 480×420 or 1000×600 extent as `tile-fill.test.ts`, phase offset asserted as
`x = ax*9525`). Table cases (`parity_elements_table_tests.rs`): paint order
`drawImage, fillRect, drawImage`, alpha `0.25`, `srcRect` clip, and the strict budget error.
LOC ≈ 430 (image_fill 150, tile move 60, raster 80, svg 70, tables 40, tests 30). Risk: tile
phase under a counter-rotated fill — the reference anchors the grid in the *unrotated* box
(`renderer.ts:2660-2680`); the tile painter must register at `box` origin then apply the command
transform, not the reverse.

## 5. WP-P4 — decode planner, decoded-byte budget, poster raster (A4)

Algorithm. Display target (`core/src/image/raster-target.ts:1-60`): full-source device grid =
destination px × DPR, **divided by the visible source fraction** (`srcRect`), then doubled for
geometry headroom (ref test `renderer-picture-raster-target.test.ts:237-275`: 480 CSS px, DPR 2,
50 % visible width ⇒ `targetWidthPx 3840`, `targetHeightPx 1440`; background case: fillRect
`(0.1,-0.1,0.2,0.05)` on 960 px ⇒ `4136×3024`). Caps (`pixel-budget.ts:20-52`):
`MAX_RASTER_DIMENSION 32767`, `MAX_RASTER_PIXELS 1<<25` (32 MP per surface),
`MAX_RASTER_SOURCE_PIXELS 1<<27` (source header ceiling), `MAX_DECODED_IMAGE_BYTES =
MAX_RASTER_PIXELS*4` (128 MiB default budget, hard cap `1<<27*4`). Admission
(`core/src/image/adaptive-image-budget.ts:134-…`): one quality level for every raster on the
slide; `adaptive` shrinks, `strict` raises the typed quota error; `resolution:
'display' | 'native-if-fit'`.

Rust mapping (deliberately simpler, same observables):
`loader/probe.rs` — `pub fn raster_size(bytes: &[u8]) -> Option<(u32,u32)>` reading PNG IHDR,
JPEG SOFn, GIF, WEBP VP8/VP8L **before** any decode, so the 60000×60000 bomb is refused pre-decode.
`loader/budget.rs` — `pub struct ImageBudget { total_bytes: usize, strategy: BudgetStrategy }`,
`pub fn plan(&self, demands: &[DecodeDemand]) -> Result<HashMap<String,(u32,u32)>, AssetError>`,
`DecodeDemand { key: String, target_w: u32, target_h: u32, source_w: Option<u32>,
source_h: Option<u32> }`; one shared `scale` for all demands (min over
`(budget / (4 * Σ target_area)).sqrt()`), `strict` ⇒ `AssetError::DecodeLimit { metric, limit,
observed }`. `rasterizer/image.rs:19` replaces `SizeHint::Native` with
`SizeHint::AtMost{width,height}` derived from the planner; `svg/image.rs:36` likewise (SVG is a
vector source: keep `Native` and record `N/A` for the SVG display target).
`AssetError` gains `DecodeLimit { metric: &'static str, limit: u64, observed: u64 }` and
`RenderError::ImageDecodeLimit(...)` passthrough; `loader/asset_store.rs::decode_raster` adds a
`max_decoded_bytes` ceiling per surface.
Poster (`renderer.ts:6736-6810`): `compiler/placeholders.rs::compile_media_placeholder` paints
`DrawImage` for `poster_path` before the placeholder frame, using the poster box as the demand;
empty `poster_path` keeps today's vector placeholder; an unsupported codec paints a
`DrawText` label (`PICT image unsupported` / `TIFF image unavailable`) instead of dropping the
element.

Tests: new `tests/parity_elements_poster_tests.rs` for the 6 `poster-decode-bomb` cases
(bomb rejected pre-decode with 0 `DrawImage`; 1920×1080 poster ⇒ 1 `DrawImage`; 12_090×9_063 ⇒
`AtMost{1280,960}`; SVG poster fail-open; shared ceiling ⇒ `DecodeLimit`; poster read from the
slide `AssetStore`, which replaces the reference's "deck owner identity"). Extend
`parity_elements_tests.rs`: 32×22 planned axes (`decodedByteBudget 2_816`, box 304_800×209_550 at
960 px), one bounded level (2× 4000² with `budget 1_048_576`, DPR 2, identical targets, product
≤ budget), 109 MP bounded target, strict quota, native-if-fit unchanged, PICT/TIFF labels.
LOC ≈ 380 (probe 90, budget 120, store 40, image 60, poster 70). Risk: the probe must accept
every format the `image` crate decodes or a legitimate file regresses to "unknown" (fail-open:
`None` = no ceiling, decode normally).

## 6. WP-P5 — chart image fills + background parity (follow-on, A5)

Chart `Fill::Image` (15 blocked in `renderer-chart-picture-marker.test.ts`) consumes
`plan_image_fill` + `DrawImage`, with the chart-space `scale * PT_TO_EMU` frame
(`renderer.ts:7509-7540`). No new IR; blocked on WP-P3 + chart `markerFillPaint` plumbing.
Background parity (no blocked case): add `dpi` + `effects` to `VectorBackground::Image`
(currently dropped: `compiler/background.rs:70-100` folds only `alphaModFix`) and route a pattern
background through WP-P1 on the slide grid (`renderer.ts:2358`). ~60 LOC.

## 7. Raster ↔ SVG obligations (both backends, same IR)

| feature | rasterizer | SVG |
| :--- | :--- | :--- |
| pattern | 64×64 tile cache, `Transform::from_scale(k,k)`, `k = cell*scale/64` | `<pattern patternUnits="userSpaceOnUse">` + 64 `<rect>` |
| gradient `scaled`/`tileRect` | `linear_gradient_axis_for` → device points | same helper → `x1..y2` on `userSpaceOnUse` |
| radial `path`/`fill_to_rect` | `radial_gradient_frame_for` → centre+radius | `<radialGradient cx cy fx fy r>` from the same helper |
| gradient/pattern stroke | shader fed to `stroke_path` | `stroke="url(#id)"`, def emitted from the stroke's own bbox |
| line ends | `DrawLineEnd` = fill or stroke a path | `<path>` with `fill` or `stroke` attrs, same transform |
| image fill stretch | existing `render_image` | existing nested-`<svg>` viewport |
| image fill tile | shared `rasterizer/tile.rs` | shared `svg/tile.rs` |
| `DrawImage` effects | `apply_blip_effects` on the decoded slice | re-encode PNG after the same call |

Parity assertion style: `tests/picture_parity_tests.rs` (rasterise the compiled slice, then
serialise, and compare the same geometry twice) — every WP adds one such pair.

## 8. Not applicable (technical reason)

- `renderer-shape-image-fill` #9 "tiled effect fill whose inspection fails natively": asserts the
  canvas-side rejection code `unsupported-image-effect` from `inspectCachedRasterSource`; the Rust
  decoder is synchronous with no separate inspection stage — a failure is a typed `AssetError`
  (covered by #8 instead).
- #17-20 "propagates equation resource failures": no MathJax load/convert stage exists; equation
  fallback is the math WP's.
- `renderer-picture-raster-target` "paints nothing until all decodes settle, then paints in one
  task" and `renderer-adaptive-image-budget` "pipelines the first decode" / "drops a superseded
  canvas render": Canvas/promise scheduling; `TinySkiaRasterizer::rasterize` is a single
  synchronous pass by construction.
- `pattern-thumbnail`'s `OffscreenCanvas` surface and `createPattern().setTransform` trace: the
  observable content is the cell period, asserted at IR level (§2).
- `tile-fill.test.ts` remaining canvas halves: already ported at IR/EMU level (12/12 pass); no
  work here.
- TIFF/EMF/WMF/PICT *decoders*: out of scope (no codec in the workspace and none required by the
  blocked cases); only their placeholder/label surface is in WP-P4.

## 9. File ownership

| agent | files | wave |
| :--- | :--- | :--- |
| A1 IR/gradient/pattern | `contracts/{paint,vector_slide,geometry}.rs`, `compiler/style.rs`, `compiler/pattern_data.rs`(new), `compiler/mod.rs` (call sites + `mod` decls), `compiler/background.rs`, `rasterizer/path.rs`, `rasterizer/pattern.rs`(new + stub), `svg/path.rs`, `svg/pattern.rs`(new) | 1 |
| A2 stroke geometry | `compiler/stroke/{mod,arrow,trim,compound}.rs`(new), `compiler/tables/compound.rs` (extract bands), `tests/parity_text_box_b_highlight_tests.rs` | 1 (pure functions; wires into A1's `compiler/stroke/mod.rs` stub) |
| A3 image fill | `compiler/shapes/image_fill.rs`(new), `compiler/tile.rs`(new), `compiler/pictures.rs`, `compiler/tables.rs`, `rasterizer/tile.rs`(new, moved), `rasterizer/image.rs`, `svg/tile.rs`(new, moved), `svg/image.rs`, `tests/shapes_a_support/image_fill_cases.rs`, `tests/picture_parity_tests.rs` | 2 |
| A4 decode/budget/poster | `loader/{probe,budget}.rs`(new), `loader/asset_store.rs`, `error.rs`, `compiler/placeholders.rs`, `tests/parity_elements_tests.rs`, `tests/parity_elements_poster_tests.rs`(new) | 2 |
| A5 chart + background parity | `compiler/charts/*`, `compiler/background.rs` (shared with A1 after it lands), `rasterizer/background_image.rs`, `svg/background_image.rs` | 3 |

Launch order: **A1** (contracts + gradient/pattern; everything compiles against its frozen
signatures), **A2** (self-contained geometry, only needs the `compiler/stroke/mod.rs` stub), and
**A3** (image fill + tile painters; needs only the §1 types). A4 starts once `error.rs` and
`loader/` are free of A1/A3 edits (it owns them), A5 after A1+A3 land.
