# PRD 02b: Vector Style & Geometry Engine

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies the **style resolver** (block B6) and **geometry engine** (block B7) consumed by the slide compiler ([PRD 02a](./02a-composition-core-and-render-pipeline.md)). Parametric equations and stand-in recipes are detailed in [PRD 02b Appendix](./02b-appendix-preset-geometries.md). Normative interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Scope

| Block | Component | Responsibility |
| :--- | :--- | :--- |
| **B6** | **Style Resolver** | Hex color parsing, alpha blending, gradient vector construction, stroke dash/cap mapping, inset stroke alignment. |
| **B7** | **Geometry Engine** | Parametric preset shape paths (T1/T2), custom path (`custGeom`) materialization, arc decomposition into `VectorPath`. |

Both services are **stateless and pure functions** (`Send + Sync`): `(input, parameters) -> VectorPath / VectorPaint`. They emit resolution-independent vector primitives consumed by the `VectorSlide` display list ([PRD 06](./06-component-interface-contracts.md) §2.1).

---

## 2. Color & Style Pipeline

The upstream parser pre-resolves DrawingML theme palettes and ECMA-376 modifiers (`lumMod`, `lumOff`, `tint`, `shade`, `satMod`) into concrete sRGB hex strings ([PRD 01](./01-parser-and-data-model.md) §4.2). The Style Resolver therefore operates directly on resolved color values:

### 2.1 Hex Parsing & Alpha Modulation
1. **Hex Formats**: 6-character hex (`"383838"` $\to$ RGB $[0.22, 0.22, 0.22]$) and 8-character hex (`"383838CC"` with embedded alpha).
2. **Opacity Multiplication**: Effective paint alpha is the product of element-level opacity, shape alpha, and run opacity:
   $$\alpha_{\text{eff}} = \alpha_{\text{element}} \cdot \alpha_{\text{fill}} \cdot \alpha_{\text{shape}}$$
3. **Fallback Color**: Malformed hex strings fall back to `#FF00FF` (magenta) and emit `WarningEvent::UnsupportedPreset`.

### 2.2 Gradient Construction
DrawingML linear and radial gradients are mapped onto `VectorPaint`:
- **Linear Gradients**: Angle is defined in degrees clockwise from horizontal ($0^\circ = \text{left}\to\text{right}$, $90^\circ = \text{top}\to\text{bottom}$). The resolver computes start point $P_0$ and end point $P_1$ on the shape bounding box:
  $$x_{\text{mid}} = \frac{w}{2},\quad y_{\text{mid}} = \frac{h}{2},\quad L = \frac{1}{2} \sqrt{w^2 + h^2}$$
  $$P_0 = (x_{\text{mid}} - L \cos\theta,\; y_{\text{mid}} - L \sin\theta),\qquad P_1 = (x_{\text{mid}} + L \cos\theta,\; y_{\text{mid}} + L \sin\theta)$$
- **Stops**: Normalized position $pos \in [0.0, 1.0]$ with sRGB color interpolation.
- **Radial Gradients**: Centered at shape center with radius $R = \frac{1}{2} \max(w, h)$.

---

## 3. Stroke Pipeline & Inset Alignment

### 3.1 Stroke Width & Hairline Preservation
- Stored as EMU width: $w_{\text{emu}}$. Converted to target pixels during rasterization ($w_{\text{px}} = w_{\text{emu}} \cdot s$).
- **Hairline Rule**: If $w_{\text{px}} < 1.0\text{ px}$ and $w_{\text{emu}} > 0$, clamp to minimum $1.0\text{ px}$ during rasterization to prevent hairline dropouts on low-DPI displays.

### 3.2 Stroke Alignment (`algn`)
DrawingML supports center-aligned (`algn="ctr"`, default) and inset-aligned (`algn="in"`) strokes:
- Center strokes apply directly to `VectorPath`.
- For `algn="in"`: The engine wraps the stroked path with a clipping boundary of the filled shape path. The outer half of the stroke is clipped away, leaving the stroke inset.

### 3.3 Dash Styles & Caps
OOXML `prstDash` values map to `VectorStroke` dash patterns (in stroke-width multipliers):

