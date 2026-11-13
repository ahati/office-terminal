# Structural parity gaps — implementation spec

Reference `/tmp/oooxml` @ `1f5c6e1d` (read-only). Baseline `crates/kitty-pptx-core/tests/parity/COVERAGE.md` (378 tests, 284 pass, 94 ignored). Gap codes `R1`–`R9` as used in `tests/parity/coverage/elements.md`. Scope: **structural** gaps only — new IR, new seams, new algorithms.

```
G1 typed limits ─┬─> G6 image fills ──> G7 charts ──> G11 EMF/WMF (label only)
                 └─> G3 parse-error card ──> G4 SmartArt contrast ──> G5 multi-column/cell runs
G2 dim overlay (leaf)
G8 scene3d IR + homography ──> G9 bevels/extrusion/wireframe
                           └─> G10 projected effect geometry + reflection blur
```

| gap | blocked | verdict | LOC | | gap | blocked | verdict | LOC |
| :--- | --: | :--- | --: | :- | :--- | --: | :--- | --: |
| G1 typed resource limits | 2 | implement now | 180 | | G7 bubble3D/ChartEx/markers | 17 | subset | 700 |
| G2 dim overlay | 1 | implement now | 40 | | G8 scene3d IR + homography | 4 | implement now | 640 |
| G3 parse-error card | 4 | now (1 N/A) | 220 | | G9 bevel/extrusion/wireframe | 40 | now (29) / OOS (9) | 1450 |
| G4 SmartArt contrast | 12 (+3 to G5) | implement now | 230 | | G10 reflection blur bands | 3 | implement now | 130 |
| G5 multi-column/cell runs | 9 | implement now | 330 | | G11 EMF/WMF | 0 (+1 label) | provably OOS | ~6000 |
| G6 `Fill::Image` | 21 | now (16) / subset (5) | 520 | | | | | |

## G1 — Typed resource-limit errors (R3)

**Blocked** `fonts`/`embedded-fonts.test.ts` 1/9 ("preserves other typed resource-limit errors"); `fonts`/`embedded-fonts-load.test.ts` 3/3 — one `it.each` × 3 rows, only the `4096`/`4097` half portable. **Rust** `ooxml-common/src/resource.rs` owns the vocabulary (`HardResourceLimitKind::wire_fields() -> (stage, resource, metric)`, `ResourceUsage`, `ResourceGovernor`) but
flattens it to a string before any caller: `ResourceGovernor::first_error() -> Option<String>` (:308), `pptx_parser::extract_font(..) -> Result<Vec<u8>, JsValue>` (pptx-parser/src/lib.rs:291). `core::error::ParseError::ResourceLimitHit(&'static str)` (error.rs:33) cannot carry `limit`/`observed`; `AssetError` has no limit variant. **Ref** `core/src/errors/ooxml-error.ts`
(`OoxmlResourceLimitError.details.violation = {stage, resource, metric, part, limit, observed, configurable, usage}`), `image/pixel-budget.ts:132-160` (`OoxmlDecodedImageLimitError`, `getOoxmlDecodedImageLimitDetails`). `embedded-fonts.test.ts:160-165` asserts `rejects.toBe(error)` → a non-font typed limit must survive the font loader **by identity**, never re-labelled
`malformed font`.

```rust
// ooxml-common/src/resource.rs
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)] pub struct ResourceUsage { .. }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct ResourceViolation { pub stage: &'static str,
    pub resource: &'static str, pub metric: &'static str, pub part: Option<String>,
    pub limit: u64, pub observed: u64, pub configurable: bool, pub usage: ResourceUsage }
impl HardResourceLimitKind { pub fn violation(self, limit: u64, observed: u64, usage: ResourceUsage) -> ResourceViolation; }
// kitty-pptx-core/src/error.rs — replaces ResourceLimitHit
ParseError::ResourceLimit(ResourceViolation)     AssetError::ResourceLimit(ResourceViolation)
// kitty-pptx-core/src/loader/embedded_fonts.rs (new)
pub fn load_embedded_fonts(refs: &[pptx_model::PptxEmbeddedFontRef],
    read: &dyn Fn(&str) -> Result<Vec<u8>, AssetError>) -> Result<Vec<LoadedFont>, AssetError>;
// pptx-parser: `pub enum PptxError { Resource(ResourceViolation), Format(String) }`
//   + `From<PptxError> for JsValue` (wire strings only) behind `wasm-entry`
```

**Backends** none (load-side; both backends only see `RenderError`). **Tests** `parity_fonts_tests::a_typed_image_limit_is_not_reported_as_a_malformed_font` (`read` → `Err(AssetError::ResourceLimit(v))`; the returned error `==` `v`) and `…::extract_font_reports_the_measured_archive_entry_limit`: `extract_font(&deck, "ppt/fonts/font3.fntdata", Some(4096), None)` →
`{resource:"archive-entry", metric:"declared-inflated-bytes", limit:4096, observed:4097}`. N/A: worker termination, `FontFace` release, `failureAfterPreflight` (host realm). **Verdict** now · **risk** low (5 construction sites) · **deps** none · **owner** `ooxml-common/src/resource.rs`, `pptx-parser/src/{lib,error}.rs`, `core/src/{error.rs,loader/embedded_fonts.rs}`.

## G2 — Dim overlay

