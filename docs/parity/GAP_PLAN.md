# GAP_PLAN — 1:1 render parity, implementation plan

Synthesis of `docs/parity/gaps/{text,paint,structure,platform}.md` against the tree at `4374656` +
uncommitted wave-2 work. Reference: `/tmp/oooxml` @ `1f5c6e1d`; corpus @ `2ddb63be403d`. **(measured)**
= produced by running the tree, not copied from a report; line numbers will drift.

---

## 1. State of play

### 1.1 Reference inventory

| suite | size | what it pins |
| :--- | :--- | :--- |
| `packages/pptx/src/*.test.ts` | **150 files, 1008–1074 `it()`** (≈454 renderer-path, ≈419 viewer/scroll/worker, ≈135 model/API) | canvas-op sequences, pure kernels, viewer state machines |
| `packages/pptx/tests/visual/*.spec.ts` | **14 Playwright specs / 25 test blocks** | pixels: VRT deck, bevel ring, warped strips, rotated tiles, table image fill, part failure |
| upstream Rust `#[test]` | **395** | vendored parser/model (byte-identical, not ours to port) |

Two port scopes, disjoint test trees:

| scope | reference files | cases | Rust home |
| :--- | --: | --: | :--- |
| **render parity** (`COVERAGE.md`) | 88 | **728** | `crates/kitty-pptx-core/tests/parity_*.rs` |
| **platform** (`platform.md` §A–F) | 72 (+14 PW) | 461 vitest + 25 PW blocks | `core/tests/{navigation,selection,find,hyperlink,part_failure,prefetch_pipeline}_tests.rs`, `tui/tests/*`, `display/tests/*`, `cli/tests/*` |

### 1.2 Ported today **(measured)**

`cargo test --test parity_*`: **27 targets, 403 tests, 336 pass, 67 ignored, 0 fail**; crate-wide 674
integration tests in `tests/` and 137 unit tests in `src/`.

| batch | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | ledger pass |
| :--- | --: | --: | --: | --: | --: | --: |
| `elements` | 89 | 3 | 20 | 8 | 58 | 24 |
| `fonts` | 104 | 9 | 13 | 78 | 4 | 22 |
| `misc` | 37 | 1 | 8 | 8 | 20 | 10 |
| `shapes-a` | 36 | 4 | 4 | 6 | 22 | 11 |
| `shapes-b` | 65 | 0 | 8 | 3 | 54 | 8 |
| `text-box-a` | 112 | 22 | 32 | 1 | 57 | 41 |
| `text-box-b` | 129 | 30 | 9 | 6 | 84 | 45 |
| `text-flow-a` | 70 | 44 | 8 | 0 | 18 | 53 |
| `text-flow-b` | 86 | 50 | 17 | 0 | 19 | 68 |
| **total** | **728** | **167** | **119** | **110** | **332** | **282** |

Ledger totals are **stale**: it records 94 ignores / 309 pass, the tree has 67 / 336 — wave 2 flipped
27 test functions without re-deriving case rows. Plan of record: ≥286 ported, ≤305 BLOCKED,
**re-derived once (task L0, wave 5)** rather than trust either number. Platform scope: 0 of ≈150
portable vitest cases and 0 of 12 portable Playwright blocks are ported; ≈18 cases + 12 blocks
already have an EXACT Rust analogue (`platform.md` §A–F).

### 1.3 BLOCKED, by spec (each spec's own count; image fill and charts appear in two specs)

| spec | blocked | note |
| :--- | --: | :--- |
| `text.md` | 160 (141 open) | T1 54, T2 14→3 open, T3 22, T4 15→7 open, T5 6, T6 3, T7 7, T8 14, T9 11, T10 14 |
| `paint.md` | 68 | image fill 13, pattern 1, gradient `rotWithShape` 1, gradient/pattern stroke 1, arrowheads 6, `cmpd` 2, decode/budget/poster 29, chart 15 |
| `structure.md` | 113 | G1 2, G2 1, G3 4, G4 12, G5 9, G6 21, G7 17, G8 4, G9 40, G10 3, G11 0 |
| `platform.md` | ≈150 | hidden 15, find 4, selection 10, part-failure 5, pipeline ≈33, container 5, zoom 19, hyperlink 5, rest EXACT |

Double-counted: G6∩paint image fill (13), G7∩paint chart (15) → merged into P3/P5. N/A total:
**110** render cases (78 of them the `fonts` browser-decode rows) + 21 platform files (~311 cases).

### 1.4 Wave-2 landing status

| work | state |
| :--- | :--- |
| `text/{bidi,slots,east_asian_default,east_asian_coverage,paragraphs,decor,stacked,bullets}.rs` | **landed uncommitted**; `parity_text_flow_b_bidi_tests` (15) and `parity_fonts_slot_tests` (10) green |
| corpus harness (`external_fixture_support/`, `fixtures.json` 419 decks, `generate_manifest.py`) | **landed uncommitted** |
| `contracts/paint.rs` (+130), `compiler/pattern.rs` (new, 119), `compiler/style.rs` (+154), `compiler/mod.rs` | **in flight** — `VectorPaint::{Image,Pattern}`, `VectorImageFill`, `VectorPattern` exist; **tree does not compile** (`rasterizer/path.rs`, `svg/path.rs` lack the new arms) |
| image fill backends, gradient `FillFrame`, 3-D, viewer tests | **not on disk** — files unclaimed |

`compiler/pattern.rs` settles a spec error: the pattern cell is `12_700/8 = 1587.5` EMU and the
8×8 tile period is **12 700 EMU (1 pt)**, not `PATTERN_CELL_EMU = 101_600` as `paint.md` §1/§2
claim (`paint.ts:487-495`: `sampleToUser = ptToUserUnits / 8` is *per bitmap pixel*; `paint.md`'s own
test formula `(240/9_144_000)*12700/8` agrees with 1587.5). P1 must correct the spec text and the
shapes-a assertion before P3/P5 read it.