| `prstDash` Name | Dash Pattern (on, off) | Visual Appearance |
| :--- | :--- | :--- |
| `solid` | `[]` | Continuous line |
| `dot` | `[1.0, 3.0]` | Round or square dots |
| `dash` | `[4.0, 3.0]` | Standard dashes |
| `lgDash` | `[8.0, 3.0]` | Long dashes |
| `dashDot` | `[4.0, 3.0, 1.0, 3.0]` | Dash-dot alternating |
| `lgDashDot` | `[8.0, 3.0, 1.0, 3.0]` | Long dash-dot |
| `lgDashDotDot` | `[8.0, 3.0, 1.0, 3.0, 1.0, 3.0]` | Long dash-dot-dot |
| `sysDash` | `[2.0, 2.0]` | Short system dashes |
| `sysDot` | `[1.0, 1.0]` | Dense system dots |

Line caps (`butt`, `round`, `square`) and joins (`miter`, `round`, `bevel`) map directly to `VectorStroke` fields.

---

## 4. Custom Geometry (`custGeom`) & Arc Conversion

When `geometry == "custGeom"`, coordinates are normalized to $[0.0, 1.0]$ within the shape bounding box $(w_{\text{emu}}, h_{\text{emu}})$.

### 4.1 Path Command Mapping to `VectorPath`

| Model Variant | Normalized Mapping | `VectorPath` Target Segment |
| :--- | :--- | :--- |
| `MoveTo { x, y }` | $(x \cdot w_{\text{emu}},\; y \cdot h_{\text{emu}})$ | `PathSegment::MoveTo { x, y }` |
| `LineTo { x, y }` | $(x \cdot w_{\text{emu}},\; y \cdot h_{\text{emu}})$ | `PathSegment::LineTo { x, y }` |
| `CubicBezTo { .. }` | Scale control & end points | `PathSegment::CubicTo { x1, y1, x2, y2, x, y }` |
| `QuadBezTo { .. }` | Scale control & end points | `PathSegment::QuadTo { x1, y1, x, y }` |
| `ArcTo { wr, hr, st_ang, sw_ang }` | Semi-axes & angles $\to$ cubic segments | Decomposed into cubic Beziers (§4.2) |
| `Close` | None | `PathSegment::Close` |

### 4.2 OOXML Arc Decomposition Algorithm
OOXML `ArcTo` specifies semi-axes $(w_r, h_r)$ and start/sweep angles $(\theta_{\text{start}}, \theta_{\text{sweep}})$ in degrees.
1. Divide $\theta_{\text{sweep}}$ into sub-arcs of at most $45^\circ$ ($\Delta \theta \le \frac{\pi}{4}$).
2. For each sub-arc from $\alpha$ to $\beta = \alpha + \Delta \theta$:
   $$k = \frac{4}{3} \tan\left(\frac{\Delta \theta}{4}\right)$$
   Control points on the unit circle:
   $$C_1 = (\cos\alpha - k \sin\alpha,\; \sin\alpha + k \cos\alpha),\qquad C_2 = (\cos\beta + k \sin\beta,\; \sin\beta - k \cos\beta)$$
   End point:
   $$P_{\text{end}} = (\cos\beta,\; \sin\beta)$$
3. Scale coordinates by $(w_r \cdot w_{\text{emu}},\, h_r \cdot h_{\text{emu}})$ and translate to the arc center.
4. Append `PathSegment::CubicTo`.

---

## 5. Preset Geometries & Tier Classification

- **Tier 1 (FULL)**: Core shapes with exact parametric equations honoring adjustment values `adj1..=8` ([PRD 02b Appendix](./02b-appendix-preset-geometries.md) §1).
- **Tier 2 (DEGRADED)**: Complex presets rendered using documented simplified geometric stand-ins ([PRD 02b Appendix](./02b-appendix-preset-geometries.md) §2).
- **Tier 3 (PLACEHOLDER)**: Unimplemented or rare shapes rendered via the standard placeholder renderer ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3). Silent fallback is prohibited: every T3 shape emits `WarningEvent::UnsupportedPreset`.

---

## 6. Service Interfaces

```rust
pub trait GeometryEngine: Send + Sync {
    fn preset_path(&self, name: &str, w: f32, h: f32, adj: &[Option<f64>]) -> Option<VectorPath>;
    fn custom_paths(&self, paths: &[Vec<PathCmd>], paint: Option<&[PathPaint]>, w: f32, h: f32) -> Vec<VectorPath>;
}

pub trait StyleResolver: Send + Sync {
    fn solid_paint(&self, hex: &str, alpha: f32) -> VectorPaint;
    fn gradient_paint(&self, g: &GradientFill, alpha: f32, w: f32, h: f32) -> VectorPaint;
    fn stroke(&self, s: &Stroke, scale: f32) -> (VectorStroke, StrokeAlignment);
}
```
All implementations are deterministic pure functions: identical inputs produce identical `VectorPath` and `VectorPaint` data, guaranteeing cross-backend consistency.
