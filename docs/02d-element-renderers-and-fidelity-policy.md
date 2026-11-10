# PRD 02d: Element Renderers & Fidelity Policy

> Part of the PRD 02 split. Specifies the per-element-type renderers (block B9) and — most importantly — the **fidelity policy**: an explicit, testable contract for what renders fully, what renders approximately, what renders as a placeholder, and what is omitted. Interfaces in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose

A .pptx renderer that silently drops features destroys trust; one that honestly labels its limits builds it. Every visual capability of `kitty-pptx` v1 is therefore classified in the matrix below. The matrix is **normative**: any deviation in code or tests is a bug in one of them.

### 1.1 Fidelity Classes

| Class | Definition | User-visible guarantee |
| :--- | :--- | :--- |
| **FULL** | Rendered to spec fidelity; golden-tested against reference renders. | Indistinguishable at presentation distance. |
| **DEGRADED** | Rendered, with documented simplification. | Recognizable; divergence enumerated here. |
| **PLACEHOLDER** | Replaced by the deterministic placeholder presentation (§3). | Communicates *what* is there and *why* it isn't rendered. |
| **OMITTED** | Not rendered at all. | Emits `WarningEvent`; never affects neighboring content. |

### 1.2 Governance
- Matrix changes require matching golden-corpus updates and land in the same change set ([PRD 07](./07-quality-and-testing.md) §4.4).
- Every non-FULL emission fires a structured `WarningEvent` ([PRD 01](./01-parser-and-data-model.md) §7) — "silent degradation" does not exist in this design.

## 2. Fidelity Matrix (v1)

### 2.1 Shapes

| Feature | Class | Notes |
| :--- | :--- | :--- |
| T1 preset geometries ([PRD 02b](./02b-vector-style-and-geometry-engine.md) §3.2) | **FULL** | Parametric, adj-honoring. |
| T2 preset geometries | **DEGRADED** | Simplified stand-ins; recipes in 02b appendix. |
| Unknown presets (T3) | **PLACEHOLDER** | §3 rendering + warning. |
| `custGeom` paths (incl. `ArcTo`) | **FULL** | Per-path paint, winding fill, normalized coords. |
| Solid / gradient fills | **FULL** | Linear & radial (02b §5.1). |
| Blip fill on shapes (textured fill) | **DEGRADED** | Image tiled/stretched inside path; effects on the fill image follow §2.2. |
| Pattern fills | **DEGRADED** | Solid fallback in the pattern's foreground color. |
| Strokes: solid, dashed, gradient-line | **FULL** | Dash styles, caps, joins (02b §5.2). |
| Line-end arrowheads (T1 markers) | **DEGRADED** | Fixed proportions at decoration bbox. |
| Shape effects: shadow, glow, reflection, soft edges | **OMITTED** (v1) | Warning; geometry and fill render normally. |
| 3-D (bevel, extrusion, scene camera) | **OMITTED** (v1) | 2-D projection only. |
| WordArt text warp (`TextWarp`) | **DEGRADED** | Text renders fully, unwrapped from its warp envelope (warp geometry omitted); 40 preset warps listed for post-v1. |

### 2.2 Pictures & Images

| Feature | Class | Notes |
| :--- | :--- | :--- |
| PNG / JPEG / GIF (first frame) / WebP / BMP / TIFF decode | **FULL** | Via `image` crate through the AssetStore ([PRD 06](./06-component-interface-contracts.md) §5). |
| Crop (`srcRect`), stretch, aspect handling | **FULL** | Crop rect math §4.1. |
| Alpha channel | **FULL** | 8-bit alpha preserved to the seam (straight alpha). |
| SVG images | **FULL** | Prefer the PNG fallback relationship when PowerPoint bundled one; else rasterize via `resvg` at target size. |
| EMF / WMF metafiles | **PLACEHOLDER** | No credible pure-Rust decoder; PNG fallback used when bundled, else §3 placeholder. |
| Blip effects: `alphaModFix`, `grayscl`, `duotone`, `biLevel` | **DEGRADED** | Applied post-decode per upstream's effect semantics. |
| Other blip effects (color change, luminance, artistic effects) | **OMITTED** | Warning; base image renders. |

### 2.3 Tables

| Feature | Class | Notes |
| :--- | :--- | :--- |
| Grid layout (`gridCol` widths, `tr` heights) | **FULL** | §4.2 algorithm. |
| Cell fills incl. banded styles | **FULL** | — |
| Cell borders (inner/outer, per-edge) | **FULL** | — |
| Cell text (rich runs, alignment, insets) | **FULL** | Text engine of [PRD 02c](./02c-typography-and-text-layout.md). |
| Merged cells (`gridSpan`, `hMerge`, `vMerge`) | **FULL** | — |
| Row auto-growth beyond authored height | **DEGRADED** | Authored heights respected; overflowing cell text clips (PowerPoint grows rows; we do not re-measure grid growth in v1). |

### 2.4 Charts