---

## 2. Dependency graph

### 2.1 Contract gates (a change here blocks every consumer until it lands)

| gate | file (region) | owner | consumers | must land |
| :--- | :--- | :--- | :--- | :--- |
| **C1** | `contracts/text.rs` — `Decoration`, `PositionedGlyphRun::{underline,strike}: Option<Decoration>`, `hyperlink: Option<String>` | T7 | T9, T7's stacked half | T7's **first commit**, wave 4 |
| **C2** | `contracts/paint.rs` — `VectorStroke::{miter_limit,paint,head_end,tail_end,compound}`, `LineEnd*`, `CompoundLine`; `contracts/vector_slide.rs` — `DrawLineEnd` | P1 | P2, P3, P5 | wave 3 (partly landed; stroke half open) |
| **C3** | `contracts/shape_effects.rs` — `VectorRot3d`, `VectorScene3d::{zoom,rot,light_rot,units_per_inch}`, `VectorShape3d::{prst_material,bevel_bottom}`, `VectorBevel` | S6 | S7a, S7b | wave 3, day 1 |
| **C4** | `contracts/vector_slide.rs` — `TextRunInfo`, `CellFrame` (parallel index, no enum churn) | S5 | — | wave 5 |

Region sharing inside one file is allowed: `contracts/vector_slide.rs` = P1 owns the image/line-end
region, S5 owns the text-info region. `text/emit.rs` = T1 owns `StyleDef`/`Measurer`, T3 owns the
line loop, T5 one `BreakOptions` literal, T6 one `do_wrap` line, S5 one `plan_columns` hook.

### 2.2 DAG

```
C1 (T7) ────────────────────────────────> T9
C2 (P1) ──────────────> P2, P3 ────────> P5
C3 (S6) ──> S7a ──> S7b
C4 (S5) ──> (none)

T1 ──┬─> T2 ──> T8 ──┐
     ├─> T7 ──────────┼─> T9
     ├─> T6           │
     └─> T10 <── T3   │
T3 ──┼─> T4 ──> T5 ──┼─> S5 <── S4
     └─> T6           │
S1 ──┬─> S3 ──> S4 ──┘
     ├─> P3 ──> P5
     ├─> P4
     └─> V4

leaves: S2, S8, V1, V2, V3, V5, V6, V7, V8
```

Edge list (a→b: b cannot start): T1→T2,T7,T6,T10 · T2→T8,T6 · T3→T4,T5,T6,T10,S5 · T4→T5,S5 ·
T5→S5 · T7→T9 · T8→T9 · P1→P2,P3 · S1→S3,P3,P4 · S3→S4,P5,V4 · P3→P5 · S6→S7a→S7b · S4→S5.

### 2.3 Topological order

- **L0** (no deps): T1, T3, P1, S1, S2, S6, S8, V1, V2, V3, V5, V6, V7, V8
- **L1**: T2, T4, T7, T10, S3, S7a, P2, P3, P4
- **L2**: T5, T6, T8, S4, S7b, P5, V4
- **L3**: T9, S5

Wave assignment must respect levels; waves 3–5 below do (T2 and P2 start early in a worktree
against frozen signatures, which is the only way to keep the chain T1→T2→T8→T9 inside 3 waves).

## 3. Workstream table

`cases` = blocked reference cases (un-ignore count → new test count). `*` = new file.
Effort = spec LOC ÷ 180, rounded to 0.5 d.