**Blocked** `misc`/`dim-overlay.test.ts` 1/1. **Rust** no `dim` concept (`grep dim_overlay` → 0 hits). **Ref** `renderer.ts:8110-8122` — save → `globalAlpha = opacity` → `fillStyle = color` → `fillRect(0,0,w,h)` → restore; applied after the whole slide (`:9091`), never inspects `slide.hidden`; `types.ts:250-255` `DimOptions {color, opacity}`. Appends the scrim as the **last**
command, folding `globalAlpha × fillStyle-alpha` into the RGBA (reuse `compiler/style.rs::modulate_alpha`).

```rust
// core/src/compiler/dim.rs (new)
pub struct DimOptions { pub color: [u8; 4], pub opacity: f32 }
pub fn apply_dim_overlay(slide: &mut VectorSlide, dim: &DimOptions);
// DrawPath { path: rect_path(0,0,width_emu,height_emu),
//            fill: Some(VectorPaint::Solid(modulate_alpha(dim.color, dim.opacity))) }
```

**Backends** nothing (ordinary `DrawPath`). **Tests** `parity_misc_tests::dim_overlay_paints_one_full_slide_scrim_last`: `"#ffffff"` / `0.6` → `commands.last()` is a `DrawPath` whose bounds equal `(0,0,w_emu,h_emu)` and fill `Some(Solid([255,255,255,153]))` (`0.6×255`); exactly one command appended. **Verdict** now · **risk** none · **deps** none · **owner**
`core/src/compiler/dim.rs` + TUI/CLI `present` call site.

## G3 — Parse-error placeholder + `CompileOptions`

**Blocked** `misc`/`renderer.parse-error.test.ts` 4/7. **Rust** `pptx_model::Slide::parse_error: Option<String>` *exists* (lib.rs:96-103) and so does `slide_number`; `compiler/mod.rs::compile` ignores both, and there is no `CompileOptions` — `SlideCompiler` has a fixed 4-arg signature (mod.rs:26-33), so no per-render option can be threaded. **Ref** `renderer.ts:8188-8274`
`drawParseErrorPlaceholder`: full-slide `#f7f7f8`; `pad = max(12, min(w,h)*.04)`; dashed `#c8ccd2` rect at `pad`, `lineWidth = max(1, min*.004)`, dash `[lw*6, lw*5]`; `⚠` at `(w/2, h*.34)`, `max(18, min*.14)`, `#b23b3b`; heading at `(w/2, h*.52)`, `max(11, min*.045)`, `#333333`; ≤4 word-wrapped detail lines from `(w/2, h*.6+lh)`, `lh = max(9,min*.028)*1.35`, `#666666`, wrap `w
- pad*4`. Short-circuits before any element (`:8477-8487`). Options `errorPlaceholder?`/`errorPlaceholderText?` (`:8142-8161`); title default `Slide {n} could not be displayed` (`:8225`); `detail: null` drops the part tag (`:8229`).

```rust
// core/src/compiler/options.rs (new)
pub struct PartFailure { pub kind: &'static str, pub slide_index: usize, pub slide_number: usize, pub message: String }
pub struct ErrorPlaceholder { pub title: String, pub detail: Option<String> }
pub type ErrorPlaceholderFn = dyn Fn(&PartFailure) -> Option<ErrorPlaceholder> + Send + Sync;
#[derive(Default)] pub struct CompileOptions { pub error_placeholder: Option<Box<ErrorPlaceholderFn>> }
// compiler/mod.rs — DEFAULTED trait method, so no existing impl breaks
fn compile_opts(&self, slide: &pptx_model::Slide, w: i64, h: i64,
                opts: &CompileOptions, sink: &Sender<WarningEvent>) -> Result<VectorSlide, RenderError>;
```

New `compiler/parse_error_card.rs` emits those five groups as `DrawPath`/`DrawText`; sizes pt→EMU via `12700`, `${n}px sans-serif` → default face at `n` pt (CSS px ≈ pt here — state the choice in a comment). **Backends** none. **Tests** (`tests/parity_parse_error_tests.rs`): 1. card emits a run containing `"Slide 3"` **and** one containing `"slide3.xml"` for `parse_error =
"ppt/slides/slide3.xml: unexpected end of stream"` plus a full-slide fill; 2. custom title replaces the heading and `detail: None` hides the part tag; 3. the closure receives `PartFailure{slide_index:2, slide_number:3, ..}` and returns `"Broken slide #3"`; 4. closure never called for a healthy slide. N/A: "rejects for an invalid result" — `ErrorPlaceholder{title: String}` makes
the `TypeError` unrepresentable. **Verdict** now · **risk** medium (touches every `SlideCompiler` caller — keep the method defaulted) · **deps** G1 · **owner** `compiler/{options.rs,parse_error_card.rs,mod.rs}`.

## G4 — SmartArt fallback contrast (issue #805)