| Feature | Class | Notes |
| :--- | :--- | :--- |
| All chart families (classic + ChartEx) | **PLACEHOLDER** | §4.3 chart placeholder — **data-rich**, using the vendored `ChartModel`. |

Chart *rendering* is v1's largest explicitly-deferred subsystem. The model already carries everything the placeholder shows; full chart rendering is a post-v1 track with its own PRD.

### 2.5 Other Elements

| Feature | Class | Notes |
| :--- | :--- | :--- |
| Equations (OMML) with bundled fallback image | **FULL** | The model retains `math_fallback` preview images; rendering the preview is pixel-faithful for authoring-time equations. |
| Equations without fallback | **PLACEHOLDER** | — |
| SmartArt (diagrams) | **PLACEHOLDER** | Frame + title text when carried; data-node diagram otherwise. |
| Video / audio media | **PLACEHOLDER** | Poster frame when bundled; media glyph otherwise. No playback (non-goal, [PRD 00](./00-system-overview.md) §7). |
| Hyperlinks | **DEGRADED** | Rendered as colored/underlined text (presentation `hlink` color); not interactive. |
| Slide background: solid / gradient / image | **FULL** | — |
| Slide transitions & animations | **OMITTED** | Non-goal; static rendering only. |

## 3. Placeholder Renderer (normative visual language)

All PLACEHOLDER-class content renders through **one** deterministic routine, so users learn to read it instantly:

```
┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
│                              │
│         ◇  Chart             │   ← glyph (per kind) + kind label
│   "Revenue by Region, 2024"  │   ← carried title (may be absent)
│   ──────────────────────     │
│   ▮ Series: N. America (6)   │   ← kind-specific summary lines
│   ▮ Series: EMEA (6)         │      (charts: series + point counts)
│                              │
└╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
```

1. **Frame**: dashed 1.5 px stroke in the presentation's `default_text_color` at 60% alpha, inset 2 px inside the element bbox. No fill (background shows through).
2. **Glyph + label**: centered uppercase kind label (`CHART`, `DIAGRAM`, `VIDEO`, `AUDIO`, `EQUATION`, `METAFILE`) in the substituted UI font.
3. **Detail lines**: up to 3 lines of kind-specific content (§4.3 for charts), truncated with `…`.
4. **Determinism**: same model input → identical pixels (golden-tested).
5. Rotation/flip transforms apply to the placeholder like any element.

## 4. Element Renderer Specifications

### 4.1 Picture Compositing
1. Resolve the blip relationship through the `AssetStore` with `SizeHint::AtMost` the on-screen pixel size (never decode a 4000 px source for a 300 px frame; saves the LRU budget).
2. **Crop** (`srcRect`, percentages): source rect `(l, t, r, b)` maps to the destination bbox; cropped region stretches to fill the shape.
3. **No-crop stretch**: scale to bbox (aspect distortion is authored intent — `a:stretch` stretches).
4. Alpha-blend with straight-alpha source over the destination (tiny-skia premultiplied path).
5. Blip effects (§2.2 subset) apply to the decoded bitmap before compositing.
6. Decode failure → AssetError → placeholder + `WarningEvent::MediaFailed`; never aborts the slide.

### 4.2 Table Layout
1. Column widths from `gridCol` EMU values scaled to target; rows minimum `tr` height.
2. Merge pass: `gridSpan`/`hMerge`/`vMerge` consume cells; merged origin carries content.
3. Cell paint order: fill → borders (outer edges of the table use the table-level outer border where defined) → text inside insets (`marL/R/T/B`).
4. Overflowing cell text clips to the cell rect (§2.3 degradation).

### 4.3 Chart Placeholder Composition
From the vendored `ChartModel` (fields verified: `chart_type`, `title`/`title_rich_runs`, `categories`, `series`, `plot_groups`, `show_data_labels`, `authored_without_series`):

1. Frame per §3 with kind label `CHART` + humanized `chart_type` (e.g. `barChart → Bar Chart`).
2. Title: `title_rich_runs` preferred (styled, line-broken as authored), else plain `title`.
3. Summary: series names with per-series value counts (`▮ Series: EMEA (6)`); if `series` is empty and `authored_without_series` is true, render "no series (authored)" — matching PowerPoint's empty-chart presentation rather than an error.
4. Legend colors: series colors from the model when carried (swatch `▮`), else neutral bullets.

## 5. Renderer Interface

```rust
pub trait ElementRenderer: Send + Sync {
    /// Renders one element into the canvas under the given transform.
    /// Failures return a PlaceholderCause and MUST NOT corrupt the canvas.
    fn render(&self, ctx: &RenderContext<'_>, elem: &SlideElement)
        -> Result<(), RenderError>;
}
```

`RenderContext` bundles: target scale, canvas handle, `GeometryEngine`, `StyleResolver`, `TextLayoutEngine`, `AssetStore`, presentation defaults, and the `WarningEvent` sink. Element renderers are stateless and independently unit-testable — a property the golden corpus (PRD 07 §4.1) depends on.