| id | goal | files owned | APIs added | cases | deps | d | risk |
| :--- | :--- | :--- | :--- | :--- | :--- | --: | :--- |
| **T1** | static face catalogue + PowerPoint line model | `text/{faces,power_point}.rs`*, `text/metrics.rs`, `text/emit.rs` (`StyleDef`/`Measurer`), `assets/fonts/office-face-metrics.json` | `power_point_ascent_share`, `power_point_face_metrics`, `FaceMetricsFull{share,glyph,excel,decoration}`, `SHARE_CACHE_LIMIT`, `compat_off_natural_line`, `font_algn_{reference,offset}`, `is_east_asian_face`, `is_serif_latin_face` | 54 → 2 un-ign + 53 new | — | 3 | med (shifts every baseline ≈0.5 pt) |
| **T2** | `vert` feature model + U+FF1A class (slots landed) | `text/vertical/eavert.rs`*, `text/{slots,east_asian_default}.rs`, `assets/text/east-asian-defaults.json` | `vert_feature_covers(face,cp,bold)`, `orientation_class(cp)` FF1A row | 10 (7 enable now) + 3 | T1 | 1 | low |
| **T3** | `rPr@spc` end-to-end: clusters, run seam, justified splits | `text/{linebreak,tabs,paint}.rs`, `text/emit.rs` (line loop) | `StyleDef::source_run_id`, `Measurer::seam_advance`, `spacing_advance` over grapheme clusters, per-piece `ls` in `place_text` | 22 → 1 un-ign + 21 new | — | 1.5 | med (`source_run_id` changes coalescing) |
| **T4** | RTL tab READING frame + finite limit (L2 paint landed) | `text/{tabs,paint}.rs`, `text/emit.rs` (line loop) | `BreakOptions::tab_start_pen`, `resolve_line_tabs(limit)` under a base-RTL frame | 15 → 7 open | T3 | 1 | high (reorders every RTL line) |
| **T5** | kinsoku across run seams + absolute tab pen | `text/linebreak.rs`, `text/emit.rs` (1 literal) | `BreakOptions::{kinsoku_across_runs,tab_start_pen}`, `measure_items` uses `marL + textXOffset` | 6 → 1 un-ign + 5 new | T3, T4 | 0.5 | low |
| **T6** | `spAutoFit` preflight | `text/preflight.rs`*, `text/emit.rs` (1 line) | `natural_width_exceeds_bbox(paras,box_w,l_ins,r_ins,measurer)` | 3 new | T1, T2, T3 | 0.5 | low |
| **T7** | glyph decorations in the IR (**C1**) | `contracts/text.rs`, `text/paint.rs`, `rasterizer/text.rs`, `svg/text.rs` | `Decoration`, `Option<Decoration>` underline/strike, `paintRunUnderline/Strike` geometry (dash, crisp snap, 2 bands, `a:uLn@w`) | 7 un-ign | T1 | 2 | med (3 backends) |
| **T8** | stacked WordArt layout | `text/vertical.rs` → `vertical/{mod,eavert,stacked}.rs` (split first), `text/stacked_layout.rs`*, `text/stacked.rs` | `StackedCell/Column/CellKind`, `layout_stacked`, `thickness = 7/6·max cell`, `lnSpc`/`algn`/`just` | 14 → 10 un-ign + 4 new | T1, T2 | 3 | med (`vertical.rs` is 598 lines) |
| **T9** | stacked content: math, hyperlink, frame, prefix cells | `text/stacked.rs`, `text/stacked_layout.rs` | `StackedOptions::object_glyph`, `BreakItem::Object` consumption, `shape_text_rotation(vert,rot,flip_h,flip_v)`, leading prefix cells | 11 → 8 un-ign + 3 new | T7, T8 | 2 | med |
| **T10** | warp residuals: per-line envelope, `lnSpc`, winding, SVG | `compiler/text_warp/lines.rs`*, `compiler/text.rs` (warp half), `svg/text.rs`, `text/shape.rs::ink_box` | `flat_lines`, `warp_envelope_for_line(env,outset)`, `anchor_line(lines,clockwise)`, `ink_box` from `glyph_bounding_box`, per-glyph affine in SVG | 14 un-ign + 1 new (SVG/raster pair) | T1, T3 | 2 | high (`0.75→0.8` baseline moves every warp number) |
| **P1** | paint IR: pattern, gradient options, `miter_limit`, custGeom paint (**C2**) | `contracts/paint.rs`, `contracts/vector_slide.rs` (image region), `compiler/{style,pattern,background}.rs`, `rasterizer/{path,pattern}.rs`*, `svg/{path,pattern}.rs`*, `compiler/mod.rs` (fill call sites) | `VectorPattern`, `PATTERN_CELL_EMU = 1587.5`, `build_pattern_bitmap`, `VectorGradient::{kind,fill_to_rect,tile_rect,flip}`, `linear_gradient_axis_for`, `radial_gradient_frame_for`, `FillFrame`, `compile_shape_fill(fill,alpha,rot,flip_h,flip_v)`, `VectorStroke::{miter_limit,paint,head_end,tail_end,compound}` | 2 un-ign + 2 new | — | 2 | med |
| **P2** | line ends, shaft trim, compound rails | `compiler/stroke/{mod,arrow,trim,compound}.rs`*, `compiler/tables/compound.rs` (extract), `tests/parity_text_box_b_highlight_tests.rs` | `line_end_metrics/retract`, `build_line_end_path`, `trim_geometry_path_ends`, `compound_runs`; emits `DrawLineEnd` in the tip frame | 6 un-ign + 3 unit | P1 (C2) | 2.5 | med (`<=` convention at the trim circle; arcs → cubics) |
| **P3** | shape/table image fill + shared tile painters | `compiler/shapes/image_fill.rs`*, `compiler/{pictures,tables}.rs`, `rasterizer/tile.rs`*, `svg/tile.rs`*, `rasterizer/image.rs`, `svg/image.rs`, `tests/shapes_a_support/image_fill_cases.rs` | `plan_image_fill`, `push_image_fill(clip)`, `paint_tiles` (moved), `MAX_TILE_PIXELS/DIM`, `VectorPaint::Image` dispatch | 13 un-ign (2 stay BLOCKED on P4) + 2 new | S1, P1 | 2.5 | med (grid registers in the *unrotated* box) |
| **P4** | decode planner, byte budget, poster raster | `loader/{probe,budget}.rs`*, `loader/asset_store.rs` (`decode_raster` ceiling), `error.rs`, `compiler/placeholders.rs`, `tests/parity_elements_poster_tests.rs`* | `raster_size(bytes)` header sniff, `ImageBudget::plan(&[DecodeDemand])`, `AssetError::DecodeLimit{metric,limit,observed}`, poster `DrawImage` | 29 new | S1 | 2 | med (probe must cover every `image` format) |
| **P5** | chart paint: bubble3D, ChartEx seam, marker fills | `compiler/charts/{bubble,markers,mod,types}.rs`, `contracts/pipeline.rs`, `compiler/background.rs`, `rasterizer/background_image.rs`, `svg/background_image.rs` | `ChartExRenderer` trait + `CompileOptions::chartex`, `bubble3d_materials`, `compile_bubbles`, `source-atop` → `PushClip`, background `dpi`/`effects` | 17 (6 markers) | P3, S3 | 4 | high for full painters (9.7k TS lines: out of scope) |
| **S1** | typed resource-limit errors | `ooxml-common/src/resource.rs`, `pptx-parser/src/{lib,error}.rs`, `core/src/error.rs`, `loader/embedded_fonts.rs`* | `ResourceViolation`, `HardResourceLimitKind::violation`, `ParseError/AssetError::ResourceLimit(v)`, `load_embedded_fonts` | 4 new | — | 1 | low (5 construction sites) |
| **S2** | dim overlay | `compiler/dim.rs`* | `apply_dim_overlay(&mut VectorSlide, &DimOptions)` — one last `DrawPath` | 1 new | — | 0.5 | none |
| **S3** | parse-error card + `CompileOptions` | `compiler/{options,parse_error_card}.rs`*, `compiler/mod.rs` (entry) | `CompileOptions::error_placeholder`, defaulted `compile_opts`, `PartFailure`, 5-group card | 4 new | S1 | 1.5 | med (every `SlideCompiler` caller; keep the method defaulted) |
| **S4** | SmartArt fallback contrast | `compiler/smartart_contrast.rs`*, `compiler/mod.rs` (theme hook) | `hex_luma`, `background_luminance`, `is_smart_art_fallback_shape`, `smart_art_fallback_text_color` | 12 new | S3 | 1.5 | low |
| **S5** | multi-column bodies + text-run identity (**C4**) | `text/columns.rs`*, `text/emit.rs` (hook), `compiler/tables.rs` (cell frames) | `plan_columns`, `ColumnPlan{line_col,lines_per_col,dx}`, `VectorSlide::text_run_info`, `CellFrame`, `TextRunInfo` | 9 new | T3, T4, T5, S4 | 2 | med |
| **S6** | scene3d camera + homography + projected geometry (**C3**) | `contracts/shape_effects.rs`, `scene3d.rs` → `scene3d/{camera,presets,homography,effect_geometry}.rs`, `compiler/shape_effects.rs`, `rasterizer/shape_effect.rs`, `svg/filters.rs` | `SceneCamera`, `compute_scene3d_quad`, `is_scene3d_non_identity`, `compute_depth_offset`, `unit_square_to_quad`, `expand_projected_quad`, `projected_effect_geometry`, 44-entry `PRESETS` | 4 un-ign + 0 | — | 3.5 | high (affine→homography moves every 3-D pixel test) |
| **S7a** | bevel inputs, profiles, light rigs, shading | `rasterizer/bevel/{inputs,profiles,rigs,lighting,shading}.rs`, `rasterizer/shape_effect.rs` (padding) | `build_bevel_inputs` (no 48 px clamp, `w>0&&h>0` gate), `profile_height/slope` (13 LUTs), `rig_vector` (23 rigs), `shade_surface` (15 materials) | 29 of 40 | S6 (C3) | 4 | high |
| **S7b** | extrusion walls, wireframe, bevel-bottom | `rasterizer/bevel/{extrusion,wireframe}.rs` | `build_extrusion_input`, `apply_extrusion`, `raster_bevel_wireframe`, `legacyWireframe` zero-width inputs | rest of the 40 | S6, S7a | 4 | high |
| **S8** | distance-aware reflection blur | `rasterizer/shape_effect/{reflection_blur,reflection}.rs` | `reflection_blur_bands(bounds,max_blur)`, `paint_distance_aware_reflection_blur`, 24-pass cap | 3 un-ign | — | 1 | low |
| **V1** | hidden-slide policy | `core/src/navigation.rs`*, `tui/src/app/state*` | `HiddenSlideMode`, `next_visible`, `resolve_visible`, `count_visible` wired into nav/search/sorter/`link_at_cell` | 15 new | — | 1 | low |
| **V2** | find over slide text | `core/src/text/find.rs`*, `tui/src/app/find_state.rs`* | search containers (body vs cell), highlight rects in EMU, next/prev/active | 4 new | — | 1.5 | low |
| **V3** | element selection | `core/src/selection.rs`* | reverse-order hit test, inverse rotation/flip, line tolerance, grapheme-bounded text | 10 new | — | 1 | low |
| **V4** | part-failure tolerance | parser adapter, `compiler/mod.rs` (placeholder), `tests/part_failure_tests.rs`* | per-slide failure capture, `FidelityDegraded{Placeholder}`, sorted+deduped delivery | 5 new | S3 | 1 | med |
| **V5** | pipeline hardening: cancel, latch, LRU, raw cache | `core/src/prefetch*`, `loader/asset_store.rs` (raw cache) | cancel check before `put_vector`, `clear_failed()` in `invalidate(DeckReload)`, `catch_unwind` clearing `in_flight`, byte-budgeted LRU | 33 new | — | 1 | low |
| **V6** | container guard | `loader` magic sniff | `ParseError::PasswordProtected` (declared, never produced), legacy-binary vs not-ooxml | 5 new | — | 0.5 | none |
| **V7** | zoom/fit scale | `tui` state + `RenderTargetSpec` | `set_scale` clamp `[0.1,4]`, `fit_page`/`fit_width`, latched pre-layout scale | 19 new | — | 0.5 | low |
| **V8** | hyperlink classification | `compiler/hyperlinks.rs` | allow `tel:`, non-navigable scheme → internal (not `UnsupportedScheme`) | 5 new | — | 0.25 | none |

