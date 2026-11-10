# PRD 02d: Element Renderers & Fidelity Policy

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies the per-element-type renderers (block B9) and the normative **fidelity policy**: an explicit contract governing what renders fully, what degrades, what renders as a placeholder, and what is omitted. Interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Fidelity Policy Governance

A PowerPoint renderer that silently omits unsupported features produces deceptive output. `kitty-pptx` establishes an honest visual contract:
1. **Zero Silent Degradation**: Every non-FULL element emission fires a typed `WarningEvent` ([PRD 06](./06-component-interface-contracts.md) §4).
2. **Determinism**: Same input model $\to$ bit-identical pixels across runs.
3. **Fail-Soft Isolation**: An element failure draws a labeled placeholder and never invalidates neighboring elements or the slide.

---

## 2. Normative Fidelity Matrix (v1)

### 2.1 Shape Features

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| T1 Preset Geometries | **FULL** | Parametric path generators honoring `adj1..=8` ([PRD 02b App](./02b-appendix-preset-geometries.md) §1). |
| T2 Preset Geometries | **DEGRADED** | Simplified 2-D stand-ins preserving silhouette ([PRD 02b App](./02b-appendix-preset-geometries.md) §2). |
| Unknown / T3 Presets | **PLACEHOLDER** | Standard placeholder (§3) with shape preset label + `WarningEvent::UnsupportedPreset`. |
| Custom Paths (`custGeom`) | **FULL** | Normalized coordinates $[0, 1]$, non-zero winding, `ArcTo` cubic expansion. |
| Solid & Gradient Fills | **FULL** | Linear (0–360°) and radial gradients with sRGB stop interpolation. |
| Blip Fills (texture fill) | **DEGRADED** | Source bitmap tiled/stretched inside shape path. |
| Pattern Fills | **DEGRADED** | Fallback to solid fill in the pattern's primary foreground color. |
| Stroke Caps, Joins, Dashes | **FULL** | Butt/round/square caps, miter/bevel joins, standard OOXML dash styles. |
| Inset Stroke (`algn="in"`) | **FULL** | Internal clipping mask with double-width stroke ([PRD 02b](./02b-vector-style-and-geometry-engine.md) §3.2). |
| Line-end Arrowheads | **DEGRADED** | Fixed-proportion arrowhead markers rendered at endpoints. |
| Shape Effects (shadow, glow, 3-D) | **OMITTED** | Geometry and fill render normally; effects omitted + warning. |
| WordArt Text Warp | **DEGRADED** | Text renders fully without warp envelope distortion. |

### 2.2 Picture & Image Features

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| PNG / JPEG / WebP / BMP / TIFF | **FULL** | Decoded via `image` crate with `SizeHint::AtMost` downscaling. |
| Image Crop (`srcRect`) | **FULL** | Sub-rectangle extraction and stretching math (§4.1). |
| Alpha Transparency | **FULL** | 8-bit straight alpha preserved to the seam. |
| Vector SVG | **FULL** | Bundled PNG fallback relationship preferred; otherwise rasterized via `resvg`. |
| EMF / WMF Metafiles | **PLACEHOLDER** | PNG fallback used if present; else metafile placeholder (§3) + warning. |
| Blip Effect: `alphaModFix` | **DEGRADED** | Modulates alpha by constant factor $A \cdot (\text{amt} / 100,000)$. |
| Blip Effect: `grayscl` | **DEGRADED** | BT.709 luma transform: $Y = 0.2126 R + 0.7152 G + 0.0722 B$. |
| Blip Effect: `biLevel` | **DEGRADED** | Thresholding: if $Y > 0.5$, white; else black. |
| Blip Effect: `duotone` | **DEGRADED** | Linear interpolation between two resolved theme colors based on luma. |

### 2.3 Tables

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| Column / Row Grid | **FULL** | Proportional column distribution from `gridCol` and minimum row heights `tr`. |
| Cell Borders & Fills | **FULL** | Inner/outer borders, solid fills, and alternating banded row styles. |
| Cell Text Formatting | **FULL** | Rich text spans, margins, vertical centering (`anchor="ctr"`). |
| Merged Cells | **FULL** | `gridSpan`, `rowSpan`, `hMerge`, `vMerge` bounds consolidation. |
| Row Auto-Growth | **DEGRADED** | Authored heights respected; overflowing text clips to cell boundary. |

