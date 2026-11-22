# PRD 12 — Render parity with `office-open-xml-viewer`

> Status: analysis + rewrite plan. No production code changed.
> Reference: [`yukiyokotani/office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer)
> `main@1f5c6e1df8776c05169b70c8e548bbc5918178fc` (2026-11-12), packages `pptx` (0.89.4) and `core`.
> Target: this workspace, `crates/office-terminal-core` (compiler + rasterizer + SVG).

This document answers three questions:

1. **Why** does the Rust render path not follow the same logic as the reference TypeScript renderer?
2. **How** should the Rust render path be rewritten so it does?
3. **Which tests** in the reference repo must be ported, and how do they become Rust tests?

Every claim below was verified by reading both trees; the measured numbers come from rendering the
reference's own demo deck through this workspace's production pipeline.

---

## 1. Measured baseline (before any change)

Deck: `packages/pptx/public/demo/sample-1.pptx` (9 slides, 12192000×6858000 EMU).
Render: `kitty-pptx export --format png --dpi 96` → 1280×720 per slide.
Comparison: the reference VRT metric — pixelmatch YIQ, `threshold = 0.20`, `includeAA = true`
(`packages/pptx/tests/visual/visual.spec.ts`), against the committed reference PNGs.

| slide | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|
| ours vs reference PNG (%) | 95.82 | 98.00 | 95.44 | 97.90 | **79.38** | 97.97 | 95.18 | 97.16 | 97.97 |
| mean abs diff (0–255) | 7.44 | 3.94 | 7.00 | 3.71 | **34.69** | 3.00 | 7.56 | 4.31 | 3.92 |
| reference's own committed score | 95.65 | 97.99 | 96.07 | 98.02 | 96.14 | 98.62 | 95.54 | 97.16 | 97.73 |

Two facts matter more than the numbers:

- The reference PNGs are **adopted renderer outputs, not Office exports**
  (`/tmp/oooxml/AGENTS.md:423`). The 20 % ceiling therefore measures *drift*, not fidelity to PowerPoint.
  A 95 % "match" can still hide a missing wrap engine — see slide 1: the reference wraps
  `STATE OF THE FOREST` onto three lines, we paint one line that runs over the photo.
- One unit fix is worth 16.6 points. `compiler/charts/layout.rs:206` reads
  `titleFontSizeHpt` as half-points (`/ 2.0`); the reference reads it as hundredths of a point
  (`packages/core/src/chart/layout.ts:153`: `(sizeHpt / 100) * ptToPx`). With `/ 100.0` the slide-5
  title drops from **800 pt to 16 pt**, the giant clipped glyph disappears, and the slide goes
  **79.38 % → 95.97 %** (`mean abs diff 34.69 → 6.23`). The remaining 4 % is still visibly wrong:
  title overlapping the caption, vertical text painted horizontally, body text running off-slide.
  That is the shape of this whole problem: **contract errors first, missing logic second**.

---

## 2. Root causes, ranked

### R1 — There is no text layout engine (dominant, affects every slide)

`compile_text_body` (`crates/office-terminal-core/src/compiler/text.rs:68`) shapes one run at a time with
`cosmic-text`, `set_size(None, None)` (no width limit, `text.rs:36`) and only splits lines on an explicit
`<a:br/>` (`text.rs:237`). The reference calls `layoutParagraph(para, maxW, …)`
(`packages/pptx/src/renderer.ts:5324`) which runs a full UAX#14 + kinsoku + tab + justification engine
(`packages/core/src/drawingml-text/break.ts:104`, `text-justify.ts`, `drawingml-text/tab.ts`).

Fields that exist in the vendored model but are **never read** by the compiler
(`grep` over `compiler/` returns zero hits):

`wrap`, `vert`, `auto_fit`, `ln_spc_reduction`, `num_col`, `spc_col`, `rtl_col`, `anchor_ctr`,
`compat_ln_spc`, `space_line`, `space_after`, `space_before_pct`, `space_after_pct`, `indent`,
`mar_r`, `tab_stops`, `def_tab_sz`, `ea_ln_brk`, `font_algn`, `rtl`, plus run-level
`letter_spacing`, `baseline`, `caps`, `highlight`, `shadow`, `outline`, `no_fill`, `pattern_fill`,
`underline_style/color/fill`, `strike_double`, `font_family_ea/cs/sym`.

Consequences visible on the demo deck:

- No wrapping (slide 1: 3 lines → 1 overflowing line).
- Vertical text (`vert="vert"` on slide 5's spine label) painted horizontally over the title.
- Line box is `size × 1.2` (`text.rs:139`), baseline fixed at `0.75 × line_h` (`text.rs:373`); the
  rasterizer then re-derives its own baseline with `origin_y − 0.85 × size`
  (`rasterizer/text.rs:81`). Two different guesses, neither from the face's ascent.
- Bullets are inlined as `"• "` in hard-coded `Carlito` at the paragraph origin (`text.rs:157-180`),
  so hanging indents, `buClr`, `buSzPct/Pts`, picture bullets and per-level numbering are lost and
  the marker is counted in the centred width.
- At paint time, `render_text_runs` **discards every compiled glyph position** and re-shapes
  (`rasterizer/text.rs:60-77`): only `glyphs[0]` is used as an anchor. Compile-time kerning, tracking
  and justification cannot survive; PNG and SVG disagree by construction.

### R2 — The render-target contract is different from the reference

| | reference | this workspace |
|---|---|---|
| canvas mapping | `scale = targetWidth / slideWidthEMU`; `canvasH = round(slideHeight·scale)`; origin (0,0) (`renderer.ts:8367-8370`) | `s = min(W/Wemu, H/Hemu)` + centred `dx,dy` (`rasterizer/mod.rs:56-63`) |
| dpr | backing store = CSS × dpr, one `ctx.scale(dpr,dpr)` (`renderer.ts:8373,8459`) | none (pixmap is device px) |
| fit to pane | outside the canvas (host), slide canvas always exact aspect | third independent fit in the TUI cell rect (`tui/src/widget/layout.rs:24-70`) with a hardcoded 0.5 cell aspect |

Because the slide is letterboxed *inside* the bitmap, every output is resampled with a sub-pixel
offset (`dx`,`dy`), and the terminal's cell grid can disagree with the bitmap aspect by ~2 %. The
reference never letterboxes and never resamples the slide: it renders exactly 1280×720 for a 16:9
slide at width 1280.

### R3 — Paint and geometry are compiled lossily, not ported

- **Presets are hand-written**, not data-driven. The reference ships
  `packages/core/src/shape/preset-geometry/presets.json` — 186 ECMA-376 definitions evaluated by a
  postfix formula VM (`evaluator.ts:124`, `path-executor.ts`). Our ten `presets_*.rs` modules use
  `adj/100000` + per-preset clamps, no `ss = min(w,h)` guide, no `pin/maxAdj`, no per-path paint
  (`compiler/shapes/util.rs:16`). Connectors (`bentConnector3`, `straightConnector1`, …) fall through
  to a filled rectangle (`shapes/mod.rs:82`). Arrow `adj1/adj2` are swapped
  (`presets_t1.rs:170-188` vs `presets.json`), hexagon/chevron slant is `w`-based instead of
  `ss`-based (2× error at 2:1), `custGeom` `ArcTo` treats the pen as the arc centre
  (`shapes/mod.rs:152`).
- **Paint is reduced at compile time.** `Fill::Image → None` (`compiler/style.rs:70`), `Fill::Pattern →
  solid fg` (`style.rs:64`), gradient keeps only `{angle, stops}` (`style.rs:42-60`), stroke paint is
  dropped (`style.rs:78`), arrowheads are dropped (`style.rs:78-124`). The rasterizer then guesses the
  gradient frame as the device-space AABB with a half-diagonal length
  (`rasterizer/path.rs:111`), while the reference uses the shape box and `(|cos|w + |sin|h)/2`
  (`packages/core/src/shape/paint.ts:296`).
- **Stroke constants disagree**: cap default `Square` vs `butt` (`style.rs:111` vs
  `stroke-geometry.ts:19`), hairline floor `1.0 px` vs `0.5 px` (`path.rs:197`), miter limit `4.0` vs
  authored/`10` (`path.rs:218`), dash table `[4,3]/[1,3]/[8,3]` vs
  `[6,3]/[1.5,3]/[10,4]` (`style.rs:93` vs `draw/dash.ts:146`).
- **Images**: `srcRect` is divided by 100000 twice — the parser already stores fractions
  (`ooxml-common/src/blip.rs:132` → `drawingml_percentage_to_fraction`), the compiler divides again
  (`compiler/pictures.rs:27-30`), so crops are ~0 and every image stretches full-frame. SVG ignores
  `src_rect` entirely (`svg/image.rs:9`). Picture silhouettes are not clipped: `PushClip`/`PopClip` are
  no-ops in both backends (`rasterizer/mod.rs:182`, `svg/mod.rs:257`) although `pictures.rs:88` emits
  them.
- **Effects**: shadow blur σ is `radius/2` (`rasterizer/shape_effect.rs:279`) where the reference uses
  `σ = radius` (`core/src/shape/effects.ts:323`); glow is `radius/2.5` + a convex remap
  (`shape_effect.rs:316`) vs CSS `shadowBlur` semantics; `rotWithShape` defaults to `false`
  (`compiler/shape_effects.rs:43`) vs the spec/reference default `true`; reflection ignores
  `stPos/endPos/dir/sx/sy`; effect order is reflection→shadow instead of shadow→reflection.
- **Tables**: authored row heights only, no measure-and-grow passes (`compiler/tables.rs:35-41` vs
  `renderer.ts:7645-7688`), every cell strokes four edges instead of one owner per gridline
  (`tables.rs:121-187`), fills/strokes/text are interleaved instead of fills+text then all borders
  (`renderer.ts:7843,7903`), `rtl` is ignored.
- **Charts**: unknown types silently become column charts (`charts/types.rs:47-80`), the frame uses
  fixed 12 %/22 % bands instead of measured bands (`charts/layout.rs:74-126`), axis label size is
  hardcoded 8.5 pt, and the font-size unit bug above.

### R4 — The fidelity policy was written down as "degraded/placeholder"

`docs/02d-element-renderers-and-fidelity-policy.md` declares chart rendering **PLACEHOLDER**, effects
**OMITTED**, pattern fills **DEGRADED**, etc. The reference's contract is "render everything; label
only what is genuinely unsupported". Code has since grown past the PRD (charts and effects exist) but
the policy still licenses approximations, which is why ad-hoc heuristics survived review. Parity
requires changing the policy, not just the code.

### R5 — Tests never encoded the reference's behavioural contract

- Reference: **150 `*.test.ts`, 1008–1074 `it()`** (≈454 renderer-path, ≈419 viewer/scroll/worker,
  ≈135 model/API) plus 14 Playwright specs and **395 `#[test]`** in the upstream Rust parser/model.
- Ours: **126 core tests** (58 unit + 68 integration), 225 workspace-wide.
- Our golden harness pins 800×450 renders of one PptxGenJS deck against **our own earlier PNGs**
  (`tests/golden_tests.rs:60`), with mean-abs-diff metrics, not pixelmatch, no per-slide scores and no
  synthetic-deck builder. The reference tests use a recording canvas and stubbed `measureText`
  (`packages/pptx/src/test-support/`, `*.test.ts` `recordingCtx()`), which is exactly what a Rust
  `RecordingDevice` would reproduce.

---

## 3. What "the same logic" actually is

The reference is a single-pass **immediate-mode Canvas2D renderer**; the Rust path is a
compile-then-rasterize pipeline with a lossy seam in the middle.

Reference `renderSlide` (`renderer.ts:8288` → `9093`), in order:

1. Resolve error placeholder; claim a per-canvas supersede token.
2. `targetWidth` ← caller; `scale = targetWidth / slideWidthEMU`; `canvasW = round(targetWidth)`;
   `canvasH = round(slideHeight · scale)`.
3. `dpr`; clamp backing store uniformly (`core/src/canvas/clamp.ts`); fold the clamp into `dpr`.
4. Await all async inputs: math/MCE projection, image planning, media posters, fonts.
5. `beginPaint()`: set canvas size (this clears), set CSS box, `ctx.scale(effectiveDpr)`.
6. `parseError` → draw error card and return (no background, no elements).
7. Resolve theme text colour and `RenderContext`.
8. Paint background: **opaque white base first**, then image (srcRect/stretch/tile/alpha) or
   `resolveShapeFill(...)` clipped to the slide rect.
9. For each element in document order, one `switch`:
   `shape → renderShape`, `picture|media → paintResolvedPicture`, `table → renderTable`,
   `chart → renderChart`, each inside `save/restore` + pattern coordinate space.
10. `dim` overlay last; return canvas.

Inside `renderShape`: CTM `translate(c)·rotate·flip·translate(−c)`; resolve fill on the unrotated box;
effects layer (`outerShdw → reflection → glow → body → innerShdw`, softEdge replaces the body);
fill then stroke; **text after the effect pass** (`renderer.ts:4645`), so shape effects never shade
text.

Inside `renderTextBody`: insets → wrap decision → columns → per paragraph `layoutParagraph` (wrap,
tabs, kinsoku) → per line natural box `1.2·maxSize` split by face ascent share → `lnSpc`/`spcBef`/
`spcAft` → autofit relayout at stored `fontScale` → anchor → alignment/justification → pen/baseline
from the face metrics → per-segment fill, highlight, outline, underline/strike, shadow.

**The seam we need is the Canvas API, not a glyph list.** Everything the reference asserts in tests is
a canvas operation (or a pure function feeding one). A Rust renderer that emits the same operations
can be tested the same way; a Rust renderer that pre-computes glyph positions cannot.

---

## 4. Rewrite design

### 4.1 Architecture

Keep the vendored parser/model unchanged (verified byte-identical to the reference). Replace stage 1 +
stage 2 with:

```
pptx_model::Slide
   │  resolve (cascade, theme, placeholders, fonts, paint bags)      ← cached per slide
   ▼
ResolvedSlide { background, elements: Vec<ResolvedElement>, size_emu }
   │  render(device, scale_px_per_emu)                              ← mirrors renderSlideLeased
   ▼
PaintDevice  ── RasterDevice (tiny-skia)
             ── SvgDevice   (standalone SVG)
             ── RecordingDevice (tests: exact canvas-op assertions)
```

```rust
pub trait PaintDevice {
    fn save(&mut self);
    fn restore(&mut self);
    fn transform(&mut self, m: Affine);
    fn clip_path(&mut self, path: &VectorPath, rule: FillRule);
    fn fill_path(&mut self, path: &VectorPath, paint: &Paint, rule: FillRule);
    fn stroke_path(&mut self, path: &VectorPath, paint: &Paint, geom: &StrokeGeom);
    fn draw_image(&mut self, img: &ImageId, src: SrcFrac, dst: RectEmu, alpha: f32,
                  effects: &[BlipEffect]);
    fn draw_text_line(&mut self, line: &TextLine);      // runs with advances + baseline
    fn layer(&mut self, bounds: RectEmu, f: &mut dyn FnMut(&mut dyn PaintDevice));
}
```

`render(device)` owns orchestration (background, order, clips, effects, dim, placeholder) exactly once;
raster and SVG become *sinks*, so they cannot drift. Text layout runs at the paint scale, once, and
`draw_text_line` carries the resulting advances — the raster device rasterises glyphs, never re-shapes.

Why this and not "fix the display list": the existing `VectorCommand` set cannot express per-path fill
modes, pattern/image fills, tile geometry, clip stacks, text lines with metrics, or effect layers.
Extending it to cover all of that *is* the Canvas API, but without the guarantee that both backends
interpret it identically. A device trait gives that guarantee and makes the ported tests possible.

### 4.2 Module layout (all files < 600 lines)

```
core/src/render/mod.rs              render() orchestration, RenderContext          <300
core/src/render/scene.rs            ResolvedSlide/Element/Background               <350
core/src/render/paint/mod.rs        PaintResolver (solid/grad/pattern/image)       <400
core/src/render/paint/gradient.rs   linear/radial/path-shade frames                <250
core/src/render/stroke.rs           dash table, cap/join/miter, arrowheads, cmpd   <350
core/src/render/effects/mod.rs      shadow/glow/softEdge/reflection/innerShdw      <400
core/src/render/effects/layer.rs    bounds ⊕ stroke/contour, layer compositing     <250
core/src/render/shapes/mod.rs       preset dispatch → evaluator                     <200
core/src/render/shapes/evaluator.rs postfix VM (17 ops, builtins, adj aliasing)     <350
core/src/render/shapes/executor.rs  paths, visual→parametric arcs, per-path paint   <400
core/src/render/shapes/presets.json 186 ECMA-376 definitions (include_str!)         data
core/src/render/text/mod.rs         body orchestration                              <300
core/src/render/text/cascade.rs     body/para/run inheritance → ResolvedRun         <350
core/src/render/text/metrics.rs     natural/spaced line box, face ascent share      <350
core/src/render/text/break.rs       UAX#14 + kinsoku + tabs + SEA + graphemes        <550
core/src/render/text/justify.rs     gap kernel, per-gap distribution, pieces        <300
core/src/render/text/bullets.rs     buChar/buAutoNum/buBlip, counters, gutter        <350
core/src/render/text/vertical.rs    vert/vert270/eaVert/stacked + UAX#50            <350
core/src/render/text/emit.rs        TextLine/TextRun IR                             <300
core/src/render/elements/picture.rs crop/tile/silhouette/effects                   <450
core/src/render/elements/table.rs   grow passes, ownership, two paint passes        <500
core/src/render/elements/chart/*.rs supported subset + Unsupported placeholder     <500 each
core/src/render/raster/*.rs         tiny-skia PaintDevice                          <400 each
core/src/render/svg/*.rs            SVG PaintDevice (+ defs/clipPath/pattern)      <400 each
core/src/render/record.rs           RecordingDevice (test-only feature flag)       <300
display/src/fit.rs                  cell rect → exact device px (CSI 16 t probe)   <250
```

Delete after cut-over: `compiler/shapes/presets_*.rs` (≈3.3 k lines), `scene3d.rs` approximations,
`svg/` duplicate paint logic, `rasterizer/text.rs` re-shaping.

### 4.3 Non-negotiable contracts

1. `scale = width_px / width_emu`; `height_px = round(height_emu · scale)`; origin (0,0); **no
   letterbox anywhere below the view layer**.
2. One `dpr`/device-scale multiply, applied once; no per-element rounding except documented crisp
   snapping for 1-device-pixel lines (table borders).
3. Opaque white base under every background; `parseError` short-circuits before background.
4. Document order only; no sorting; every element inside save/restore; real clip stack.
5. Text: layout at paint scale from real face metrics; per-line baseline; no re-shaping at paint.
6. Paint resolved on the shape box (pre-rotation); gradient length `(|cos|w+|sin|h)/2`; `rotWithShape`.
7. Stroke: `max(0.5, w·scale)`, cap `butt`, miter `10`, spec dash table, arrowheads + retraction.
8. Effects on the composed shape (fill+stroke), text painted after; order
   `outerShdw → reflection → glow → body → innerShdw`, softEdge replaces the body; σ = px reach.
9. SVG and raster read the same resolved paint/text; SVG must not have its own gradient/stroke math.
10. Unsupported features emit the reference's labelled placeholder, never a substituted family
    (charts) or a silently blank shape.

### 4.4 Phases and exit criteria

| phase | work | exit criterion |
|---|---|---|
| **P0** seams | `PaintDevice`, `RecordingDevice`, `TextMeasure` injection, synthetic-deck builder, pixelmatch port | ported kernel tests run against the new traits |
| **P1** text engine | `text/*` modules; body/paragraph/line port | slide 1 wraps to 3 lines; `powerpoint-line-metrics`, `cjk-wrap`, `text-justify`, `tab-*` specs pass |
| **P2** shapes + paint | presets.json evaluator/executor, per-path paint, gradient frame, stroke geometry, clip stack | preset parity harness (186 presets × 3 boxes × adj extremes) within 0.01 px; `custgeom-path-paint`, gradient/stroke specs pass |
| **P3** elements | pictures (crop/tile/silhouette/effects), tables (grow/ownership/two-pass) | `renderer-shape-image-fill`, `table-border-single-draw`, `srcRect` tests pass |
| **P4** effects | 2-D effects pipeline; bevel/extrusion subset or explicit placeholder | `reflection-blur`, `shape-inner-shadow`, `bevel-band-conversion` pass |
| **P5** charts | measured frame, axis/legend/labels, supported-family table, `Unsupported chart` | chart specs pass; slide 5 title 16 pt, no overlap |
| **P6** target/fit | single-axis raster target, dpr, cell probe, dim/error placeholder | terminal goldens regenerate at exact cell aspect; no letterbox in any golden |

Rough effort: 15–18 engineer-weeks serial; P1/P2/P3 parallelise cleanly across the module boundaries.

---

## 5. Test port plan ("the exact same tests, in Rust")

### 5.1 Inventory and realistic scope

| category | reference files | `it()` | port | notes |
|---|---:|---:|---|---|
| text layout (justify, tabs, kinsoku, spacing, indent, CJK/SEA/bidi, autofit) | 47 | ~350 | **P0** | pure kernels + stub metrics |
| fonts (stack, routes, requests, embedded, substitution) | 13 | ~85 | **P0** | pure tables |
| shapes/paint (bevel, custgeom, gradient, reflection, effects) | 14 | ~67 | **P0** | pure math + recording device |
| tables (border conflict/single draw, cell fill, selection) | 4 | ~38 | **P0** | recording device |
| pictures (image fill, raster target, budget, poster bomb) | 6 | ~52 | P1 | budget math pure |
| charts (nested emit, picture marker, font substitution) | 4 | ~13 | P1 | model-shape oracle |
| viewer/scroll/worker/find/DOM | 39 + 23 | ~469 | **skip** | browser lifecycle; no Rust analogue (our TUI tests cover the state) |
| model/API | rest | ~135 | P2 | adopt where meaningful |
| upstream Rust parser/model tests | — | **395** | **adopt verbatim** | already the same source in `crates/pptx-parser` |

Target: ~450 behavioural Rust tests + 395 adopted parser tests.

### 5.2 Harness (mirrors the reference's `test-support/`)

```
tests/parity_support/deck.rs        synthetic deck builder (port of test-support/synthetic-deck.ts, 75 lines)
tests/parity_support/record.rs      RecordingDevice: ops with attributes (fill/stroke/font/text/image/clip)
tests/parity_support/metrics.rs     TextMeasure stubs: per-char width, ascent share table, face table
tests/parity_support/present.rs     present-faces context (all authored faces installed)
tests/parity_support/pixelmatch.rs  vendored MIT pixelmatch (YIQ, max_delta = 35215·t², includeAA)
tests/parity_support/scores.rs      scores.json read/write, 2 dp, sorted, ratchet
tests/parity_support/render.rs      load → render_slide(width=1280) → RGBA (deck cache)
tests/parity_fidelity_tests.rs      one test per deck, per-slide scores
tests/parity_self_vrt_tests.rs      byte-exact baselines (reference `pngPixelsEqual`)
```

Rationale: the reference unit tests never assert on pixels for layout — they assert on
`ctx` calls with a **stubbed measurer** (`measureText: s => s.length * 10`, ascent 8/descent 2, etc.).
`RecordingDevice` + `TextMeasure` reproduces that exactly, including the numeric expectations.
Tests whose truth is visual (`renderer.gradient`, `bevel-ring`, `warp-strip-quality`) assert sampled
pixels from the 1280-px raster, not IR ordering — otherwise they pass while the render is wrong.

### 5.3 First 20 ports (exact reference cases)

| reference spec | assertion | Rust test |
|---|---|---|
| `text-justify` (`dist`, CJK) | gaps `[1,2]`, perGap 30 | `parity_text::justify_dist_cjk` |
| `text-justify` (`thaiDist`) | gaps `[1,3,4]`, perGap 10 | `parity_text::justify_thai_clusters` |
| `justify-bracket-overlap` | one fillText with `letterSpacing = ls + perGap` | `parity_text::justify_bracket_overlap` |
| `tab-layout` | `[50,T,40,T,40]` → `[50,110,40,160,40]` | `parity_text::tab_widths_mixed` |
| `tab-layout` default grid | no stops, grid 100 → gap 100 | `parity_text::tab_default_grid` |
| `tab-layout` overflow | stop r@700 in 600 → `[560,40]` | `parity_text::tab_clamps_overflow` |
| `cjk-wrap` | `あい、う` @20px → head 1 | `parity_text::kinsoku_retract_leading_comma` |
| `ea-line-break` | `eaLnBrk=false` lets `、` end a line | `parity_text::ea_ln_brk_false` |
| `boundary-glue-lb28-wrap` | `wwww ` + `$` + `100` stays joined | `parity_text::uax14_lb28_glue` |
| `latin-hyphen-wrap` | `aaaa bbbb non-managed` @140 → 2 lines | `parity_text::hyphen_break_latin` |
| `percentage-line-spacing` | `spcPct 80` → pitch `0.96·size` | `parity_text::ln_spc_pct` |
| `percentage-paragraph-spacing` | `spcBef 50 %` of the *next* line | `parity_text::spc_bef_pct_next_line` |
| `powerpoint-line-metrics` | Arial 72 pt baselines `[97,217]` | `parity_metrics::face_ascent_share` |
| `firstline-indent-autofit` | bulleted positive indent → autofit ignores it | `parity_text::autofit_ignores_bullet_indent` |
| `buszpts-bullet-size` | `buSzPts` wins over co-present `buSzPct` | `parity_bullets::size_pts_precedence` |
| `empty-bullet` | paragraph with only `<a:br/>` is not renderable | `parity_bullets::renderable_content` |
| `table-border-conflict` | null loses / wider wins / darker wins / owner wins | `parity_tables::border_conflict_order` |
| `table-border-single-draw` | shared gridline stroked once with winner's style | `parity_tables::shared_gridline_once` |
| `custgeom-path-paint` | per-`a:path@fill/@stroke` honoured | `parity_shapes::custgeom_per_path_paint` |
| `bevel-band-conversion` | `w·cssScale·devScale ±1`, no double scale | `parity_shapes::bevel_band_emu_to_px` |

### 5.4 Fidelity VRT (the reference's `visual.spec.ts`, faithfully)

- Corpus: `packages/pptx/public/demo/sample-1.pptx` → `tests/fixtures/parity/sample-1.pptx`
  (1.98 MB, 9 slides) and `tests/visual/references/demo/sample-1/slide-1..9.png` (4.6 MB, 1280×720)
  → `tests/fixtures/parity/reference/sample-1/`. Copy with provenance in `crates/VENDORED.md`:
  repo MIT, deck embeds 8 unattributed JPEGs (redistribution risk to note), reference PNGs are
  renderer outputs, not Office exports.
- Render: width 1280, dpr 1, `height = round(720)`, dispatch exactly like
  `renderSlide(canvas, slide, w, h, { width: 1280, dpr: 1 })`.
- Compare: pixelmatch port, `threshold 0.20`, `includeAA`, pad to `max(w)×max(h)` with white;
  `diffPct = diff/(w·h)·100`; fail if `> 20`; ratchet fail if `match < prior − 0.5`.
- Modes: `UPDATE_REFS=1`, `UPDATE_SCORES=1`, `VRT_SNAPSHOT=1`; self-VRT is exact (`pixels == pixels`).
- Treat the 20 % ceiling as a *smoke alarm*, not evidence: slides 1–4, 6–9 already pass at 95–98 %
  while visibly wrong. Add per-slide structural assertions (wrap line counts, chart title size,
  baseline deltas) alongside the score.

### 5.5 Determinism

- Bundle Carlito/Caladea/Liberation + one CJK face and one face with vertical forms; register them in
  one `FontSystem`; prefer bundled over system; warn on every miss (today silent).
- Metric authority must be the face that paints: `FaceMetrics { ascent_share, us_win_ascent,
  us_win_descent, underline }` from the same face program.
- Emit a `RenderedTextLine { baseline_y, ascent, descent, runs }` record so tests assert layout
  numbers, not pixels.

---

## 6. What not to do

- **Do not port the viewer/scroll/worker specs** (~469 cases): they test DOM lifecycle, zoom,
  selection overlays and worker handshakes. Replace with TUI state tests (already 71) and driver
  stream goldens (already 12).
- **Do not chase PowerPoint ground truth.** The reference's own oracle is its previous renderer.
  Fix the contract, then ratchet against ourselves.
- **Do not translate `presets.json` formulas into Rust by hand.** Evaluate the strings; 85 of 186
  presets depend on `ss` and the derived-scale idioms that hand ports get wrong.
- **Do not keep the display list and add "just one more field".** Pattern/image fills, clip stacks,
  effect layers and text lines are the Canvas API; implement it as a device trait.
- **Do not fake 3-D or chart-ex.** The reference itself has explicit unsupported paths; match them.

---

## 7. Definition of done

1. `sample-1` slide 1 renders `STATE OF / THE / FOREST` on three lines inside the title box.
2. Slide 5 chart title is 16 pt, measured bands (no overlap with the caption), vertical text rotated.
3. No text run on any demo slide overflows its shape horizontally (wrap + autofit).
4. Preset parity harness: 186/186 presets within 0.01 px of the reference evaluator at three box
   aspect ratios and default/extreme adjustments.
5. `srcRect` crops pixels (verified on a synthetic 2× crop), pictures clip to their silhouette, image
   and pattern fills paint, arrowheads and compound lines render.
6. Raster and SVG of the same slide differ only by glyph rasterisation (structural equality test).
7. `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green;
   parity fidelity scores committed with a ratchet; every source file < 600 lines.

---

## Appendix A — Reproducing the measurements

```sh
# reference source (sparse)
git clone --filter=blob:none --depth 1 --sparse \
  https://github.com/yukiyokotani/office-open-xml-viewer.git /tmp/oooxml
git -C /tmp/oooxml sparse-checkout set packages/pptx packages/core

# our render of the reference demo deck
kitty-pptx export /tmp/oooxml/packages/pptx/public/demo/sample-1.pptx \
  --format png --dpi 96 -o /tmp/parity/ours      # 12192000 EMU → 1280 px at 96 dpi

# comparison used for §1 (pixelmatch YIQ, threshold 0.20, includeAA)
python3 /tmp/parity/compare.py
```

Verified spot checks used in this document:

- `rasterizer/mod.rs:56` `min(W/Wemu, H/Hemu)` + `dx,dy`; reference `renderer.ts:8368` single-axis.
- `compiler/text.rs:36` `set_size(None, None)`; `text.rs:237` the only line split; `rasterizer/text.rs:60-81`
  first-glyph anchor + `0.85` baseline.
- `compiler/charts/layout.rs:206` `/2.0` vs `packages/core/src/chart/layout.ts:153` `/100`.
- `compiler/pictures.rs:27-30` `/100000` on already-fractional `srcRect`
  (`ooxml-common/src/blip.rs:132`).
- `rasterizer/shape_effect.rs:279` `radius/2` vs `packages/core/src/shape/effects.ts:323` σ = radius.
- `compiler/style.rs:111` cap `Square` vs `stroke-geometry.ts:19` `butt`.
- `rasterizer/mod.rs:182` / `svg/mod.rs:257` clip no-ops.
- Reference `presets.json` = 186 definitions; our dispatch = ten hand-written modules.

## Appendix B — Pre-existing workspace state

`git status` before this analysis already showed edits in
`crates/office-terminal-core/src/compiler/text.rs` (debug print) and
`crates/office-terminal-core/src/rasterizer/text.rs` (`query_face` fallback chain + debug print).
Those were preserved untouched.

---

## 8. Implementation status (this pass)

Landed and verified in `crates/office-terminal-core` (all files < 600 lines, clippy clean):

| area | modules | tests |
|---|---|---|
| text layout | `text/{metrics,linebreak,justify,tabs,emit,paint,shape,fonts,vertical}.rs` | parity text specs, emit tests, metrics/break/justify unit tests |
| bundled faces | `text/fonts.rs` + `assets/fonts` (Carlito, Caladea, Nunito Sans, OFL) | font resolution used by layout, raster and SVG |
| preset geometry | `compiler/shapes/{preset_data,preset_eval,preset_exec,custgeom}.rs` + `presets.json` | 6138-case oracle comparison + 10 geometry tests |
| paint/stroke | `compiler/style.rs`, `rasterizer/path.rs`, `svg/path.rs` | 11 stroke/gradient tests (spec dash table, butt default, 0.5 px hairline, miter 10, box gradient frame) |
| pictures | `compiler/pictures.rs`, `svg/image.rs` | 8 crop tests (fractional srcRect, outset, SVG viewBox) |
| clip stack | `rasterizer/mod.rs`, `svg/mod.rs` | 12 tests (mask compositing, nested intersection, SVG clipPath) |
| tables | `compiler/tables.rs` + `border_conflict.rs` | 6 tests (ownership, conflict order, two passes, RTL, span split, row growth) |
| effects | `rasterizer/{filters,shape_effect}.rs` | effects parity tests (order, sigma, glow, soft edge, inner shadow) |
| charts | `compiler/charts/*` | units/gap/overlap tests (hpt/100, gapWidth 150) |
| harness | `tests/parity_support/{pixelmatch,deck,scores,vrt}.rs`, `tests/parity_fidelity_tests.rs` | pixelmatch vs reference on 27,696 randomised images; synthetic deck builder; per-slide score ratchet |

Measured on the reference demo deck at 1280x720 (pixelmatch YIQ 0.20, includeAA):

| slide | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
|---|---|---|---|---|---|---|---|---|---|
| before this pass (%) | 95.82 | 98.00 | 95.44 | 97.90 | **79.38** | 97.97 | 95.18 | 97.16 | 97.97 |
| after (%) | 95.74 | 98.01 | 95.82 | 98.28 | **95.98** | 98.43 | 95.76 | 97.03 | 97.78 |

Slide 5's 16.6 points came from one unit fix (`hpt/100`); the rest of the delta is stroke/gradient,
text wrapping and table borders. The remaining pixel gap is dominated by layout boxes we inherit
differently (see §8.1), not by paint.

### 8.1 Known remaining gaps

1. **Reference PNGs use substituted fonts.** The demo PNGs render slide 1's title with Arial
   metrics, not the deck's Nunito Sans: measured ink widths are 446.9 / 185.7 / 379.3 px for Arial
   versus 435.5 / 186.8 / 359.2 px for Nunito Sans, and the PNG measures 448 / 186 / 380. Only the
   Arial advance of `THE FOREST` (608.0 px) exceeds the 606 px text budget, which is exactly why the
   reference wraps three lines. With the authored Nunito Sans our two-line wrap is correct, so this
   is a font-substitution difference in the oracle, not a layout bug.
2. **Vertical text `eaVert` upright substitution** is approximated (rotation, not per-glyph UAX#50).
3. **Charts** only render the column/bar/line/area/pie/doughnut subset; other families now show the
   labelled placeholder (parity with the reference's unsupported path) but are not drawn.
4. **`eaVert` Latin segments** are emitted with a rotated frame, but the rasteriser still applies one
   transform per `DrawText` command, so a mixed Han+Latin column can lose the Latin rotation. Upright
   EA cells and the U+FE1x/FE3x substitutions are emitted per run and render correctly.
5. **3-D bevel/extrusion** remains the previous approximation; the reference's material/rig shading
   is not ported.
6. **Table cell clipping**, `cmpd` double borders and device-pixel crisp snapping are not
   expressible in the current IR.

### 8.1.1 Second slice

Landed after the first commit: `rPr@baseline` super/subscript (0.65 size, `-(baseline/100000)*size`
shift), background image fills (`srcRect`, `fillRect`, tile, alpha, white base), measured chart
bands (title/legend/axis-title, elision, manual layout), and a font-metric investigation that
explains the slide-1 wrap difference above.

Fourth slice: SVG reflection now mirrors the raster semantics (`dir`, `stPos`/`endPos`, `sx`/`sy`,
`algn`, `dist`, padded filter region) and `eaVert` emits upright East-Asian cells with Tu/Tr
vertical-form substitutions instead of rotating the whole run.

Third slice: chart-family dispatch (unsupported families now draw the reference's labelled
"Unsupported chart" placeholder with a warning instead of a silent column chart), reflection
parameters (`dir`, `stPos`/`endPos`, `sx`/`sy`, `algn`, `dist`; vendored patch 0002 corrects the
ECMA default `dir = 5400000`), and table cell clipping plus compound (`cmpd`) borders.

### 8.2 Test entry points

```sh
cargo test -p office-terminal-core --test parity_text_tests
cargo test -p office-terminal-core --test preset_parity_tests
cargo test -p office-terminal-core --test parity_fidelity_tests -- --nocapture
cargo test -p office-terminal-core --test pixelmatch_tests
cargo clippy --workspace --all-targets -- -D warnings
```

### 8.3 Test port status

The reference suite is now ported file-for-file: **161 reference files** (the 150
`packages/pptx/src` specs plus the 14 `tests/visual` Playwright specs, deduped
across batches) are covered by **33 `parity_*` targets** — 32 under
`crates/office-terminal-core/tests/` and one in `crates/office-terminal-tui/tests/` —
holding 525 test functions, 422 passing, 103 `#[ignore]`d stubs, 0 failing,
clippy clean. Of **1184 enumerated reference cases**, 214 are EXACT (the
assertion is the reference's own number), 179 are BEHAVIOURAL (same state
machine, expressed on the IR instead of canvas ops), 263 are N/A
(DOM/canvas/`ImageBitmap` surfaces with no Rust analogue) and **528 are
BLOCKED** — each with a stub whose ignore reason names the missing symbol. The
blockers concentrate in four places: the installed-face metric catalogue
(`powerPointFaceMetrics`/`powerPointAscentShare`, 50 cases), the 3-D bevel and
reflection compositor (48), the viewer platform surface (zoom, hidden slides,
selection/find, worker pull protocol, container guards — 247), and equation
fallbacks, decode/placeholder plumbing, `rPr@spc` on the draw IR and the ICU
Southeast-Asian dictionary break (the remaining 183). Per-batch ledgers live in
`crates/office-terminal-core/tests/parity/COVERAGE.md`.