**Totals: 32 workstreams, 728 render cases + ≈150 platform cases, ≈55 dev-days.**

## 4. Wave sequencing

Every agent runs with `CARGO_TARGET_DIR=/workspace/kitty-pptx/target/agent-<name>` (kache makes a
cold build cheap, so worktrees are affordable). Merge order inside a wave is by contract gate first.

### 4.1 Wave 3 — unblock the tree and freeze the contracts (9 agents)

| agent | owns | why now |
| :--- | :--- | :--- |
| `agent-paint-ir` | **P1** | **The tree is red** (`rasterizer/path.rs`/`svg/path.rs` lack the new `VectorPaint` arms). First commit = green build; second = `VectorStroke`/`DrawLineEnd` (**C2**). Blocks P2/P3/P5. |
| `agent-text-faces` | **T1** | Largest single count (54), no deps, freezes `StyleDef`/`FaceMetricsFull` that T2/T6/T7/T8/T10 read. |
| `agent-struct-core` | **S1+S2+S3** | 7 cases; `AssetError::ResourceLimit` + `CompileOptions` are the seams P3/P4/P5/V4 call. |
| `agent-text-spc` | **T3+T4** | 37 cases, biggest algorithm block, no T1 dependency. |
| `agent-text-slots` | **T2** — `isolation: worktree` | Only needs `faces::is_serif_latin_face`; code against the signature, stub until T1 merges. Keeps T1→T2→T8→T9 inside 3 waves. |
| `agent-stroke` | **P2** — `isolation: worktree` | Needs only **C2**; pure geometry, disjoint files. Rebases after P1. |
| `agent-scene3d` | **S6** | No deps; owns **C3**, which S7a/S7b need in wave 4. Critical-path head. |
| `agent-viewer-nav` | **V1+V8** | 20 platform cases, zero overlap with core. |
| `agent-pipeline` | **V5+V6** | 38 platform cases, `prefetch`/`loader` only. |