**Blocked** `misc`/`smartart-fallback-contrast.test.ts` 12 here (`backgroundLuminance` 4, `smartArtFallbackTextColor` 4, `isSmartArtFallbackShape` 2, renderSlide contrast 2); 3 more (run identity ×2, `hlinkClr="tx"` ×1) go to G5. **Rust** no luminance helper, no fingerprint: `compiler/mod.rs:179` passes only `shape.default_text_color` into `text::compile_text_body`, whose
default style is built at `text/emit.rs:175-189` — that *is* the seam, since a run with no authored colour falls to it. `pptx_model::ShapeElement` has `name`/`id` (lib.rs:547,551) and `pptx-parser/src/smartart_fallback.rs` sets `name: "SmartArt"`, `id: None`. **Ref** `smartart-fallback-contrast.ts`: `hexLuma` = Rec.601, 8-char RGBA composited over white (`alpha*base +
(1-alpha)`); `backgroundLuminance` = solid luma, gradient `first.l*first.p + last.l*(1-last.p) + Σ((l_i+l_{i+1})/2)*(p_{i+1}-p_i)` over stops clamped to `[0,1]` then sorted, `null` for image/pattern/none/absent; `smartArtFallbackTextColor` → `#FFFFFF` iff `bg < 0.5` **and** the theme default's own luma `< 0.5`; `isSmartArtFallbackShape` = `name == "SmartArt" && id == None`.
Wiring `renderer.ts:8523` + `:3560-3566`.

```rust
// core/src/compiler/smartart_contrast.rs (new)
pub fn hex_luma(hex: &str) -> Option<f32>;                       // 6 or 8 chars
pub fn background_luminance(fill: Option<&pptx_model::Fill>) -> Option<f32>;
pub fn is_smart_art_fallback_shape(name: Option<&str>, id: Option<&str>) -> bool;
pub fn smart_art_fallback_text_color(background: Option<&pptx_model::Fill>, theme_default: &str) -> Option<[u8; 4]>;
```

