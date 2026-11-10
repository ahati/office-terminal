# PRD 02b: Vector Style & Geometry Engine

> Part of the PRD 02 split. This document specifies the **style resolution** and **geometry generation** services consumed by the element renderers of [PRD 02d](./02d-element-renderers-and-fidelity-policy.md). Normative interface contracts live in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Scope

| Block | Component | Responsibility |
| :--- | :--- | :--- |
| B6 | **Style Resolver** | Hex color parsing, alpha application, gradient construction, hyperlink/default colors. |
| B7 | **Geometry Engine** | Preset shape generators, custom path (`custGeom`) materialization, stroke/dash configuration. |

Both services are **pure and stateless** (`Send + Sync`): `(input, parameters) → tiny-skia primitives`, with no knowledge of slides, terminals, or caches.

## 2. Color Pipeline — Corrected Division of Labor

> **Change from the previous revision.** The original PRD 02 §4 placed DrawingML theme resolution and modifier math (`lumMod`, `lumOff`, `tint`, `shade`, `satMod`, hue rotation) in the renderer. **Verification against the vendored parser shows this work is already done upstream**: the parser resolves theme slots (`dk1`, `lt1`, `dk2`, `lt2`, `accent1–6`, `hlink`, `folHlink`), applies the ECMA-376 modifier chain in its own color module, and emits **concrete sRGB hex strings** in the model (`Fill::Solid { color: "383838" }`, `Stroke { color: … }`, run colors, etc.).

The Style Resolver therefore has a deliberately small surface:

### 2.1 Responsibilities
1. **Hex parsing** — model colors are 6-digit lowercase hex (`"383838"`); parse to `f32` linear/sRGB components. 8-digit hex (with alpha) tolerated where the parser emits it.
2. **Opacity composition** — element/`<a:alpha>` values multiplied into the paint alpha; opacity from placeholders (`Fill::Solid` with alpha) merged with run-level alpha for text.
3. **Gradient construction** — the model carries `Gradient { stops, angle_deg, kind: linear|radial }` with stops as (position, hex) pairs; the resolver interpolates stop colors in sRGB space and maps the OOXML angle (0° = left→right, 90° = top→bottom) onto the `tiny-skia` gradient transform.
4. **Presentation-level colors** — `default_text_color` and `hlink` color come from the `Presentation` struct (theme-baked by the parser); hyperlink runs without an explicit color take the presentation hyperlink color.
5. **No-op passthrough** — the resolver never re-derives theme slots or modifiers. If a color string is malformed, the resolver falls back to a documented error color (`#FF00FF` magenta) and emits a `WarningEvent`.

### 2.2 Non-Responsibilities (explicitly)
- Theme slot tables, tint/shade/luminance math — **upstream parser**, already applied.
- Palette quantization for Sixel — display driver ([PRD 03](./03-terminal-graphics-protocol.md) §8).
- Terminal blending for half-blocks — display driver.

## 3. Preset Geometries

ECMA-376 defines **187 preset geometries** (`ST_ShapeType`). Implementing all faithfully is a project in itself; this spec makes the coverage explicit and testable.

### 3.1 Coverage Tiers

| Tier | Meaning | Acceptance |
| :--- | :--- | :--- |
| **T1 — Full** | Parametric generator honoring adjustment values (`adj`…`adj8` from the model), exact outline semantics. | Each T1 preset must have a golden corpus entry ([PRD 07](./07-quality-and-testing.md) §4.1). |
| **T2 — Simplified** | Renders a visually approximate stand-in built from T1 primitives (e.g. callouts as rounded rect + triangle, bevel-variants as their base shape). Minor divergence allowed; must preserve bbox and primary silhouette. | Spot-checked; divergence noted in the matrix appendix of this PRD. |
| **T3 — Placeholder** | Unknown/unimplemented preset names render via the placeholder renderer ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3). | Never silent: emits `WarningEvent::UnsupportedPreset`. |

**Default rule**: a preset not listed in the T1/T2 tables below is T3. This rule keeps the matrix honest as the OOXML preset universe grows.

### 3.2 Tier 1 (v1 launch set)

Rectangles & basics: `rect`, `roundRect`, `ellipse`, `triangle`, `diamond`, `parallelogram`, `trapezoid`, `pentagon`, `hexagon`, `octagon`, `pie`, `chord`, `arc`, `donut`, `triangle` variants.
Arrows & callout primitives: `rightArrow`, `leftArrow`, `upArrow`, `downArrow`, `leftRightArrow`, `chevron`, `homePlate`.
Stars: `star5` (`star`), `star6`, `star8`.
Connectors: `line`, `straightConnector1`, math shapes (`mathPlus`, `mathMinus`, `mathMultiply`, `mathEqual`, `mathNotEqual`).
Flowchart core: `flowChartProcess`, `flowChartDecision`, `flowChartTerminator`, `flowChartDocument`, `flowChartMultidocument`, `flowChartInputOutput`, `flowChartPredefinedProcess`, `flowChartConnector`, `flowChartOfflineStorage`, `flowChartMagneticDisk`.

(T1 list is closed for v1; additions require a matrix change per PRD 07 §4.1 governance.)