### 4.2 Wave 4 — feature blocks on the frozen contracts (9 agents)

| agent | owns | starts from |
| :--- | :--- | :--- |
| `agent-text-decor` | **T7** (**C1** first commit) | T1 merged |
| `agent-text-wrap` | **T5+T6** | T3, T4 merged (`text/linebreak.rs`, `text/emit.rs`) |
| `agent-text-stacked` | **T8** (split `vertical.rs` first) | T1, T2 merged |
| `agent-image-fill` | **P3** | P1, S1 merged |
| `agent-decode` | **P4** | S1 merged (`error.rs`, `loader/`) |
| `agent-bevel-a` | **S7a** | S6 + C3 merged |
| `agent-bevel-b` | **S7b** — `isolation: worktree` | needs S7a's `bevel/inputs.rs`; codes against its signature, rebases |
| `agent-struct-contrast` | **S4** | S3 merged (`compiler/mod.rs` theme hook) |
| `agent-viewer-find` | **V2+V3** | — |

### 4.3 Wave 5 — residuals, close-out, ledger (7 agents)

| agent | owns |
| :--- | :--- |
| `agent-text-stacked-content` | **T9** (C1 consumers) |
| `agent-text-warp` | **T10** |
| `agent-chart-paint` | **P5** |
| `agent-struct-text` | **S5** (C4) |
| `agent-reflect` | **S8** |
| `agent-viewer-misc` | **V4+V7** |
| `agent-ledger` | **L0**: re-derive `COVERAGE.md` from the tree, delete the 6 N/A stubs, land `parity_ledger_tests.rs`, run the corpus and record `external.json` |

### 4.4 Isolation and merge order

Worktrees: **3** (`agent-text-slots`, `agent-stroke` in wave 3; `agent-bevel-b` in wave 4). Everything
else shares the tree, because its dependencies merged in the previous wave. Merge order inside a
wave: **contract commit → dependent commits → test un-ignores**.

### 4.5 Shared-file region ownership (enforced by review)

| file | regions, in merge order |
| :--- | :--- |
| `text/emit.rs` | T1 `StyleDef`/`Measurer` (w3) → T3 line loop (w3) → T5 literal, T6 `do_wrap` (w4) → S5 hook (w5) |
| `text/paint.rs` | T3/T4 (w3) → T7 decorations (w4) |
| `compiler/mod.rs` | S3 `compile` entry (w3) → P1 fill call sites (w3) → S4 theme hook (w4) → V4 placeholder (w5) |
| `contracts/vector_slide.rs` | P1 image/line-end (w3) → S5 `TextRunInfo` (w5) |
| `loader/asset_store.rs` | V5 raw cache (w3) → P4 decode ceiling (w4) |
| `rasterizer/shape_effect.rs` | S6 padding/homography (w3) → S7a clamps (w4); S8 owns only `shape_effect/{reflection,reflection_blur}.rs` |
| `svg/text.rs` | T7 decorations (w4) → T10 warp (w5) |

**Critical path: S6 → S7a → S7b = 3.5 + 4 + 4 = 11.5 dev-days** (3-D shading). Every other chain
finishes ≥2 days earlier (next longest: T1→T2→T8→T9 = 9 d; S1→P3→P5 = 8 d). Wall clock ≈ 11.5
dev-days of the slowest agent; S7b is the only workstream whose overrun moves the end date, so it
gets the second agent (`agent-bevel-b`) by design.

## 5. Zero-ignore policy

### 5.1 Rules