Per slide: `theme_default = slide.theme.default_text_color.unwrap_or("#000000")`; use the result as `default_shape_text_color` **only** for the fingerprint shape when the authored value is `None`. **Backends** none. **Tests** (`tests/parity_smartart_contrast_tests.rs`) the 10 pure cases verbatim (`1F3864 → <0.5`; white `1.0 ±5e-6`; black→white ramp integral `≈0.5`; navy ramp `p
.2/.8 → <0.3`; `00000000 → 1.0`; `000000FF → 0.0`; none/image/pattern/absent `→ None`; navy+`#383838 → #FFFFFF`; white+`#383838 → None`; `#383838` alone `→ None`; navy+`#EEEEEE → None`; fingerprint accept/reject ×3) + 2 renderSlide cases (fallback run `[255,255,255,255]` on dark, unchanged on light; ordinary shape untouched). **Verdict** now · **risk** low · **deps** none
(`SlideTheme::default_text_color` exists) · **owner** `compiler/smartart_contrast.rs` + a 6-line hook in `compiler/mod.rs` (coordinate with G3's owner).

## G5 — Multi-column bodies, cell overflow, text-run identity

**Blocked** `elements`/`multi-column-overflow.test.ts` 2/2 (R6); `elements`/`table-text-selection.test.ts` 7/14 (R5); G4's 3 leftovers. **Rust** `pptx_model::TextBody` already parses `num_col`, `spc_col`, `rtl_col` (lib.rs:1012-1028); `text/emit.rs::compile_text_body_with_markers` lays out one flow and ignores them. Table cell boxing *is* ported — `compiler/tables/clip.rs`
honours `horzOverflow="overflow"` and the six `vert` rows pass BEHAVIOURALLY. Missing: (a) the column plan, (b) run identity. **Ref** `renderer.ts:5616-5676`: `capacity = bh - tPad - bPad`; `trailing = max(0, last.linePx - last.lineHeight)`; `occupied = requiredHeight - trailing`; `fits = (bh == 0) || occupied <= capacity + .5`; `useMultiCol = numCol > 1 && !fits`; `linesPerCol
= multi ? ceil(n/numCol) : n`; advance when `entriesInCol >= linesPerCol`, resetting `cursorY = colTopY`; `xShift = visCol * ((bw-lPad-rPad-(numCol-1)*spcColPx)/numCol + spcColPx)`, `visCol = rtlCol ? numCol-1-colIdx : colIdx`. Identity `renderer.ts:1490-1560` (`PptxTextRunInfo`), cell frames `renderer.ts:7591+`.

```rust
// core/src/text/columns.rs (new) — pure, so the reference's "set of fillText x"
// collapses to an assertion on `dx`
pub struct ColumnPlan { pub line_col: Vec<u16>, pub lines_per_col: usize, pub dx: Vec<f32> }
pub fn plan_columns(n_lines: usize, num_col: u32, box_h: f32, capacity: f32, occupied_h: f32,
                    inner_w: f32, spc_col_px: f32, rtl_col: bool) -> ColumnPlan;
// contracts/vector_slide.rs — a parallel index like `hyperlinks` (no enum churn)
pub struct CellFrame { pub row: u32, pub col: u32, pub row_span: u32, pub col_span: u32, pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
pub struct TextRunInfo { pub shape_id: Option<String>, pub element_index: usize, pub origin: (f32, f32), pub cell: Option<CellFrame> }
pub struct VectorSlide { .., pub text_run_info: Vec<TextRunInfo> }
```

**Backends** both apply `dx` per line (raster shifts the glyph origin, SVG the `<text x>`) and ignore `text_run_info`; no visual change while `num_col == 1`. **Tests** `tests/parity_multicol_tests.rs::four_fitting_paragraphs_stay_in_column_one` (2 cols, 4 short lines, `290×142.64`, insets `108000`, `spcAft 300`, `lnSpc 120%`) → `plan.dx` has exactly **one** distinct value;
`…::real_overflow_uses_both_columns` (8 lines) → exactly **two**, `lines_per_col == 4`. `parity_elements_table_tests`: `text_run_info[i].cell` in row-major order with `(row,col,row_span,col_span)`, empty cells absent, `origin` per cell frame, and for the 30° row the `CellFrame` positions equal `compute_shape_transform(30°)` on the cell box (±1e-4). G4's 2 identity cases:
`shape_id == Some("4")` for a file shape, `None` for the fallback. **Verdict** now · **risk** medium (must land after the line-break kernel settles — `text/emit.rs`, `text/paint.rs` are dirty) · **deps** none · **owner** `text/columns.rs`, `text/emit.rs`, `compiler/tables.rs`. **Coordination** `text/emit.rs` is shared with the text-kernel agent — add one `#[inline]` hook
calling `plan_columns`.

## G6 — Non-background image fill (`Fill::Image`, R7)

**Blocked** `elements`/`renderer-table-image-fill.test.ts` 3/3; `shapes-a`/`renderer-shape-image-fill.test.ts` 18/27. **Rust** `compiler/style.rs:90-94` `Fill::Image { .. } => None` — the fill is dropped. `VectorCommand::DrawImage { rel_id, src_rect, dest_rect, opacity, transform, effects }` already exists with `srcRect` + `BlipEffect`. The **background** blip path is fully
ported (`VectorBackground::Image { src_rect, fill_rect, tile, opacity }`, `compiler/background_image.rs`, `svg/background_image.rs`), so the tile sampler, `fillRect` insets and `tileSourceExtent` are reusable verbatim. **Ref** `renderer.ts:7591+` (`renderTable`) over `:2218-2417`. Paint order for an image-filled table: **whole-table background → per-cell fills (alpha bands) →
per-cell image clips**; the test pins the tail `['drawImage','fillRect','drawImage']`.

```rust
// core/src/compiler/image_fill.rs (new)
pub struct ImageFillPlan { pub rel_id: String, pub src_rect: NormalizedRect,
    pub dest: ImageFillDest,          // Stretched { rect: RectF32 } | Tiled(BackgroundTile)
    pub opacity: f32, pub rot_with_shape: bool, pub effects: Vec<BlipEffect> }
pub fn compile_image_fill(fill: &pptx_model::Fill, box_w: i64, box_h: i64,
                          transform: &AffineTransform) -> Option<ImageFillPlan>;
pub fn emit_image_fill(plan: &ImageFillPlan, tf: &AffineTransform, out: &mut Vec<VectorCommand>);
```

`compile_fill` stays `Option<VectorPaint>` (an image is not a paint); table/shape compilers call `compile_image_fill` *before* the path fill. Blip sharing is a `rel_id` LRU hit — already the Rust default (`DefaultAssetStore`). **Backends** raster already honours `src_rect`/`dest_rect`/`opacity`/`effects`; SVG expands a tiled plan into exact `<pattern>` tiles (reuse the `tile-*`
port). **Tests** un-ignore table case 1: `src_rect == {x:.5,y:0,w:.5,h:1}` for the `srcRect.l=.5` cell and the tail exactly `[DrawImage(table bg, dest 96×96), DrawPath(cell solid [255,0,0,128]), DrawImage(cell, opacity .25, src_rect l=.5)]`. Keep the 2 budget cases (`resizeWidth/Height`, `ooxml-decoded-image-limit`) BLOCKED on the decode planner (R1/R2) — do **not** re-label.
`renderer-shape-image-fill`: 13 of 18 (crop, insets, `rotWithShape`, opacity, blip routing) become implementable; 5 (tile scratch budget, unbounded-tile rejection) stay blocked. **Verdict** now for 16/21 · **risk** medium (new paint order in the table compiler) · **deps** G1 · **owner** `compiler/{image_fill.rs,style.rs,tables.rs}`, `svg/image.rs`.

## G7 — Chart families: bubble3D, ChartEx seam, picture markers (R8)

**Blocked** `elements`/`renderer-chart-bubble3d.test.ts` 2/2; `elements`/`renderer-chart-picture-marker.test.ts` 15/15. Already correct: unsupported families paint the reference `"Unsupported chart"` card at `#888`/11 pt centred (`compiler/charts/mod.rs:130-165`, ref `core/src/chart/renderer.ts:318-327`). **Rust** `ChartFamily::Bubble` is `!is_supported()`
(`compiler/charts/types.rs`), so bubbles hit the placeholder; no ChartEx injection point; no `Fill::Image` on markers. Reference painters are large: `scatter.ts` 713, `radar.ts` 414, `stock.ts` 794, `surface.ts` 1516, `chart-ex-renderer.ts` 2565, `three-d-renderer.ts` 4953. **Ref** bubble3D `core/src/chart/marker-style.ts:45-54` (`series.bubble3D ??
series.bubble3DGroupDefault`); the paint is **3** `createRadialGradient`s with **4, 5, 6** stops composited `source-atop` onto the already-painted disc (`renderer-correctness.test.ts:13406`). ChartEx seam `renderer.ts:4418+` (`chartEx.render(ctx, chart, rect, ptToPx, rotationDeg)`; order vs `threeD`/`regionMap` is observable). Markers `core/src/chart/image-fill.ts` +
`image-fill-geometry.ts`.

```rust
// contracts/pipeline.rs — compile-time seam
pub trait ChartExRenderer: Send + Sync { fn render(&self, chart: &ooxml_common::chart::ChartModel, rotation_deg: f32) -> Option<Vec<VectorCommand>>; }
pub struct CompileOptions { .., pub chartex: Option<Arc<dyn ChartExRenderer>> }   // from G3
// compiler/charts/bubble.rs (new)
pub struct BubbleMaterial { pub stops: Vec<(f32, [u8; 4])>, pub clip: VectorPath }
pub fn bubble3d_materials(series: &ChartSeries, cx: f32, cy: f32, r: f32) -> Option<Vec<BubbleMaterial>>;
pub fn compile_bubbles(chart: &ChartModel, layout: &ChartLayout, bounds: &ChartDataBounds, tf: &AffineTransform) -> Vec<VectorCommand>;
```

`source-atop` needs **no** blend mode in the IR: in a display list the reference's composite is exactly "confine this gradient to the disc already painted" → `PushClip(disc) … PopClip` (record the mapping in a comment). **Backends** none — both already consume `RadialGradient` + `PushClip`/`PopClip`; marker image fills reuse G6. **Tests**
`parity_elements_chart_tests::bubble3d_material_has_three_bounded_ramps` → `len() == 3`, stop counts `[4,5,6]`, `atan2(sx,ky) ≈ π/2` (±1e-4) for the 90° chart rotation; the reference's `compositeModes.filter(=='source-atop').len() == 3` maps to "3 `PushClip`/`PopClip` pairs, each wrapping one radial-gradient fill".
`…::chartex_renderer_is_forwarded_once_with_the_chart_and_rotation` → recording impl sees `calls == 1`, `rotation_deg == 0.0`, its commands in the stream. Markers: 6 of 15 (crop, DPR dedup, tiled-vs-stretched, invalid frame) are pure geometry → implementable; 9 need the decode plan (`targetWidthPx`, `decodedByteBudget`, duotone fail-closed) → stay BLOCKED. **Verdict** tested
subset. Bubble3D + ChartEx + marker geometry: yes. Full `Scatter`/`Radar`/`Stock`/`Surface`/`ThreeD`/`ChartEx` painters: **provably out of scope this milestone** — 9.7k + 2.6k + 5.0k TS lines of canvas-2D-specific point/label/legend planning. **Risk** high for painters, medium for the subset · **deps** G3, G6 · **owner** `compiler/charts/{bubble.rs,markers.rs,mod.rs,types.rs}`,
`contracts/pipeline.rs`.

## G8 — scene3d IR + camera homography + projected effect geometry

**Blocked** `shapes-b`/`shape-effect-transform.test.ts` 4/9 (cases 3-6); enables G9/G10. **Rust** `contracts/shape_effects.rs:122-139` `VectorScene3d { camera_type, fov_deg, light_rig_type, light_direction_deg }`, `VectorShape3d { extrusion_*, contour_*, bevel_top_{width,height,type} }`. `scene3d.rs::camera_angles` invents a 20-entry preset table of hardcoded ±35.264°/±45°
values with **no `zoom`, no `rot`, no `rev`, no scene depth**; `camera_affine` then approximates the homography by an affine fit to three corners (documented DEGRADED policy). `pptx_model::{Camera3d, Rot3d, LightRig}` already carry `fov`, `zoom`, `rot{lat,lon,rev}`, `light_rig.rot` (lib.rs:362-405) — all discarded at stage 1. **Ref** `core/src/shape/scene3d-camera.ts:154-250` —
44-entry `PRESETS` `{kind, baseLat, baseLon, baseRev, fovDeg, sceneInches, shearX, shearY}`, `MODERN_SCENE_INCHES = 6.28`; `cameraDistance` with the 2 %-of-volume near guard `max(distance, nearest + .02*volume)`; `R = Rz(-rev)·Rx(-lat)·Ry(-lon)`; perspective `f = d/(d-rz)`; parallel `rx + rz*shearX`. `computeScene3dQuad` (397-470) → `{corners, isAffine, isIdentity}`, `eps =
1e-3*max(w,h)`, affine test `(C0+C2) == (C1+C3)`. `isScene3dNonIdentity` (472-522), `scene3dSurfaceFrame` (524-533), `computeDepthOffset` (546-590). `renderer.ts:3689-3718` `projectedEffectGeometry` over `projectedSurface` (3669-3687, back-face winding test + `expandProjectedQuad`). `core/src/shape/scene3d-draw.ts:40-79` `unitSquareToQuad` (Heckbert §2.2, affine branch at
`|dx3|,|dy3| < 1e-12`), `411-459` `expandProjectedQuad` (horizon guard `w > 1e-9`), `460-476` `projectQuadPoint`.

```rust
// contracts/shape_effects.rs
pub struct VectorRot3d { pub lat_deg: f32, pub lon_deg: f32, pub rev_deg: f32 }
pub struct VectorBevel { pub width_emu: i64, pub height_emu: i64, pub preset: String }
pub struct VectorScene3d { pub camera_type: String, pub fov_deg: f32, pub zoom: f32, pub rot: Option<VectorRot3d>,
    pub light_rig_type: String, pub light_dir: String, pub light_rot: Option<VectorRot3d>, pub units_per_inch: f32 }
pub struct VectorShape3d { pub extrusion_depth_emu: i64, pub extrusion_color: Option<[u8;4]>,
    pub contour_width_emu: i64, pub contour_color: Option<[u8;4]>, pub prst_material: String,
    pub bevel_top: Option<VectorBevel>, pub bevel_bottom: Option<VectorBevel> }
// scene3d.rs -> scene3d/{camera,presets,homography,effect_geometry}.rs
pub struct SceneCamera<'a> { pub prst: &'a str, pub fov_deg: Option<f32>, pub zoom: f32,
    pub rot: Option<VectorRot3d>, pub units_per_inch: f32, pub scene_depth: f32 }
pub struct SceneQuad { pub corners: [(f32, f32); 4], pub is_affine: bool, pub is_identity: bool }
pub fn compute_scene3d_quad(cam: &SceneCamera<'_>, w: f32, h: f32, surface_depth: f32) -> SceneQuad;
pub fn is_scene3d_non_identity(cam: &SceneCamera<'_>, w: Option<f32>, h: Option<f32>, depth: f32) -> bool;
pub fn compute_depth_offset(cam: &SceneCamera<'_>, w: f32, h: f32, depth_px: f32) -> (f32, f32);
pub fn unit_square_to_quad(c: &[(f32, f32); 4]) -> Option<[f64; 9]>;
pub fn project_quad_point(c: &[(f32, f32); 4], u: f32, v: f32) -> Option<(f32, f32)>;
pub fn expand_projected_quad(c: &[(f32, f32); 4], fx: f32, fy: f32) -> Option<[(f32, f32); 4]>;
pub fn projected_effect_geometry(cam: &SceneCamera<'_>, tf: &AffineTransform, x: f32, y: f32, w: f32, h: f32,
    edge_pad_css: f32, align: &str, back_depth_css: f32) -> (RectF32 /*bbox*/, (f32, f32) /*anchor*/);
```

**Backends** raster replaces the 3-corner affine with a true warp (body → scratch `Pixmap`, sampled through `unit_square_to_quad`, bilinear; affine quads keep the fast `tiny_skia::Transform` path). SVG emits an exact `<g transform="matrix(..)">` while `is_affine`; otherwise `WarnEvent::FidelityDegraded{feature:"scene3d_perspective"}` **and** the affine approximation — never
silently flatten. **Tests** (`tests/parity_shapes_b_tests.rs`, un-ignore 4): 1. `authored_shadow_anchor_is_distinct_from_the_projected_aabb_corner` (`orthographicFront; a=0,b=1,c=-1,d=0,e=75,f=-25; 0,0,100,50; pad 0; 'tr'`) → `bbox == (25,-25,50,100)`, `anchor == (75,75)`; 2. `raster_padding_rounds_up_to_device_pixels_under_camera_zoom` (`zoom 4`, tf scale 2, pad `1.1`) →
`bbox.x ≈ -312`, `bbox.w ≈ 824`; 3. `exposed_back_surface_is_bounded_at_the_extrusion_base` (`orthographicFront rot lon=150`, `back_depth 40`) → `bbox.x ≈ 70 - 50·cos30`, `bbox.w ≈ 50·√3`, `anchor == (bbox.x, 0)`; 4. `camera_crossing_rear_face_keeps_one_finite_scene` (`perspectiveFront fov 90 rot lon 120`, `768×96`, `depth 768`, tf `a=d=2,e=5,f=7`) → all four `bbox` fields +
anchor match the closed-form quad to 1e-7. N/A: cases 7-9 (`cacheDevicePaint`) — no per-render offscreen cache in Rust. **Verdict** now · **risk** high (affine→homography moves every 3-D pixel test; `partly_off_viewport_shape_stays_in_the_projection_path` pins the *old* ±35.264°/±45° parallelogram and must be re-derived from `PRESETS`, not tweaked) · **deps** none · **owner**
`contracts/shape_effects.rs`, `scene3d.rs` → `scene3d/*`, `compiler/shape_effects.rs`, `rasterizer/shape_effect.rs`, `svg/filters.rs`.

## G9 — 3-D bevels, extrusion walls, wireframe rims

**Blocked** `shapes-b`/`bevel-band-conversion.test.ts` 2/4; `shapes-b`/`shape-orthographic-bevel.test.ts` 38/39. **Rust** `rasterizer/bevel.rs` (126 lines) clamps the band to `clamp(0.0, 48.0)` px, uses a 2-pass chamfer distance transform, a 5-entry `bevel_curve` (`scene3d.rs:186-215`), a synthetic `light_vector` (one azimuth, fixed 45° elevation) and a hardcoded
`Material::from_preset("plastic")`. No extrusion wall, wireframe, bottom bevel, contour, `prst_material` or light-rig table; `shape_effect.rs::effect_padding` mirrors the same 48 px / 120 px clamps. **Ref** `renderer.ts:6302-6348` `buildBevelInputs`: `widthPx = bevelT.w * scale * devScale`, gated on `w > 0 && h > 0`; `bevelB` adds `depthPx = extrusionH * emuToDev`;
`prstMaterial === 'legacyWireframe'` synthesises zero-width inputs for both faces so wireframe has geometry with no bevel. `renderer.ts:6348+` `buildExtrusionInput`; offscreen order (`:6590-6600`): `applyExtrusion` → front `applyBevelShading` → bottom `applyBevelShading` when the camera exposes the back face. `bevel-profiles.ts` — 13 `CURVES` → 1024-interval height/slope LUTs,
cubics split into 256 segments, overhang resolved by greatest height along the front ray. `bevel-light-rigs.ts` — 23 `MODERN` rigs `{ambient, lights[{rgb, direction, scale, specular, diffuse}]}`. `bevel-lighting.ts` — 15 `MATERIALS` (`specularShape/White`, `diffuseShape/White/Alpha`, `ambientAlpha`, `emissive`, `power`, `blinn`, `diffuseFresnel`, `alphaFresnel`), `rigVector`
(whole rig rotated by `Rz(rev)·Rx(lat)·Ry(lon)` on the MS-OI29500 §2.1.1274 negated matrix), `prepareLighting`/`shadeSurface`. `bevel-shading.ts` — `distanceToEdge` (EDT1D+gaussian), `computeBevelNormals`, `shadePixel`, `applyBevelShading`; `applyExtrusion` (509-591: sweep the front silhouette by the centre depth offset, fill uncovered pixels with the wall colour, `< 0.75 px` →
no-op). `bevel-wireframe.ts` `rasterBevelWireframe` (85 lines).

```rust
// rasterizer/bevel/ -> {inputs,profiles,rigs,lighting,shading,extrusion,wireframe}.rs
pub struct BevelInput { pub width_px: f32, pub height_px: f32, pub preset: String, pub material: MaterialKind,
    pub light: RigVector, pub bottom: bool, pub depth_px: f32, pub wireframe_line_px: f32 }
pub struct ExtrusionInput { pub offset_x: f32, pub offset_y: f32, pub rgb: [u8; 3] }
pub fn build_bevel_inputs(sp3d: Option<&pptx_model::Sp3d>, rig: Option<&pptx_model::LightRig>,
                          scale: f32, dev_scale: f32) -> Vec<BevelInput>;
pub fn build_extrusion_input(sp3d: Option<&pptx_model::Sp3d>, cam: Option<&SceneCamera<'_>>,
                             w: f32, h: f32, scale: f32, dev_scale: f32) -> Option<ExtrusionInput>;
pub fn apply_bevel_shading(layer: &mut [u8], w: usize, h: usize, input: &BevelInput);
pub fn apply_extrusion(layer: &mut [u8], w: usize, h: usize, input: &ExtrusionInput);
pub fn raster_bevel_wireframe(layer: &mut [u8], w: usize, h: usize, input: &BevelInput);
pub fn profile_height(preset: &str, t: f32) -> f32;
pub fn profile_slope(preset: &str, t: f32) -> f32;
pub fn rig_vector(rig: &str, dir: &str, rot: Option<&VectorRot3d>) -> RigVector;
pub fn shade_surface(normal: [f32; 3], lighting: &Lighting) -> [f32; 3];
```

Delete the 48 px / 120 px clamps (the reference has none). **Backends** raster: full pixel pipeline. SVG: contour as a stroked path; extrusion/bevel as `WarnEvent::FidelityDegraded{feature:"shape3d_bevel"}` with a flat face — SVG has no distance-field shading (documented, testable degradation). **Tests** — replace `bevel_band_scales_linearly_with_device_scale` with
`bevel_band_width_is_w_times_scale_times_dev_scale_{1,4,8}x`: pure `build_bevel_inputs(&sp3d, rig, 1920.0/12_192_000.0, dev_scale)` → `width_px` within ±1 of `304_800 * css_scale * dev_scale` (48 / 192 / 384) and `width_px / dev_scale ≈ 304_800 * css_scale` to 1e-6; add `bevel_is_omitted_when_w_or_h_is_zero` (`{w:0,h:100}`, `{w:100,h:0}` → empty). `shape-orthographic-bevel`:
**27 of 38** are self-contained numeric assertions → implementable: `applies translucent material once` ×2 (contrasting rig: `ambient=1`, no diffuse, flat alpha .7, so `(80,100,140)` over white is `(133,147,175) ±1`), `100×1000 under a one-degree perspective tilt` (near edge ~4 px above `y=80`), `back bevel only when the camera exposes the back face`, face-on inset no-op ×2,
camera-crossing side ×2, extrusion binding ×2, `circle` convex rim ×2, wireframe rim with transparent interior, wireframe topology ×2, oblique depth ×4, oblique soften ×4, extruded back face ×2, physical framing ×2 (independent pinhole bounds), body-opacity independence ×2. **9 provably out of scope**: `preserves overflowing projected text` ×9 build a *reference-renderer
oracle* in `page.evaluate` and compare ink counts/bounds against it; `cargo test` has no JS runtime or Chromium and the expected numbers are not spec-derivable → `#[ignore]` with that reason. **Verdict** now for 29/40, 9 out of scope · **risk** high (largest single change; rewrites the existing bevel tests) · **deps** G8 · **owner** `rasterizer/bevel/*`, `scene3d/*` (share
`presets.rs`), `compiler/shape_effects.rs`, `rasterizer/shape_effect.rs`.

## G10 — Distance-aware reflection blur

**Blocked** `shapes-b`/`reflection-blur.test.ts` 3/3. **Rust** `rasterizer/shape_effect/reflection.rs` (195 lines) uses one uniform Gaussian over the whole mirrored band; no band plan. **Ref** `packages/pptx/src/reflection-blur.ts:29-75` `reflectionBlurBands` (not exported — the port must publicise it): `count = max(4, min(24, ceil(maxBlur/0.5)+1))`; `sample =
sqrt(i/(count-1))`, `near = i==0 ? 0 : (prev+sample)/2`, `far = i==count-1 ? 1 : (sample+next)/2`; `y = bottom - far*h`, `h = (far-near)*h`, `radius = maxBlur*i/(count-1)`; `maxBlur <= 0 || h <= 0` → one band `{y, h: max(0,h), radius: 0}`. Painter `:78-92` per band: `save → rect(0,y,canvasW,h) → clip → filter → drawImage → restore`.

```rust
// rasterizer/shape_effect/reflection_blur.rs (new)
pub struct ReflectionBand { pub y: f32, pub h: f32, pub radius: f32 }
pub fn reflection_blur_bands(bounds: (f32, f32, f32, f32), max_blur: f32) -> Vec<ReflectionBand>;
pub fn paint_distance_aware_reflection_blur(dst: &mut tiny_skia::Pixmap, src: &tiny_skia::Pixmap,
    bounds: (f32, f32, f32, f32), max_blur: f32, canvas_w: f32);
```

**Backends** raster only; SVG approximates with one `feGaussianBlur` at `max_blur` and records the degradation. **Tests** (`tests/parity_shapes_b_tests.rs`, un-ignore 3): 1. `reflection_blur_ramps_to_the_far_edge` — `(10,20,100,40)`, `maxBlur 2`: `len() == 5`, `radius[0] == 0`, `radius[4] ≈ 2` (1e-10), non-decreasing, first band clip `y > 20`, last band `y+h ≈ 20`, the band
containing `y = 40` has `radius ≈ 0.5` (±0.05), all radius steps equal to 1e-10 and `<= 0.5`, first band height `>` last band height; 2. `the_pass_cap_bounds_adjacent_radius_changes` — `(0,0,100,80)`, `maxBlur 100` → `len() == 24`, max step `≈ 100/23` (1e-10); 3. `one_sharp_pass_when_no_blur_is_authored` — `maxBlur 0`, `(0,5,20,10)`: exactly one band, `radius == 0`, `y == 5`,
`h == 10`. **Verdict** now · **risk** low · **deps** none · **owner** `rasterizer/shape_effect/reflection_blur.rs`, `rasterizer/shape_effect/reflection.rs`.

## G11 — EMF / WMF metafile players

**Blocked** zero reference cases in `COVERAGE.md` → nothing to un-ignore. The only touched rows are `renderer-picture-raster-target`'s `PICT image unsupported` label and the `mimeType: 'image/emf'` "image-planning" site. **Rust** `loader/asset_store.rs::decode_raster` calls `image::load_from_memory`, which has no EMF/WMF support → `AssetError::DecodeFailed`; no metafile player,
DIB decoder or device-context state machine. **Ref** `core/src/image/emf.ts` 2745, `wmf.ts` 1281, `wmf-text.ts` 845, `dib.ts`, `emf-plus.ts`, `raster-or-metafile.ts` 305 — full `[MS-EMF]`/`[MS-WMF]` record interpreters (world→page→device→target chain, ~20 `MapMode` variants, object table, clip regions, symbol-font text out, EMF+ dual-stream validation) plus five spec files.
**Verdict. Provably out of scope this milestone.** Technical reason: the reference support is a *device-context interpreter*, not a decode adapter — 5176 TS lines and a dedicated corpus, with the fidelity contract "return what playback drew, with detected gaps attached" (`raster-or-metafile.ts:30-50`). A 1:1 port is a standalone ≈6k-LOC crate the parity harness cannot validate
(zero EMF/WMF cases). In-scope residue is the **label**: `placeholders::compile_picture_placeholder("PICT image unsupported", ..)` for `image/x-pict`, and `AssetError::UnsupportedFormat("image/emf")` + `WarnEvent::MediaFailed` instead of `DecodeFailed` — that is G1 + G6 work. **Deps** G1, G6 · **owner** unassigned; do not schedule.

## Launch order (parallel-safe)

**Agent 1 — `agent-a-options-errors`** (G1 + G2 + G3). No dependencies; owns the seams every later gap calls into. `ooxml-common/src/resource.rs`, `pptx-parser/src/{lib,error}.rs`, `core/src/error.rs`, `core/src/compiler/{options.rs,parse_error_card.rs,dim.rs,mod.rs}`, `core/src/loader/embedded_fonts.rs`, `tests/parity_{parse_error,dim_overlay}_tests.rs`,
`tests/parity_fonts_tests.rs`.

**Agent 2 — `agent-b-scene3d`** (G8 → G9 → G10). 3-D vertical only → no conflict with Agent 1. `core/src/contracts/shape_effects.rs`, `core/src/scene3d.rs` → `core/src/scene3d/{camera,presets,homography,effect_geometry}.rs`, `core/src/compiler/shape_effects.rs`, `core/src/rasterizer/{shape_effect.rs,bevel/*}`, `core/src/rasterizer/shape_effect/reflection_blur.rs`,
`tests/parity_shapes_b_tests.rs`.

**Agent 3 — `agent-c-imagefill`** (G6 → G7). Needs Agent 1 only for `AssetError::ResourceLimit`. `core/src/compiler/{image_fill.rs,style.rs,tables.rs}`, `core/src/compiler/charts/{bubble.rs,markers.rs,mod.rs,types.rs}`, `core/src/svg/image.rs`, `core/src/contracts/pipeline.rs`, `tests/parity_elements_{table,chart}_tests.rs`, `tests/parity_shapes_a_tests.rs`.

G4 + G5 (`agent-d-text`) start once Agent 1 lands `CompileOptions`: G4 edits `compiler/mod.rs` (Agent 1's file) and G5 edits `text/emit.rs`, currently owned by the text-kernel work.