### 3.3 Tier 2 (simplified stand-ins)
Remaining arrows (double, striped, notched), block arrows (`bentArrow`, `circularArrow`, …), basic callouts (`wedgeRectCallout`, `wedgeEllipseCallout`, `cloudCallout`, …), ribbons, bevel/`frame` variants (bevel effects omitted → base geometry), `can`, `cube`, `foldedCorner`, `smileyFace`, `heart`, `lightningBolt`, `sun`, `moon`, `cloud`, seals, plaques, `diagStripe`, crosses, frames, chords/teardrop/quote variants, remaining flowchart shapes, gear (`gear6`, `gear9`), chart supplement shapes (`chartStar`…), math division. Each maps to a documented stand-in recipe in the matrix appendix.

### 3.4 Adjustment Values
The model carries up to eight `adj` values per shape (OOXML units, 0–100000). T1 generators take the relevant values as parameters (e.g. `roundRect` corner radius = `adj`% of min(w,h), default 16.67%; arrow head/shaft proportions from `adj`/`adj2`). T2 stand-ins may ignore non-primary adjustments.

## 4. Custom Geometry (`custGeom`)

### 4.1 Model Contract (verified against the vendored model)
`geometry == "custGeom"` carries:

```rust
pub cust_geom: Vec<Vec<PathCmd>>,      // outer: one <a:path> per subpath
pub cust_geom_paint: Option<Vec<PathPaint>>, // per-path fill/stroke policy
```

Coordinates are **normalized to [0,1]** within the shape bbox (parser-normalized). `PathCmd` variants as emitted by the parser:

| Variant | Parameters | tiny-skia mapping |
| :--- | :--- | :--- |
| `MoveTo { x, y }` | normalized point | `PathBuilder::move_to` (after bbox scale) |
| `LineTo { x, y }` | normalized point | `line_to` |
| `CubicBezTo { x1, y1, x2, y2, x, y }` | two control points + end | `cubic_to` |
| `QuadBezTo { x1, y1, x, y }` | control + end | `quad_to` |
| `ArcTo { wr, hr, st_ang, sw_ang }` | radii + start/sweep **degrees** | Endpoint-to-center arc conversion → cubic segments (max 45°/segment, tolerance ≤ 0.2 px at target resolution) |
| `Close` | — | `close()` |

*(Note: the earlier revision listed `QuadTo`/`CubicTo` variants — the actual model uses `QuadBezTo`/`CubicBezTo` and additionally carries `ArcTo`.)*

### 4.2 Rules
1. **Subpath independence**: each `<a:path>` may carry its own fill/stroke policy (`PathPaint`: fill = `norm | none`, stroke on/off). Paths paint in document order.
2. **Fill rule**: nonzero winding (DrawingML default). `tiny-skia` fill types map accordingly.
3. **Bbox scaling**: normalized coordinates scale by (w_px, h_px) in the element's local frame; stroke width is **not** scaled by the path scale factor (stroke width comes from `a:ln` in EMU).
4. **Degenerate paths** (empty path list, or all commands outside [0,1] after clamping) → placeholder per PRD 02d §3 + warning.

## 5. Fills & Strokes

### 5.1 Fills (from the model)
- `Solid { color }` → opaque or alpha-modulated `tiny_skia::Paint`.
- `Gradient { stops, angle_deg, kind }` → linear/radial gradient paint (§2.1.3).
- `None` → no fill.
- Additional fill kinds carried by the vendored model (e.g. blip/pattern fills on shapes) are classified in the fidelity matrix ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2) — generally **DEGRADED** (solid fallback using the pattern's foreground color) or placeholder.

### 5.2 Strokes
The model carries `Stroke { color, width_emu, fill, dash_style, dash_segments, … }`:

- **Width**: EMU → px via §2.3 of [PRD 02a](./02a-composition-core-and-render-pipeline.md); minimum rendered width 1 px below ~0.75 pt at target scale (hairline preservation).
- **Dash**: `dash_style` maps the OOXML `prstDash` names (`dash`, `dot`, `dashDot`, `lgDash`, `lgDashDot`, `lgDashDotDot`, `sysDash`, `sysDot`) to `tiny_skia` dash arrays; explicit `dash_segments` (lengths as multiples of stroke width) override when present.
- **Caps/joins**: butt/round/square caps and miter/round/bevel joins map 1:1 onto `tiny_skia` types.
- **Line-end decorations** (start/end arrowheads) as carried by the model: T1 subset (triangle, stealth, arrow, diamond) **DEGRADED** — drawn at decoration bbox with fixed proportions; missing marker types render without decoration + warning.

## 6. API Surface & Determinism

```rust
pub trait GeometryEngine: Send + Sync {
    /// T1/T2 generator lookup. Returns None for T3 (caller renders placeholder).
    fn preset_path(&self, name: &str, w: f32, h: f32, adj: &[Option<f64>])
        -> Option<Vec<tiny_skia::Path>>;
    /// custGeom materialization incl. ArcTo conversion.
    fn custom_paths(&self, paths: &[Vec<PathCmd>], paint: Option<&[PathPaint]>,
                    w: f32, h: f32) -> Vec<tiny_skia::Path>;
}

pub trait StyleResolver: Send + Sync {
    fn solid_paint(&self, hex: &str, alpha: f32) -> tiny_skia::Paint;
    fn gradient_paint(&self, g: &GradientFill, alpha: f32, w: f32, h: f32)
        -> tiny_skia::Paint;
    fn stroke(&self, s: &Stroke, scale: f32) -> tiny_skia::Stroke;
}
```

Both are deterministic pure functions — required by the golden-image regime of [PRD 07](./07-quality-and-testing.md) (identical inputs ⇒ identical bytes).