1. `#[ignore]` is a **scheduling marker only** and never survives a wave that owns it.
2. A blocked case has exactly three legal fates: **enable** (the API named in the reason exists and
   the test asserts the reference's numbers), **delete as N/A** (recorded in the ledger with a
   one-line technical reason), or **re-scope** (the case moves to another workstream's row).
   "Later wave" is only legal while a wave is in flight, and must name the wave.
3. Deleting a stub is not losing coverage: the case stays in `COVERAGE.md` as `N/A + reason`, and
   `parity_ledger_tests` asserts `BLOCKED == 0`.
4. No `#[cfg(skip)]`, no empty `fn x() {}` body, no early `return` hiding an assertion. A test that
   cannot assert anything is deleted, not stubbed.
5. Enabling a test is a behaviour gate: the same commit must not touch its expected numbers unless
   the reference says so, and the reason string is deleted with the attribute.

### 5.2 The 67 ignores and their fate (tree order)

| site | test | fate |
| :--- | :--- | :--- |
| `parity_shapes_a_tests.rs:104` | `custgeom_per_path_fill_and_stroke_flags` | **enable** — P1 (`ShapeElement::cust_geom_paint` → per-subpath `fill`/`stroke`) |
| `parity_shapes_a_tests.rs:227` | `rot_with_shape_false_counter_rotates_the_gradient` | **enable** — P1 (`FillFrame` + `fill_counter_rotation`) |
| `parity_text_box_b_bullet_image_tests.rs:270,276,284,290` | `picture_marker_reuses_the_prepared_bitmap_cache`, `svg_picture_marker_uses_the_worker_bridge`, `svg_picture_marker_loads_through_html_image_element`, `shared_svg_marker_dedupes_at_the_largest_height` | **delete N/A (4)** — `createImageBitmap` cache, `svgDecoder` worker bridge, `HTMLImageElement`/`createObjectURL`, shared-bitmap dedupe: all browser-only |
| `parity_text_box_b_eavert_tests.rs:143` | `fullwidth_colon_rotates_with_the_page` | **enable** — T2 (`orientation_class` FF1A row) |
| `parity_text_box_b_eavert_tests.rs:190,375` | `colon_is_not_reflected`, `helper_installs_no_fallback_matrix` | **delete N/A (2)** — no per-glyph scale-y/reflection and no canvas transform stack in the eaVert model |
| `parity_text_box_b_eavert_tests.rs:350` | `helper_uses_the_vert_feature_for_long_marks_only` | **enable** — T2 (`vert_feature_covers`) |
| `parity_text_box_b_eavert_tests.rs:365` | `helper_keeps_uncovered_glyphs_on_the_plain_fallback` | **enable** — T2 (same API, negative branch) |
| `parity_text_box_b_eavert_tests.rs:381` | `helper_advances_by_measure_plus_spacing` | **enable** — T3 (`measure + ls` per code point) |
| `parity_text_box_b_eavert_tests.rs:396` | `latin_faces_classify_by_panose_serif_style` | **enable** — T1 (`is_serif_latin_face`) |
| `parity_text_box_b_eavert_tests.rs:412` | `east_asian_default_face_covers_the_whole_text` | **enable now** — `text::east_asian_default` landed; wire the file-local helper stub |
| `parity_text_box_b_eavert_tests.rs:433` | `east_asian_latin_face_keeps_its_own_stack` | **enable now** — same |
| `parity_text_box_b_eavert_tests.rs:448` | `complex_script_default_face_follows_the_script` | **enable now** — same |
| `parity_text_box_b_eavert_tests.rs:461` | `empty_east_asian_slot_uses_the_application_default` | **enable now** — `slots::push_run_items` landed |
| `parity_text_box_b_eavert_tests.rs:471` | `resolved_east_asian_face_is_kept_and_complex_scripts_split` | **enable now** — same |
| `parity_text_box_b_eavert_tests.rs:489` | `east_asian_face_claims_symbols_and_curly_quotes` | **enable now** — same |
| `parity_text_box_b_eavert_tests.rs:500` | `east_asian_slot_symbol_stack_resolves_per_cluster` | **enable now** — same |
| `parity_text_box_b_highlight_tests.rs:271` | `highlight_uses_the_face_glyph_box` | **enable** — T1 (`FaceMetricsFull::glyph`) |
| `parity_text_box_b_highlight_tests.rs:434` | `curved_connector_shaft_stops_at_the_vertex` | **enable** — P2 (`trim_geometry_path_ends`) |
| `parity_text_box_b_highlight_tests.rs:445` | `custom_geometry_cubic_shaft_stops_at_the_vertex` | **enable** — P2 |
| `parity_text_box_b_highlight_tests.rs:470` | `custom_geometry_arc_shaft_stays_on_its_ellipse` | **enable** — P2 (arc → cubic, ≤1e-3 px) |
| `parity_text_box_b_highlight_tests.rs:503` | `bent_connector_short_leg_stops_at_the_vertex` | **enable** — P2 |
| `parity_text_box_b_highlight_tests.rs:514` | `both_arrow_ends_consume_the_shaft` | **enable** — P2 (both ends ⇒ `[]`) |
| `parity_text_box_b_highlight_tests.rs:534` | `compound_straight_connector_trims_both_lines` | **enable** — P2 (`compound_runs`) |
| `parity_text_box_b_stacked_tests.rs:172` | `stacked_glyphs_advance_one_cell_along_the_column` | **enable** — T8 (`thickness = 7/6·size`) |
| `parity_text_box_b_stacked_tests.rs:182` | `stacked_cell_follows_the_drawing_face` | **enable** — T8 + T1 |
| `parity_text_box_b_stacked_tests.rs:198` | `stacked_ascii_bracket_is_turned_90_degrees` | **enable** — T8 (`CellKind::Rotated`) |
| `parity_text_box_b_stacked_tests.rs:204` | `stacked_parallel_mark_follows_the_face_feature` | **enable** — T8 + T2 (`vert`) |
| `parity_text_box_b_stacked_tests.rs:210` | `stacked_bold_specific_vert_coverage` | **enable** — T8 + T2 (`vert` per face weight) |
| `parity_text_box_b_stacked_tests.rs:227` | `stacked_glyphs_carry_the_run_hyperlink` | **enable** — T9 (**C1** `hyperlink`) |
| `parity_text_box_b_stacked_tests.rs:266` | `stacked_body_rotation_follows_the_frame_flips` | **enable** — T9 (`shape_text_rotation`) |
| `parity_text_box_b_stacked_tests.rs:293` | `horizontal_cluster_stays_in_the_base_segment` | **enable** — T8 (`layout_stacked` segmentation) |
| `parity_text_box_b_stacked_tests.rs:307` | `stacked_cluster_split_across_runs_stays_one_cell` | **enable** — T8 |
| `parity_text_box_b_stacked_tests.rs:312` | `horizontal_cluster_split_across_runs_stays_in_the_base_segment` | **enable** — T8 |
| `parity_text_box_b_stacked_tests.rs:335` | `stacked_cached_equation_keeps_the_horizontal_geometry` | **enable** — T9 (`object_glyph`) |
| `parity_text_box_b_stacked_tests.rs:344` | `stacked_equation_without_an_engine_keeps_the_blank_column` | **enable** — T9 |
| `parity_text_box_b_stacked_tests.rs:349` | `stacked_equation_conversion_failure_draws_the_fallback_text` | **enable** — T9 (`piece.fallback`) |
| `parity_text_box_b_stacked_tests.rs:354` | `stacked_equation_rasterization_failure_draws_the_fallback_text` | **enable** — T9 |
| `parity_text_box_b_stacked_tests.rs:360` | `stacked_complete_inline_equation_branch_is_replaced` | **enable** — T9 |
| `parity_text_box_b_stacked_tests.rs:368` | `stacked_paints_glyph_sized_strikes` | **enable** — T7 (**C1** `Decoration`) |
| `parity_text_box_b_stacked_tests.rs:375` | `stacked_rtl_paints_glyph_sized_strikes` | **enable** — T7 |
| `parity_text_box_b_stacked_tests.rs:381` | `stacked_strikes_follow_the_glyph_frame` | **enable** — T7 |
| `parity_text_box_b_stacked_tests.rs:387` | `stacked_unstruck_paragraph_mark_style_is_honoured` | **enable** — T7 (`end_run_properties` gate) |
| `parity_text_box_b_stacked_tests.rs:393` | `stacked_underline_uses_one_frame_per_column` | **enable** — T7 |
| `parity_text_box_b_stacked_tests.rs:400` | `stacked_rtl_underline_uses_one_frame_per_column` | **enable** — T7 |
| `parity_text_box_b_stacked_tests.rs:406` | `stacked_underline_uses_the_face_descent` | **enable** — T7 + T1 |
| `parity_text_box_b_stacked_tests.rs:416` | `stacked_reserves_char_prefix_cells` | **enable** — T8 |
| `parity_text_box_b_stacked_tests.rs:425` | `stacked_reserves_autonum_prefix_cells` | **enable** — T8 |
| `parity_text_box_b_stacked_tests.rs:431` | `stacked_picture_markers_keep_their_aspect` | **enable** — T9 (`Bullet::Blip` cells) |
| `parity_text_box_b_warp_tests.rs:115` | `multi_line_arch_anchors_the_last_line` | **enable** — T10 (`anchor_line`) |
| `parity_text_box_b_warp_tests.rs:126` | `counterclockwise_lines_expand_outside_the_lower_arch` | **enable** — T10 (single-edge outset) |
| `parity_text_box_b_warp_tests.rs:136` | `warp_honours_fixed_line_spacing` | **enable** — T10 (`spaced_line_box` in `flat_lines`) |
| `parity_text_box_b_warp_tests.rs:146` | `warp_keeps_paragraph_spacing_between_paragraphs` | **enable** — T10 (`spcBef`/`spcAft`) |
| `parity_text_box_b_warp_tests.rs:156` | `warp_uses_50_percent_line_spacing` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:164` | `warp_uses_100_percent_line_spacing` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:172` | `warp_uses_150_percent_line_spacing` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:180` | `warp_uses_200_percent_line_spacing` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:189` | `warp_accumulates_mixed_size_line_boxes` | **enable** — T10 (`ink_h = maxA+maxD`) |
| `parity_text_box_b_warp_tests.rs:198` | `warp_follows_the_path_winding_at_135_degrees` | **enable** — T10 (clockwise from the first two points) |
| `parity_text_box_b_warp_tests.rs:206` | `warp_follows_the_path_winding_at_225_degrees` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:215` | `circle_anchors_the_final_line_and_expands_radially` | **enable** — T10 (radial outset) |
| `parity_text_box_b_warp_tests.rs:226` | `expanded_ellipse_recomputes_each_line` | **enable** — T10 |
| `parity_text_box_b_warp_tests.rs:265` | `warp_layout_measures_the_full_segment_once` | **enable** — T10 (measurer call recording) |
| `parity_text_tests.rs:557` | `bulleted_first_line_ignores_the_positive_indent` | **enable** — T5/T6 (bullet gate on `first_line_indent`; `text.md` calls it "already expressible") |

Fate totals: **61 enable** (7 of them immediately, in the next commit that touches that file) and
**6 delete as N/A**. Nothing is left "for a later wave" after wave 5: 61 + 6 = 67.

### 5.3 Enforcement

New target `crates/kitty-pptx-core/tests/parity_ledger_tests.rs`:

- `no_ignored_parity_tests` — `include_str!`s every `parity_*.rs` (explicit macro list, no new dep)
  and asserts none contains `#[ignore`.
- `ledger_rows_are_self_consistent` — parses the `COVERAGE.md` summary table and asserts, per row,
  `EXACT + BEHAVIOURAL + N/A + BLOCKED == cases`, `BLOCKED == 0`, and that the row's `pass` equals
  the number of `#[test]` attributes in that batch's targets.
- `every_na_row_has_a_reason` — every N/A cell is a non-empty reason of ≥8 words.

---

## 6. Corpus policy

### 6.1 What the corpus is

`fixtures.json` pins **419 decks / 545 slides** from `LibreOffice/core:sd/qa/unit/data/pptx` @
`2ddb63be403d`, rendered at **1600 px** by the reference viewer (`@silurus/ooxml` + Playwright);
references cache under `target/external/goldens/<commit>/`, never committed. 1 deck
(`tdf148478_transitionMusic`) is unrenderable (non-ASCII ZIP member, forbidden by OPC) and is
counted, never silently skipped.

### 6.2 Tiers and thresholds

| tier | count | fails the build? | purpose |
| :--- | --: | :--- | :--- |
| **strict** | 4 (`Bar_List`, `ShapePlusImage`, `ShapeTextInflateTop`, `chart_pt_color_bg1`) | yes | gate: stable geometry/text/fill/chart decks |
| **diagnostic** | 415 | no, but must not regress (§6.4) | feature/fuzz/crash samples, host-font dependent |

Per-fixture budgets, overridable by `thresholds{}` in `fixtures.json`; comparison is 400 px wide,
10×10 px cells, per-channel cell tolerance 40/255:

| key | default | meaning |
| :--- | --: | :--- |
| `min_cell_pass_ratio` | `0.90` | fraction of cells within 40/255 |
| `max_global_mean_abs_diff` | `8.0` | mean per-cell per-channel difference (0–255) |
| `max_ink_fraction_delta` | `0.03` | non-white pixel fraction delta (3 pp) |

### 6.3 Promotion (strict/diagnostic), and how the corpus feeds this plan

A deck moves diagnostic → strict **only** in the PR that closes the gap that made it fail:

1. `run.py --skip-test` for that deck, then read its metric line under `--nocapture`.
2. Two consecutive `--force-render` runs must agree (font/Chromium stability).
3. Set `"strict": true` + explicit `thresholds` with ≥20 % headroom on each key.
4. The workstream's parity tests must be green **before** promotion; a strict deck never substitutes
   for a unit-level assertion.
5. After the first green, thresholds may only be tightened. Loosening needs a `note` explaining why.

**Gap → fixture acceptance map** (the deck that failed for a known gap becomes that gap's acceptance
test; the last column is what the workstream must make true):

| gap | fixture(s) | acceptance assertion |
| :--- | :--- | :--- |
| T1 face metrics | `font-scale`, `TextFittingStored` | strict-eligible: baselines within `max_global_mean_abs_diff 8.0` at 1600 px |
| T6 spAutoFit | `testShapeAutofit`, `narrow-placeholder-no-wrap` | strict-eligible: no wrap where the reference grows horizontally |
| T3/T5 tab + `spc` | `tdf102261_testParaTabStopDefaultDistance` | cell ratios ≥0.90 on the tabbed line |
| T8/T9 stacked | `tdf169825_vertical_layouts`, `vertical-bracket-list`, `tdf125573_FontWorkScaleX` | per-column pitch matches; no dropped columns |
| T10 warp | `ShapeTextInflateTop` (**already strict**), `tdf140865Wordart3D` | stay strict: `min_cell_pass_ratio ≥ 0.90` after the `0.75→0.8` baseline change |
| P2 line ends | `connectors`, `tdf153036_resizedConnectorL`, `glue_point_leaving_directions` | strict-eligible: shaft trim + tip geometry |
| P3 image fill | `ShapePlusImage` (**already strict**), `hyperlinkOnImage`, `predefined-table-style` | stay strict after `Fill::Image` lands |
| P5 charts | `chart_pt_color_bg1` (**already strict**) | stay strict |
| S4 SmartArt | `smartart-*` (10 decks) | fallback text is white on dark, unchanged on light |
| S5 multi-column | `3columns`, `multicol`, `tdf144092-tableHeight` | `3columns` strict-eligible after `plan_columns` |
| S6/S7/S8 3-D + reflection | `tdf140865Wordart3D`, `shape-reflection-effect` | strict-eligible after homography |
| V1/V8 hidden + links | `hidden_group_shape`, `hyperlinkOnImage` | hidden slides not painted |

### 6.4 Diagnostic ratchet

Diagnostics may differ but must not drift. `agent-ledger` records every deck's metric line into
`tests/parity/scores/external.json` (shape of `scores/sample-1.json`); `external_fixture_tests` fails
when a recorded diagnostic regresses by `min_cell_pass_ratio −0.05` or `max_global_mean_abs_diff
+2.0`, naming the deck and its owning workstream (§6.3). An unmapped deck blocks the wave.

## 7. Definition of done

### 7.1 Checklist

- [ ] **0** `#[ignore]` in `crates/**/tests/**` and **0** empty placeholder test bodies.
- [ ] `COVERAGE.md`: `BLOCKED == 0`; every one of the 728 render cases is `EXACT`/`BEHAVIOURAL`/`N/A`
      with a reason; `pass` equals the `#[test]` count per batch (`parity_ledger_tests`).
- [ ] All ≈150 portable platform cases ported; the 21 N/A files recorded with reasons.
- [ ] Every workstream's named APIs exist with the signature in §3, and every "cases" count in §3
      resolves to a named test.
- [ ] Fidelity: `match_pct` for each of the 9 slides never falls >0.5 pt below
      `tests/parity/scores/sample-1.json`, and `diff_pct ≤ 20` (`FAIL_ABOVE_DIFF_PCT`);
      `UPDATE_SCORES=1` may only raise a recorded score.
- [ ] Corpus: 418 compared + 1 known unrenderable + 0 errors; 4/4 strict pass; no diagnostic
      regression vs `external.json`; every strict deck from §6.3 promoted in the closing PR.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean; no source file ≥ 600 lines;
      no `unwrap()`/`expect()` in production paths; `#![forbid(unsafe_code)]` intact.

### 7.2 Verification commands (run in this order, from the repo root)

```sh
# 1. workspace build + tests
cargo check --workspace
cargo test --workspace

# 2. lint gate (zero warnings, all targets)
cargo clippy --workspace --all-targets -- -D warnings

# 3. file-size gate (must print nothing)
find crates -name '*.rs' -not -path '*/target/*' | xargs wc -l | awk '$1 >= 600 && $2 != "total"'

# 4. parity ledger consistency + zero-ignore
cargo test -p kitty-pptx-core --test parity_ledger_tests

# 5. per-target parity sweep (0 ignored expected)
for t in $(ls crates/kitty-pptx-core/tests/parity_*.rs | xargs -n1 basename | sed 's/\.rs$//');
  do cargo test -p kitty-pptx-core --test "$t" || break; done

# 6. fidelity ratchet (pixelmatch 0.20, fail >20 %, ratchet 0.5 pt)
cargo test -p kitty-pptx-core --test parity_fidelity_tests

# 7. external corpus: 418 compared, 4/4 strict, diagnostics not regressed
cargo test -p kitty-pptx-core --test external_fixture_tests -- --nocapture
#    (opt-in cache fill: python3 crates/kitty-pptx-core/tests/golden/external/run.py --offline)
```

Exit criteria: steps 1–7 green on the reference machine (cache populated). On a clean clone without
`target/external`, step 7 prints `skipping external corpus comparison: ...` and returns — that
self-skip is the only legal skip in the tree, and it must never be reachable with the cache present.
