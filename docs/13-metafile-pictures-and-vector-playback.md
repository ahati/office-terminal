# PRD 13: Metafile Pictures — EMF / WMF / EMF+ Vector Playback

> Status: **design / implementation plan** (normative for the implementing PRs).
> Depends on: [PRD 02a](./02a-composition-core-and-render-pipeline.md) (compile stage), [PRD 02c](./02c-typography-and-text-layout.md) (text shaping seam), [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) (fidelity policy), [PRD 06](./06-component-interface-contracts.md) (display-list contracts), [PRD 07](./07-quality-and-testing.md) (test levels).
> Validation details: [PRD 13a](./13a-metafile-fidelity-and-validation.md).
> External engine: [`metafile-rs`](https://github.com/Dantusaikamal/metafile-rs) 1.0.1 (Apache-2.0), vendored.

---

## 1. Problem, Goals & Non-Goals

### 1.1 Current state

`<p:pic>` blips that reference an **EMF** or **WMF** part are compiled to `VectorCommand::DrawImage` and then dropped by the decoder, because `image::load_from_memory` has no EMF/WMF support (`AssetError::DecodeFailed`). The rasterizer's fail-soft path (`Err(_) => return`) paints nothing and emits no warning, so the picture is an invisible hole in both the TUI and exports. Metafiles are common in real decks (figures exported from MATLAB/R/matplotlib, Excel charts pasted as pictures, legacy Office art, Visio), and many are **full-bleed**, making the hole look like a layout bug.

PRD 02d currently classifies EMF/WMF as `PLACEHOLDER` with "PNG fallback used if present; else metafile placeholder (§3) + warning". Even that behaviour is not implemented today.

### 1.2 Goals

1. **Vector playback**: compile EMF, EMF+ and WMF pictures into `VectorSlide` display-list commands, so they render with full resolution in the terminal rasterizer **and** in the SVG exporter from the same compiled scene.
2. **Honest fidelity**: every successfully played record paints; every approximation emits a typed warning; every failure degrades to the §3 placeholder — never an invisible hole.
3. **Determinism**: identical bytes produce identical `VectorCommand` output across runs, processes and backends.
4. **Bounded work**: parsing/playback is subject to explicit resource limits; no panics on malformed input; no unbounded memory.
5. **One playback per metafile**: parse and translate once per unique content; rasterization and SVG export replay the cached display list at any target.
6. **No platform dependencies**: no Windows GDI, LibreOffice, Inkscape, browser DOM or network at build or runtime.

### 1.3 Non-goals

- Pixel-identical Windows GDI/GDI+ output. The engine targets a faithful vector reconstruction, not device-driver emulation.
- Writing/editing EMF/WMF (`emfsdk`-style round-tripping) or printing.
- Animated or time-dependent metafile playback (EMF has none; `EMF+` timing records are ignored with a diagnostic).
- Rasterizing metafiles at compile time (see §9.3 rationale).

---

## 2. Normative Dependencies

### 2.1 Vendored crates (exact pin)

| Crate | Version | Role | License |
| :--- | :--- | :--- | :--- |
| `metafile-core` | `=1.0.1` (crates.io) | Renderer-neutral primitives: `Renderer`, `Pen`, `Brush`, `Paint`, `Stroke`, `Font`, `TextRun`, `Bitmap`, `BitmapPlacement`, `BitmapSampling`, `ClipRegion`, `Mapping`, `ResourceLimits`, `Diagnostic` | Apache-2.0 |
| `metafile-emf` | `=1.0.1` (crates.io) | Classic EMF parsing, DC state machine, EMF+ stream arbitration, `playback(bytes, options, &mut dyn Renderer)` | Apache-2.0 |
| `metafile-wmf` | `=1.0.1` (crates.io) | WMF (standard + placeable) playback | Apache-2.0 |
| `metafile-emfplus` | `=1.0.1` (**local patched**) | EMF+ record playback (Only / Dual) | Apache-2.0 |
| `metafile-dib` | `=1.0.1` (**local patched**) | Bounded DIB/BMP decoding shared by the players | Apache-2.0 |

**Not used:** `metafile` (high-level convenience wrapper) and `metafile-svg` (its SVG backend). We implement the renderer ourselves to emit `VectorSlide` commands. A differential oracle can add `metafile-svg` as a **dev-dependency** later ([PRD 13a](./13a-metafile-fidelity-and-validation.md) §6); it is never linked into release binaries.

**Consumption model:** unpatched crates come from crates.io pinned with `=`; only the crates carrying local fixes live in-tree and are wired through `[patch.crates-io]`. This keeps the source tree small and the provenance auditable while preserving the crop-clamping fixes.

### 2.2 Patch & dependency policy

- **crates.io crates** (`metafile-core`, `metafile-emf`, `metafile-wmf`) are pinned with `=1.0.1` in `[workspace.dependencies]`; `Cargo.lock` pins the checksums. A version bump requires the metafile corpus to pass.
- **Patched crates** (`metafile-emfplus`, `metafile-dib`) live under `crates/` and are selected via `[patch.crates-io]`. They are listed in `crates/VENDORED.md` with upstream revision, license and patch references, following [AGENTS.md](../AGENTS.md) §E.
- Every behavioural change to a patched crate is a minimal diff in `crates/patches/` (currently `0003-metafile-clamp-bitmap-crops.patch`), never an undocumented edit.
- Once the clamp fixes are released upstream, delete the two local copies, drop the `[patch.crates-io]` entries and consume the new pinned version from crates.io.
- Both patched crates are excluded from `cargo clippy --workspace`/`cargo test --workspace` so upstream code is not held to first-party lints; `-p kitty-pptx-core` still compiles them as dependencies.
- Apache-2.0 is compatible with this project's MIT license; each local crate keeps its LICENSE and `crates/VENDORED.md` records the NOTICE.
- **Supply-chain audit (pinned revision):** no `unsafe` blocks, no `build.rs` scripts, no C bindings; transitive dependencies are `serde`, `thiserror`, `encoding_rs`, `png`, `jpeg-decoder`. The audit result is recorded in `crates/VENDORED.md` and re-run on every version bump.

### 2.3 Why this engine (alternatives considered)

| Candidate | Assessment |
| :--- | :--- |
| **`metafile-rs`** | ✅ Renderer-neutral `Renderer` trait maps almost 1:1 to `VectorCommand`; EMF + EMF+ Only/Dual + WMF; structured diagnostics; resource limits; fuzz targets; real-world fixtures; Apache-2.0. ⚠️ Young (v1.0.1, single maintainer) → vendored + wrapped behind our own trait. |
| `betteroffice-metafile` | Good second choice: `replay`/`decode` → `Drawing`/`MetafileOp` display list, Apache-2.0. Coupled to the `ooxml-drawingml` model and a large monorepo; kept as the drop-in alternative behind `MetafileEngine`. |
| `emfsdk` | Typed read/edit/write + an optional **raster-only** renderer. Wrong output shape (no display list); would force bitmap embedding. |
| `emf-core` / `wmf-core` | Parser + direct SVG emission, no renderer seam. Rejected: we do not re-parse SVG. |
| Hand-written GDI player | 8–15k lines to cover EMF + EMF+ dual mode with acceptable fidelity; not justified while a maintained engine exists. |

---

## 3. Architecture

### 3.1 Pipeline

```
<p:pic r:embed="ppt/media/imageN.emf">
        │
        ▼
compile_picture_element                  (compiler/pictures.rs)
        │  classify blip: raster | svg | metafile | unknown
        ▼
VectorCommand::DrawMetafile { rel_id, transform, src_rect, dest_rect, opacity }
        │                                  (thin marker: no bytes read here)
        ▼
expand_metafiles(&mut VectorSlide, assets, warning_sink)
        │   runs once per compiled slide in the pipeline / export / dump path
        ▼
MetafileResolver::resolve(rel_id)        (content-addressed cache)
        │
        ▼
MetafileEngine::play(bytes, limits, sink)
        │
        │  metafile-emf / -emfplus / -wmf: DC state, mapping, objects,
        │  clipping, EMF+ arbitration, DIB decode
        ▼
MetafileSink calls  (renderer-neutral, output coordinates)
        │
        ▼
MetafileRecorder  ──►  Vec<VectorCommand>   (picture-local EMU)
        │
        ▼
marker replaced by expanded commands (picture transform composed)
        │
        ▼
VectorSlide.commands  ──►  tiny-skia raster   /   SVG serializer
```

**Why a marker + expansion step instead of playing during `compile`:** the `SlideCompiler` contract ([PRD 06](./06-component-interface-contracts.md) §5.2) has no `AssetStore` and never touches archive bytes. Expansion runs where both the display list and the store are available — `DefaultRenderPipeline`, the CLI export path, `dump`, and the golden harness — and its output is a fully self-contained `VectorSlide` (no marker remains). Warnings are emitted during expansion, i.e. still at compile time, before rasterization.

The metafile is **never** rasterized at compile time and never re-parsed per render target.

### 3.2 Module layout (`crates/kitty-pptx-core/src/metafile/`)

| File | Responsibility | Budget |
| :--- | :--- | :--- |
| `mod.rs` | Public traits (`MetafileEngine`, `MetafileSink`), types (`MetafileFormat`, `MetafileLimits`, `MetafileInfo`, `MetafileDiagnostic`), re-exports | < 220 lines |
| `classify.rs` | Extension/MIME/header classification (`emf`, `wmf`, `emz`, `wmz`, unknown), magic-byte sniffing | < 160 |
| `engine_metafile_rs.rs` | The only file that names `metafile_*` crates; implements our `MetafileEngine` and a `metafile_core::Renderer` that forwards to `MetafileSink` | < 450 |
| `recorder.rs` | `MetafileRecorder`: sink → `VectorCommand`, clip stack, transform composition, warning collection | < 450 |
| `expand.rs` | `expand_metafiles(scene, assets, warnings)`: replaces `DrawMetafile` markers with expanded commands or the placeholder; composes the picture transform into every emitted command (including clip paths) | < 260 |
| `mapping/pen.rs` | `Pen` → `VectorStroke` (widths, dash, caps, joins, miter, compound) | < 260 |
| `mapping/paint.rs` | `Brush`/`Paint` → `VectorPaint` (solid, hatch, pattern, linear/radial gradient, texture) | < 300 |
| `mapping/path.rs` | `Path`/`ArcKind` → `VectorPath`; fill-mode mapping; degenerate guards | < 320 |
| `mapping/text.rs` | `TextRun` → `PositionedGlyphRun`(s); alignment, escapement, `dx`, decorations, background | < 380 |
| `mapping/bitmap.rs` | `BitmapPlacement` → affine; PNG encode; sampling | < 260 |
| `mapping/clip.rs` | `ClipRegion` → clip paths; exact vs bbox-approximated combine | < 240 |
| `cache.rs` | Content-addressed compiled-command cache (LRU + byte budget) | < 220 |
| `limits.rs` | `MetafileLimits` defaults + per-deck budget accounting | < 160 |
| `diagnostics.rs` | Engine diagnostics → `WarningEvent` mapping, dedup, class resolution | < 220 |

Every new source file stays under the 600-line ceiling; the table above keeps headroom for review fixes.

### 3.3 Stable seam

```rust
/// Swappable playback backend (metafile-rs today; betteroffice or a local
/// player later). The compiler depends only on this trait.
pub trait MetafileEngine: Send + Sync {
    fn format(&self, bytes: &[u8]) -> Option<MetafileFormat>;
    fn inspect(&self, bytes: &[u8]) -> Result<MetafileInfo, MetafileError>;
    fn play(
        &self,
        bytes: &[u8],
        limits: &MetafileLimits,
        sink: &mut dyn MetafileSink,
    ) -> Result<PlaybackOutcome, MetafileError>;
}

/// Renderer-neutral commands in *output coordinates* (metafile device units).
pub trait MetafileSink {
    fn push_clip(&mut self, clip: &MetafileClip) -> Result<(), MetafileError>;
    fn pop_clip(&mut self);
    fn draw_path(
        &mut self,
        path: &VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        fill_rule: FillRule,
    ) -> Result<(), MetafileError>;
    fn draw_text(&mut self, run: &MetafileTextRun) -> Result<(), MetafileError>;
    fn draw_bitmap(&mut self, bitmap: &MetafileBitmap) -> Result<(), MetafileError>;
    fn diagnostic(&mut self, diagnostic: MetafileDiagnostic);
}
```

`MetafileRecorder` implements `MetafileSink`; `engine_metafile_rs.rs` implements `metafile_core::Renderer` by forwarding to the sink. Swapping engines therefore touches exactly one file.

```rust
/// Replaces every `DrawMetafile` marker in `scene` with its playback output.
///
/// - one `MetafileResolver::resolve` per unique (rel_id, content hash);
/// - the marker's `transform` is composed into every emitted command and clip path;
/// - failures emit a placeholder plus the mapped warning;
/// - a marker seen by a backend (unexpanded) renders the placeholder defensively.
pub fn expand_metafiles(
    scene: &mut VectorSlide,
    assets: &dyn AssetStore,
    warnings: &crossbeam_channel::Sender<WarningEvent>,
) -> MetafileExpansionSummary;
```

---

## 4. Playback Adapter: `Renderer` → `VectorCommand`

The adapter receives **output coordinates** (device units after window/viewport/world mapping) and produces commands in **picture-local EMU** (§5).

### 4.1 Lines, polylines, polygons

| `Renderer` call | Emission |
| :--- | :--- |
| `line(from, to, pen)` | `DrawPath { path: [MoveTo, LineTo], fill: None, stroke: pen, transform: picture }` |
| `polyline(points, pen)` | one open path; closed polylines stay open (GDI semantics) |
| `polygon(points, pen, brush, fill_mode)` | closed path; fill + stroke |
| `poly_polygon(polys, …)` | one `DrawPath` per polygon with the shared brush/pen (GDI paints each) |
| `pixel(point, color)` | 1×1 `DrawPath` with `VectorPaint::Solid` |

`fill_mode` maps to `FillRule::EvenOdd` / `FillRule::NonZero` (§4.3).

### 4.2 Pens and strokes

| Engine field | `VectorStroke` |
| :--- | :--- |
| `color` | `color: [u8; 4]` |
| `width` (output units) | `width_emu = width × scale` (§5.3); `cosmetic` → `max(1 device px)` |
| `style` (solid/dash/dot/dash-dot/…) | `dash_segments` mapped to GDI pattern in width multiples; custom `ExtCreatePen` dashes preserved |
| `line_cap` / `line_join` | `LineCap` / `LineJoin` 1:1 |
| miter limit | `miter_limit` (default 10) |
| — | `head_end`/`tail_end`: `None` (EMF lines carry no DrawingML arrowheads) |
| — | `compound`: `Single` |

Unsupported pen styles (e.g. geometric hatched pens) degrade to solid and emit `metafile_pen_style` (`DEGRADED`).

### 4.3 Paths, rectangles, ellipses, arcs

- Engine `Path` segments (move/line/cubic/quadratic/arc/close) map directly to `PathSegment`.
- GDI `ArcTo` is converted to cubic fragments by the engine; `arc(kind)` (arc/pie/chord) is closed/sectored per `ArcKind` with a maximum angular step of $5^\circ$ ($\le 72$ segments per full circle).
- `rectangle(rect, radius)` → rounded-rect path (radius `None` = square corners).
- `ellipse(rect)` → four cubic arcs with the standard $\kappa = 0.5522847498$ control points.
- `fill_mode` → `FillRule`; an empty path after conversion is skipped (no command, no warning).

### 4.4 Brushes and paints

| Engine | `VectorPaint` | Class |
| :--- | :--- | :--- |
| `BrushStyle::Solid` | `Solid(color)` | FULL |
| `BrushStyle::Null` | `None` (no fill) | FULL |
| `BrushStyle::Hatched(style)` | `Pattern(VectorPattern)` using the existing 32×32 DrawingML pattern atlas approximation | DEGRADED (`metafile_hatch`) |
| `BrushStyle::Pattern` | `Image(VectorImageFill)` from the pattern DIB | DEGRADED (`metafile_pattern_brush`) |
| `Paint::Solid` | `Solid` | FULL |
| `Paint::LinearGradient` | `LinearGradient(VectorGradient)` (angle from start→end; stops mapped) | FULL |
| `Paint::RadialGradient` (EMF+) | `RadialGradient(VectorGradient)` | FULL |
| `Paint::Hatch` | `Pattern` or solid foreground | DEGRADED (`metafile_hatch`) |
| `Paint::Texture` | `Image(VectorImageFill)` (bitmap registered via §6.2) | DEGRADED if transformed beyond the representable frame |

Brush patterns and textures reuse `VectorPaint::Image`/`VectorPattern`; no new paint variants are introduced.

### 4.5 Text

`TextRun` → one or more `DrawText` commands:

1. **Font**: `Font { height, weight, italic, underline, strike_out, family }` → `text::shape(text, family, size_emu, bold, italic)` (the existing shaping seam, [PRD 02c](./02c-typography-and-text-layout.md) §2). Family resolution uses the same substitution matrix; a substitution emits `WarningEvent::FontSubstituted`.
2. **Size**: `height` (output units) × `scale` → EMU; `font_size_pt = size_emu / 12700`.
3. **Position**: `TextRun.position` → run origin; `horizontal_align` (left/center/right) and `vertical_align` (top/center/bottom/baseline) offset the glyph block using the shaped width/ascent.
4. **Advances**: when `dx.len() == char count`, `dx × scale` overrides glyph advances and cumulative x positions; otherwise the shaped advances are used and `metafile_text_dx` is emitted.
5. **Escapement**: `escapement_tenths / 10` degrees. Each rotated run becomes its own `DrawText` with `transform = picture_transform ∘ rotate(θ)`; 0°/90°/180°/270° are exact, arbitrary angles are supported by the same affine.
6. **Decorations**: `underline`/`strike_out` → `TextDecorations` (existing renderer paths).
7. **Opaque background**: `background`/`background_rect` → a solid `DrawPath` emitted immediately before the run (the EMF `ETO_OPAQUE` behaviour).
8. **Right-to-left**: `right_to_left` → reorder runs and set the bidi base direction in the shaping call.

### 4.6 Bitmaps

`bitmap(placement, bitmap, sampling)`:

- `BitmapPlacement { origin, x_axis, y_axis }` is an affine basis mapping the bitmap's $[0,1]^2$ into output coordinates. Convert to an `AffineTransform` and scale into picture-local EMU.
- Encode the decoded RGBA grid to PNG **once** at compile time and emit `VectorCommand::DrawBitmap` (§6.2); the same bytes serve raster and SVG.
- `sampling` maps to `BitmapSampling::{Nearest, Linear}` in the command; the rasterizer chooses `FilterQuality::{None, Bilinear}`.
- Bitmaps larger than the resource limit are downsampled with a documented filter (Lanczos3 via `image`) before encoding and marked `metafile_bitmap_downsampled`.
- `BitmapPlacement` with shear/rotation is preserved exactly (affine), not approximated by a rect.

### 4.7 Clipping

`ClipRegion` → one `PushClip(path)` / matching `PopClip` per engine clip scope:

| Region | Handling | Class |
| :--- | :--- | :--- |
| `Rect` | rectangle path | FULL |
| `Polygon` | closed polygon path (convex by contract) | FULL |
| `Path { path, fill_mode }` | path + `FillRule` | FULL |
| `Intersection(Vec<_>)` | one `PushClip` per operand (nested clips intersect) | FULL |
| `Combine { AND }` | nested `PushClip`s | FULL |
| `Combine { OR / XOR / DIFF }` | **exact compound path** for simple closed rect/polygon operands: OR concatenates figures (`NonZero`), XOR uses `EvenOdd`, DIFF intersects the left clip with `left + reversed(right)` (`NonZero`). Curved/nested/`Complement` operands keep the conservative bounding-box approximation | FULL for exact cases; DEGRADED (`metafile_clip_combine`) for the fallback |

The approximation never paints outside a conservative region, so it cannot leak ink outside the picture. Exact region booleans (via `i_overlay`) are a follow-up if the corpus shows visible gaps ([PRD 13a](./13a-metafile-fidelity-and-validation.md) §8).

### 4.8 Diagnostics

Every engine `Diagnostic` is mapped to exactly one warning class and deduplicated per (element, tag) before emission:

| Engine diagnostic category | Tag | Class |
| :--- | :--- | :--- |
| Unsupported/ignored record | `metafile_record_ignored` | DEGRADED |
| Approximated ROP (`ROP2` ≠ copy) | `metafile_rop` | DEGRADED |
| Approximated clip combine | `metafile_clip_combine` | DEGRADED |
| Hatched/pattern brush approximation | `metafile_hatch`, `metafile_pattern_brush` | DEGRADED |
| Bitmap downsampled / sampling approximated | `metafile_bitmap_downsampled`, `metafile_bitmap_sampling` | DEGRADED |
| EMF+ dual stream fell back to classic records | `metafile_emfplus_fallback` | DEGRADED |
| Font substitution | `WarningEvent::FontSubstituted` | DEGRADED |
| Picture opacity < 1 (group alpha not representable) | `metafile_opacity` | DEGRADED |
| Resource limit hit | `WarningEvent::ResourceLimitHit` | PLACEHOLDER |
| Parse/validation error | `WarningEvent::MediaFailed` | PLACEHOLDER |

---

## 5. Coordinate Systems & Units

### 5.1 Source and target spaces

- **Engine output space**: device units after window/viewport/world mapping. The playback result exposes `output_bounds` (device units) and `physical_size` (0.01 mm units).
- **Picture-local space**: the picture box, origin at its top-left, $x \in [0, W_{pic}]$, $y \in [0, H_{pic}]$ in EMU.
- **Slide space**: `compute_shape_transform(pic.x, pic.y, pic.width, pic.height, rotation, flip_h, flip_v)` maps picture-local EMU to slide EMU, exactly as for raster pictures today.

### 5.2 Scale and placement

$$ s_x = \frac{W_{pic}}{bounds.width}, \qquad s_y = \frac{H_{pic}}{bounds.height} $$

- Coordinates are mapped as $x_{emu} = (x - bounds.left)\cdot s_x$, $y_{emu} = (y - bounds.top)\cdot s_y$.
- The picture is **stretched** to its authored frame, matching how PowerPoint draws a metafile picture; aspect is not preserved independently (the authored picture box already encodes it).
- `bounds.width`/`height` ≤ 0, non-finite, or larger than `max_dimension` → `PLACEHOLDER` with `metafile_bounds`.
- `physical_size` is used only to derive DPI-aware pen widths when `output_bounds` is absent.

### 5.3 Pen widths and cosmetic pens

- Geometric pen width: `width_emu = round(pen.width × (s_x + s_y) / 2)`.
- Cosmetic pens are one device pixel wide in GDI regardless of mapping; they map to `width_emu = max(1 device px × s_x, 1)` so the stroke stays >= 1 raster pixel at every target.
- Dash patterns are expressed in pen-width multiples, so they scale with the pen.

### 5.4 Text units

- `Font.height` is in output units; `size_emu = height × s_y`; `font_size_pt = size_emu / 12700`.
- Negative `lfHeight` (character height) and zero-height fonts are normalised by the engine; the adapter clamps `size_pt` to $[0.5, 400]$ and emits `metafile_text_size` if clamped.
- Escapement is applied as a rotation about the run origin (§4.5).

### 5.5 Numeric guards

All adapter arithmetic uses `f64` at the engine boundary and converts to `f32` only at contract boundaries. Non-finite coordinates, zero-length paths, zero-size bitmaps and degenerate arcs are skipped with a diagnostic; they never produce NaN in the display list.

---

## 6. Contract Delta (PRD 06)

Two display-list variants are added: a thin `DrawMetafile` marker emitted by the compiler, and a `DrawBitmap` command emitted by expansion for embedded raster records.

### 6.1 `VectorCommand::DrawMetafile` (compiler → expansion)

```rust
/// A picture backed by an EMF/WMF/EMF+ part. The compiler cannot read archive
/// bytes, so it emits a marker that `expand_metafiles` replaces with playback
/// output (or the §3 placeholder on failure).
DrawMetafile {
    rel_id: String,
    /// Picture transform (position, size, rotation, flip) — composed into every
    /// emitted command and clip path during expansion.
    transform: AffineTransform,
    /// `<a:srcRect>` crop window (`NormalizedRect`, full by default).
    src_rect: NormalizedRect,
    /// `<a:stretch><a:fillRect>` destination in picture-local EMU.
    dest_rect: RectF32,
    /// `<a:alphaModFix>` opacity (`1.0` = opaque).
    opacity: f32,
},
```

Invariants:

- **No marker survives expansion.** `expand_metafiles` must remove every `DrawMetafile`; a backend that still sees one renders the §3 placeholder and emits `metafile_unexpanded` (defensive, never a blank region).
- `src_rect` crop is applied by clipping to the visible destination and scaling the source window into it; an empty crop window paints nothing (same guard as `DrawImage`).
- `opacity < 1` is `DEGRADED`: the display list has no group-alpha command. Bitmap commands apply it, vector commands ignore it, and `metafile_opacity` is emitted once per picture.
- Both backends must handle the marker defensively; `debug_assert!` that expansion ran in tests.

### 6.2 `VectorCommand::DrawBitmap` (expansion → backends)

EMF bitmaps live inside the metafile stream and have no PPTX relationship id, so `DrawImage { rel_id }` cannot reference them. Add one display-list variant:

```rust
/// A bitmap embedded in another container (EMF/WMF), carried inline so the
/// display list stays self-contained across backends and cache targets.
DrawBitmap {
    /// PNG-encoded pixels, encoded once at compile time.
    png: Arc<[u8]>,
    /// Intrinsic pixel size (diagnostics, sampling decisions, AST hashes).
    width: u32,
    height: u32,
    /// Maps the bitmap's [0, 1]² unit square into element-local EMU
    /// (translation, scale, rotation, shear — an arbitrary affine).
    placement: AffineTransform,
    opacity: f32,
    sampling: BitmapSampling,
},
```

Rationale for inline PNG rather than a synthetic asset id:

- `SlideCompiler` has no `AssetStore`; registering bytes in the store would require threading mutable state through the compile trait.
- `VectorSlide` must stay self-contained: it is cached by slide index, cloned into `Arc`s, and consumed by two backends with the same object.
- PNG keeps the AST/JSON and SVG data URIs compact; the rasterizer decodes once per `Arc` (a small decode cache keyed by pointer identity).

### 6.3 Raster semantics

- Decode via `image` (PNG), then draw with `tiny_skia::Pixmap::draw_pixmap(x=0, y=0, transform=placement_to_pixmap_transform(picture ∘ target))`.
- `sampling`: `Nearest` → `FilterQuality::Nearest`, `Linear` → `FilterQuality::Bilinear`.
- `opacity` multiplies source alpha; `1.0` is the fast path (no `PixmapPaint::opacity` set).
- Decode failure is a **compile-time** impossibility (bytes were encoded by us); a corrupt command is skipped with `MediaFailed` rather than panicking.

### 6.4 SVG serialization

`<image>` with `href="data:image/png;base64,…"`, `x="0" y="0" width="1" height="1"`, `preserveAspectRatio="none"`, `transform="matrix(a b c d e f)"` carrying `placement`, and `opacity` when `< 1`. This mirrors `serialize_svg_image`'s data-URI path today.

### 6.5 AST, dumps and goldens

- `dump --json` expands metafiles first (it has the `AssetStore`), then renders `DrawBitmap` as metadata: `{ "type": "drawBitmap", "width": …, "height": …, "pngSha256": … }`. Full base64 bytes are emitted only with the new `dump --embed-bitmaps` flag. `DrawMetafile` never appears in a dump; an expansion failure appears as the placeholder commands plus the warning list.
- L2a AST goldens use the metadata form, so PNG encoder changes do not churn snapshots.
- `PartialEq` on `Arc<[u8]>` compares contents; goldens compare hashes first for speed.

### 6.6 Compatibility

- Adding an enum variant is a breaking change for exhaustive matches; every backend match (`rasterizer/mod.rs`, `svg/mod.rs`, `svg/defs.rs`, dump, selection) must be updated in the same PR.
- [PRD 06](./06-component-interface-contracts.md) §2.1 gains the variant; its invariants (all geometry in EMU, `Arc` immutability) apply.
- No serialization version bump is required for `dump` consumers that ignore unknown command types; the schema note in PRD 05 is updated.

---

## 7. Fidelity Policy (PRD 02d update)

PRD 02d's EMF/WMF row changes from unconditional `PLACEHOLDER` to the matrix in [PRD 13a §2](./13a-metafile-fidelity-and-validation.md). Summary:

| Format | Class | Conditions |
| :--- | :--- | :--- |
| EMF (classic) | **FULL** | Playback completes with no diagnostics beyond font substitution |
| EMF (classic) | **DEGRADED** | Any approximation in §4.8; one warning per tag |
| EMF+ (Only/Dual) | **FULL / DEGRADED** | Same rule; dual fallback emits `metafile_emfplus_fallback` |
| WMF | **FULL / DEGRADED** | Same rule; placeable headers and window mapping included |
| Any | **PLACEHOLDER** | Parse error, resource limit, zero drawable output, or engine unavailable |

**Fallback order** (normative):

1. `svgBlip` / raster fallback relationship, when the picture carries one.
2. Metafile playback (§3).
3. §3 metafile placeholder + `MediaFailed` warning (never a blank region).

The placeholder remains the final state for unsupported/broken metafiles; it is the same dashed `◇ METAFILE` box defined in PRD 02d §3.

---

## 8. Resource Limits, Security & Determinism

### 8.1 Limits

`MetafileLimits` starts from `metafile_core::ResourceLimits::default()` and tightens:

| Limit | Value | Rationale |
| :--- | :--- | :--- |
| `max_input_bytes` | 32 MiB | largest EMF seen in Office decks is ~11 MiB; 32 MiB bounds decompression |
| `max_records` | 200 000 | engine default |
| `max_points_per_record` | 1 000 000 | engine default |
| `max_pixels` | $8192^2$ | bitmap decode |
| `max_dc_depth` | 64 | GDI save/restore nesting |
| `max_objects` | 16 384 | pens/brushes/fonts/palettes |
| `max_diagnostics` | 64 | warning-drawer noise |
| `max_container_depth` | 8 | EMF+ container nesting |
| Per-deck metafile budget | 256 MiB decoded/parsed | prevents a hostile deck from exhausting memory |
| Per-metafile compile budget | 250 ms (soft) | measured and warned; hard limit enforced by records/points |

A limit hit aborts playback for that picture → `PLACEHOLDER` + `ResourceLimitHit`.

### 8.2 Decompression (EMZ/WMZ)

- `.emz`/`.wmz` are gzip-wrapped metafiles. Inflate with a bounded writer (`flate2`, already available transitively via `zip`) capped at `max_input_bytes`; a bomb is a placeholder + warning.
- Header sniffing (`01 00 00 00` + `" EMF"` at offset 40; WMF magic) runs **before** dispatch; mislabelled parts fall back to classification by content.

### 8.3 Robustness

- No `unwrap`/`expect` on metafile input paths; all engine errors map to `MetafileError`.
- Truncated/malformed records are the engine's responsibility; our adapter additionally guards index arithmetic, `dx` length mismatches and empty paths.
- Fuzzing: `fuzz/fuzz_targets/fuzz_metafile.rs` feeds arbitrary bytes to classification + playback with the recorder; the panic rule of [PRD 07](./07-quality-and-testing.md) §3.3 applies.

### 8.4 Determinism

- No time, randomness, environment or locale inputs.
- Iteration order over engine objects is definitional (object handles), not hash-based.
- Content hashing for the cache uses a stable algorithm (§9.1), never `RandomState`.
- Output `VectorCommand` order equals engine draw order; clips/pops are balanced by construction.

---

## 9. Caching & Performance

### 9.1 Compiled-command cache

- Owner: `MetafileResolver` (`metafile/cache.rs`), called by `expand_metafiles` (it owns the `AssetStore` handle for `get_raw_bytes`).
- Key: `(rel_id, blake3(bytes))` — content-addressed, so two relationships that point at identical bytes share an entry.
- Value: `Arc<CompiledMetafile> { commands: Vec<VectorCommand>, diagnostics: Vec<MetafileDiagnostic>, format, bounds }`.
- Budget: 16 MiB of estimated command bytes, LRU eviction; diagnostics are copied into `WarningEvent`s at first use and dropped from the cache.
- Invalidation: deck reload clears the cache ([PRD 02a](./02a-composition-core-and-render-pipeline.md) `InvalidateCause::DeckReload`).
- Thread-safety: expansion runs on pipeline worker threads, so the resolver is `Send + Sync` with a `Mutex` dictionary; a miss is computed outside the lock and inserted with a double-check to avoid duplicate playback.
- `blake3` is added to the workspace dependencies (pinned); CRC32 is explicitly rejected for cache keys (collision would render the wrong picture).

### 9.2 Performance budgets

| Metric | Target |
| :--- | :--- |
| Playback + translate, typical EMF ≤ 1 MiB | p95 ≤ 50 ms |
| Playback + translate, large EMF ≤ 11 MiB | ≤ 250 ms |
| Cache hit | ≤ 1 ms (Arc clone + diagnostics reuse) |
| Repeat picture on another slide | O(1) cache hit |
| Raster/export of a compiled EMF picture | indistinguishable from an equivalent vector shape (no re-parse, no decode) |
| Memory during playback | ≤ 8× input bytes (excluding final display list) |

Measurements follow [PRD 07](./07-quality-and-testing.md) §5 (release profile, pinned toolchain).

### 9.3 Why not rasterize at compile time

- SVG export would lose vector quality and selectable text.
- Every terminal resize/zoom would reuse one fixed-resolution bitmap (blur) or force re-rendering (latency, memory).
- The two-stage architecture ([PRD 00](./00-system-overview.md) §3) guarantees shaping/geometry happen once; metafiles must follow the same rule.

---

## 10. Integration Points

### 10.1 Compiler

**Compiler** (`compile_picture_element`, [`compiler/pictures.rs`](../crates/kitty-pptx-core/src/compiler/pictures.rs)) gains only a classification step:

1. Resolve the effective path (`svg_image_path` first, then `image_path`).
2. If raster or SVG → existing `DrawImage` behaviour (unchanged).
3. If metafile (`emf`, `wmf`, `emz`, `wmz`, or MIME `image/x-emf`, `image/x-wmf`, case-insensitive) → emit `VectorCommand::DrawMetafile { … }` inside the existing silhouette clip and background fill; **do not read bytes**.
4. Unknown/undecodable formats → `compile_metafile_placeholder(pic)` + placeholder warning (shared with [issue #5](https://github.com/ahati/kitty-pptx/issues/5) Phase 1).

**Expansion** (`expand_metafiles`) runs immediately after compilation, where both the scene and the `AssetStore` exist:

| Call site | When |
| :--- | :--- |
| `DefaultRenderPipeline::rasterize_now` (sync path) | after `compiler.compile_in_deck`, before `cache.put_vector` — cached scenes are always expanded |
| `pipeline/worker.rs::compile_and_rasterize` | same position on the worker thread |
| `cli/commands/export.rs` (PNG and SVG paths) | after `compile_in_deck`, before rasterize/serialize |
| `cli/commands/dump.rs` | before AST serialization |
| `tests/golden_support` + metafile harness | after `compile_in_deck` |

Expansion emits every `FidelityDegraded`/`MediaFailed`/`ResourceLimitHit` warning through the compile-stage `warning_sink`, so the warnings drawer behaves exactly as for any other element.

### 10.2 TUI

- Warnings appear in the existing `W` drawer with element labels (`slide N · picture <id> · EMF`).
- The render-failure note ([`ui/mod.rs`](../crates/kitty-pptx-tui/src/app/ui/mod.rs)) is **not** used for per-picture diagnostics; pictures degrade in place.
- Sorter thumbnails inherit the compiled `VectorSlide`; no extra playback per card.

### 10.3 CLI & export

- `export --format svg` emits inline `<image>` data URIs for EMF bitmaps and native paths/text otherwise.
- `export --format png` rasterizes the same commands.
- `dump --json` compiles and expands every slide before serialization; the envelope carries `vectorSlides` with no surviving `DrawMetafile`. `DrawBitmap` is projected as `{width, height, pngSha256, placement, opacity, sampling}`; `--embed-bitmaps` adds `pngBase64`. `info` counts metafile pictures as media.
- `--metafile=<playback|placeholder>` (global flag) and `KITTY_PPTX_METAFILE` override the engine for debugging; `placeholder` reproduces the old behaviour without playback and without diagnostics. The flag wins over the environment variable (implemented in M6).
- The export, view and dump paths all drain compile-stage warnings to stderr (font substitutions, fidelity gaps) in addition to expansion warnings.

### 10.4 Feature flag & rollout

- Cargo feature `metafile` in `kitty-pptx-core`, default **on**.
- With the feature off, the engine module and its `metafile-*` dependencies are not built; every marker becomes the §3 placeholder with no diagnostic (documented degradation).
- Rollout completed in M6: the engine is default-on, the flag exists for build-size/regression bisection, and `cargo check -p kitty-pptx-core --no-default-features --tests` is clean.

### 10.5 Warning plumbing prerequisite (M0)

Metafile diagnostics are only useful if they reach the warnings drawer, and **the warning stream is currently dropped before it gets there**:

- `WorkerPool::new` creates `(warning_sink, _warning_rx)` and drops the receiver (`pipeline/worker.rs`);
- `DefaultRenderPipeline::rasterize_now` compiles with a throwaway channel (`let (tx, _rx) = unbounded()`).

Every `WarningEvent` produced by the compiler today (font substitution, unsupported presets, fidelity) therefore never reaches `WarningsState`; only synthetic render-failure entries do. The metafile PR depends on fixing this, and the fix benefits all elements:

1. `DefaultRenderPipeline` owns a `warning_buffer: Arc<Mutex<Vec<(usize, WarningEvent)>>>` (slide index + event) shared with the worker context and the sync path.
2. Both compile paths send into that sink; the pipeline exposes `drain_warnings() -> Vec<(usize, WarningEvent)>`.
3. `PresentationApp::sync_graphics` (or the frame loop) drains the buffer and calls `state.add_warning(slide, event)`, deduplicated by `WarningsState`.
4. The CLI `export` path drains and prints warnings unless `--quiet`; `dump` includes them in its JSON envelope.
5. A test asserts that a fixture with a substituted font produces exactly one drawer entry through the real pipeline.

Implemented in **M0** (`crates/kitty-pptx-core/tests/warning_plumbing_tests.rs`): the pipeline owns the buffer, flushes it before publishing a cached scene, and `WarningsState` deduplicates entries per `(slide, event)`. The CLI export prints drained warnings to stderr.

---

## 11. Error Handling Matrix

| Condition | Commands emitted | Warning |
| :--- | :--- | :--- |
| Playback succeeds, no diagnostics | display list | none |
| Playback succeeds, approximations | display list | `FidelityDegraded` per §4.8 |
| Engine reports unsupported critical feature | §3 placeholder | `FidelityDegraded { class: Placeholder }` |
| Parse/validation error | §3 placeholder | `MediaFailed { rel_id, reason }` |
| Resource limit hit | §3 placeholder | `ResourceLimitHit` |
| Picture has a raster/SVG fallback | fallback paints (no playback) | none |
| Part missing from archive | §3 placeholder | `MediaFailed` |
| Engine feature disabled | §3 placeholder | none (documented) |
| Marker reached a backend unexpanded | §3 placeholder | `FidelityDegraded { feature: "metafile_unexpanded" }` |
| Picture opacity < 1 | display list (opacity applied to bitmaps only) | `metafile_opacity` |

---

## 12. Milestones

| Milestone | Scope | Exit criteria |
| :--- | :--- | :--- |
| **M0 — Warning plumbing** | Pipeline-owned `WarningEvent` buffer shared by worker + sync compile paths; `drain_warnings()`; TUI/CLI/`dump` consumers (§10.5) | Substituted-font fixture produces exactly one drawer entry through the real pipeline; no throwaway warning channels remain |
| **M1 — Contracts & backends** | `DrawMetafile` marker + `DrawBitmap` in IR; raster + SVG implementations; expansion wired into pipeline, export and dump; `MetafileEngine`/`MetafileSink` traits; feature flag | Synthetic marker expands to `DrawBitmap` in raster, SVG and AST goldens; unexpanded marker degrades; clippy clean |
| **M2 — Classic EMF vectors** | Lines, polylines, polygons, paths, rect/ellipse/arc, pens/brushes, clips, bitmap records | Synthetic EMF corpus renders without placeholders; L2a/L2b goldens pass |
| **M3 — Text** | Fonts, alignment, `dx`, escapement, decorations, opaque background | Text fixture goldens; substituted-font warnings correct |
| **M4 — EMF+** | Only/Dual arbitration, gradients, textures, transforms, regions | EMF+ fixtures pass; dual-fallback diagnostic covered |
| **M5 — WMF & EMZ/WMZ** | Placeable headers, window mapping, bounded gzip | WMF fixtures pass; bomb test degrades |
| **M6 — Default-on hardening** | Warnings drawer, perf budgets, fuzz corpus, docs | PRD 13a §10 checklist complete; feature default on |

### 12.1 Milestone status

| Milestone | Status | Evidence / notes |
| :--- | :--- | :--- |
| M0 | ✅ done (`124a1e4`) | `tests/warning_plumbing_tests.rs` |
| M1 | ✅ done (`5a3cd20`) | marker/bitmap contracts, raster + SVG, expansion wired |
| M2 | ✅ done (`1ff178d`) | `metafile-rs` adapter; real EMF/EMF+/WMF fixtures |
| M3 | ✅ done | `tests/metafile_milestone_tests.rs`: alignment (left/center), `dx` overrides, escapement rotation, decorations, `ETO_OPAQUE` background, size clamping, `FontSubstituted` for substituted faces |
| M4 | ✅ done with engine limits | dual `GetDC` fallback diagnostic, hatch → preset pattern, texture → average solid (DEGRADED), gradient axis from the brush transform. `metafile-emfplus =1.0.1` has no radial/path-gradient vocabulary, so patch `0007` approximates those brushes with a linear ramp + `metafile_gradient`; glyph-index text still degrades with the mapped tag instead of the FULL rows in PRD 13a §2.2 |
| M5 | ✅ done | placeable WMF + `META_EXTTEXTOUT`, `.emz`/`.wmz` bounded inflate (bomb degrades to `ResourceLimitHit`), mislabelled containers classified by content |
| M6 | ✅ code complete | default-on `metafile` feature with feature-off placeholder routing, global `--metafile`/`KITTY_PPTX_METAFILE` override, export compile-warning drain, release perf smoke (`tests/metafile_perf_tests.rs`), deterministic robustness smoke + `fuzz/fuzz_targets/fuzz_metafile.rs`, external golden harness expands metafiles. The committed AST/PNG golden corpus (PRD 13a §2.4/§4) remains the tracked ⚠️ follow-up; every matrix row already has a fixture/assertion in `tests/metafile_*.rs`. |

**Playback fixes landed with M3–M5:**

- EMF+ `DrawString` carries `position` in page coordinates and the
  world/container affine in `TextRun.transform`; the adapter now applies that
  affine to the anchor and the glyph axes, so labels inside transformed
  containers (chart insets, translated callouts) land at their authored spot.
- EMF+ pens that declare distinct start/end caps no longer become unsupported
  objects: patch `0004` keeps the start cap, so frames, spines and tick marks
  that select those pens are stroked instead of being dropped.
- Split EMF+ objects commit when their accumulated bytes reach
  `TotalObjectSize` (patch `0005`), not only when the final fragment clears the
  continuation bit. MS-EMFPLUS 2.3.5.1 makes the total authoritative; producers
  that leave C set on the last fragment otherwise leave the object slot empty,
  so the first `EmfPlusDrawImage`/`DrawImagePoints` aborts the whole picture.

**Engine-capability deltas (normative until the upstream fix lands):**

- `metafile-core 1.0.1` exposes no radial/path-gradient paint. Patch `0007`
  approximates `EmfPlusPathGradientBrush` with a linear ramp (surrounding
  colours → centre) and tags it `metafile_gradient`; radial geometry is not
  reconstructed, but the picture keeps painting instead of falling back to the
  §3 placeholder.
- Texture brushes have no inline-image paint in the display list; they are
  approximated by the bitmap's alpha-weighted average colour and tagged
  `metafile_pattern_brush` (DEGRADED).
- `ETO_GLYPH_INDEX` text cannot be decoded by the engine (no source glyph map);
  the record is skipped and tagged `metafile_glyph_index_text`.

---

## 13. Risks & Mitigations

| Risk | Impact | Mitigation |
| :--- | :--- | :--- |
| `metafile-rs` is young (v1.0.1, single maintainer) | API churn, regressions | Vendor + exact pin; all engine types confined to `engine_metafile_rs.rs`; `MetafileEngine` seam allows swapping to `betteroffice-metafile`; corpus + fuzz gate version bumps |
| EMF+ coverage gaps | Missing shapes/effects in modern files | Diagnostics surface gaps as `DEGRADED`; dual-mode falls back to classic records; corpus tracks per-record coverage ([PRD 13a §2](./13a-metafile-fidelity-and-validation.md)) |
| Font metric drift vs GDI | Text placement differs by a pixel | Reuse the project's shaping stack (same as PPTX text); `FontSubstituted` warnings; goldens tolerate AA differences |
| Huge metafiles (10 MiB+) | Compile stalls | Bounded input/records; per-metafile soft budget; cache by content; placeholder on limit |
| Clip-combine approximation | Ink hidden or over-painted | Conservative bounding-box clips (never outside the picture); exact booleans are a follow-up |
| Golden churn from PNG/font updates | Noisy CI | AST goldens use `pngSha256` metadata; PNG goldens use DSSIM/pixelmatch thresholds, not exact hashes |
| Licensing/attribution drift | Compliance | Apache-2.0 notice preserved; `crates/VENDORED.md` + `crates/patches/` per AGENTS.md |

---

## 14. Open Questions

1. **Exact region booleans** (`OR`/`XOR`/`DIFF`): simple closed rect/polygon operands are exact via compound paths and two-clip subtraction (PRD 13 §4.7); curved/nested/`Complement` operands keep the conservative bbox + warning. `i_overlay` remains a candidate if the corpus shows visible gaps for those.
2. **`DrawBitmap` PNG vs raw RGBA**: PNG chosen for AST/SVG compactness; revisit if raster decode shows up in profiles > 2 ms per picture at 1080p.
3. **`dx` clustering**: strict 1:1 char/dx mapping is required for override; complex-script clusters fall back to shaped advances. Confirm acceptable against the corpus.
4. **EMF `TextOut` with glyph indices** (`ETO_GLYPH_INDEX`): re-shape by text (loses exact glyph choice) or drop the run with a warning? Current plan: re-shape + `metafile_glyph_index_text` warning.
5. **Cache persistence**: in-memory only for v1; a persisted cache is out of scope.
6. **`metafile-svg` differential**: dev-dependency only, never linked into release binaries; confirm CI can build it on the pinned toolchain.

---

## 15. References

- [MS-EMF] — Enhanced Metafile Format, Microsoft Open Specifications.
- [MS-EMFPLUS] — Enhanced Metafile Format Plus Extensions.
- [MS-WMF] — Windows Metafile Format.
- `metafile-rs` repository: <https://github.com/Dantusaikamal/metafile-rs> (record audits, qualification notes, real-world fixtures, fuzz targets).
- [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) — placeholder language and fidelity governance.
- [PRD 06](./06-component-interface-contracts.md) — display-list invariants (`VectorSlide`, `VectorCommand`).
- [PRD 13a](./13a-metafile-fidelity-and-validation.md) — fidelity matrix, diagnostics taxonomy, goldens and NFRs.
- [Issue #5](https://github.com/ahati/kitty-pptx/issues/5) — silent-drop defect and the placeholder fallback this PRD builds on.