### 2.4 Charts & Miscellaneous

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| All Charts (classic + ChartEx) | **PLACEHOLDER** | Data-rich chart placeholder (§4.3) showing title, series names, counts, swatches. |
| OMML Equations (with fallback) | **FULL** | Renders authoring-time `math_fallback` image preview faithfully. |
| OMML Equations (no fallback) | **PLACEHOLDER** | Standard placeholder with equation label. |
| SmartArt (Diagrams) | **PLACEHOLDER** | Standard placeholder showing carried diagram title. |
| Embedded Video / Audio | **PLACEHOLDER** | Carried poster frame with media glyph; playback triggered via `M` ([PRD 04](./04-interactive-tui-presenter.md) §3). |
| Hyperlinks | **DEGRADED** | Visual underline and presentation hyperlink color; not interactive. |

---

## 3. Normative Placeholder Visual Language

All `PLACEHOLDER`-class elements render through a unified, deterministic routine:

```
┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
╎                                               ╎
╎                   ◇  CHART                    ╎  ← Kind icon + uppercase label
╎             "Q3 Revenue Summary"              ╎  ← Carried title (italic, optional)
╎         ─────────────────────────────         ╎  ← Separator line
╎         ▮ North America (8 points)            ╎  ← Series summary with color swatches
╎         ▮ EMEA (8 points)                     ╎
╎                                               ╎
└╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
```

1. **Frame Geometry**: Dashed 1.5 px border (`dash=[4.0, 3.0]`) inset 2.0 px inside element bounding box. Stroke color is `presentation.default_text_color` at 60% opacity. No fill (background shows through).
2. **Header Area**: Centered glyph icon and uppercase kind label (`CHART`, `DIAGRAM`, `EQUATION`, `METAFILE`, `MEDIA`) at $y = 0.35 \cdot H$, rendered in 13 pt bold font.
3. **Title Area**: If the element carries an authored title, centered at $y = 0.50 \cdot H$ in 11 pt italic font (truncated with `…`).
4. **Summary Area**: Up to 3 summary detail lines starting at $y = 0.65 \cdot H$ in 10 pt regular font.

---

## 4. Element Renderer Implementations

### 4.1 Picture Compositing & Crop Math
When compositing a picture with source dimensions $(W_{\text{src}}, H_{\text{src}})$ and crop attributes $l, t, r, b$ (in $100,000$ths):
1. **Crop Rect Extraction**:
   $$x_0 = W_{\text{src}} \cdot \frac{l}{100,000},\qquad y_0 = H_{\text{src}} \cdot \frac{t}{100,000}$$
   $$w_{\text{crop}} = W_{\text{src}} \cdot \left(1.0 - \frac{l + r}{100,000}\right),\qquad h_{\text{crop}} = H_{\text{src}} \cdot \left(1.0 - \frac{t + b}{100,000}\right)$$
2. **Crop Validation**: If $l + r \ge 100,000$ or $t + b \ge 100,000$, crop is invalid; fallback to uncropped source and emit `WarningEvent`.
3. **Canvas Transform**: Scale cropped sub-image to target bounding box $(w_{\text{px}}, h_{\text{px}})$, apply element rotation and flips, and composite onto pixmap.

### 4.2 Table Layout & Rendering
1. **Grid Resolution**: Column widths are scaled proportionally from `gridCol` EMU values to fill table width. Row heights take the maximum of authored `tr` height and baseline line height.
2. **Cell Merging**: Cells marked with `hMerge` or `vMerge` expand the master cell's bounds across merged columns/rows.
3. **Paint Pass**:
   - Step 1: Cell background fills (honoring alternating banded fills).
   - Step 2: Inner and outer cell borders (mitered joins).
   - Step 3: Text body rendered inside cell margins (`marL`, `marR`, `marT`, `marB`) with vertical alignment applied.

### 4.3 Data-Rich Chart Placeholder
Extracted from `pptx_model::ChartModel`:
1. Humanized chart type: e.g. `barChart` $\to$ `"Bar Chart"`, `pieChart` $\to$ `"Pie Chart"`.
2. Title: Preferred from `title_rich_runs` if styled, else plain `title`.
3. Series summary: Formatted as `▮ <SeriesName> (<N> points)`. Swatch `▮` takes the series theme color if present. If series is empty and `authored_without_series` is true, displays `"Empty chart (authored)"`.
