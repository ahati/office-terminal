# PRD 02d: Element Renderers & Fidelity Policy

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies how individual slide elements compile into `VectorCommand`s on `VectorSlide` (block B9) and the normative **fidelity policy**. Interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Fidelity Policy Governance

A PowerPoint renderer that silently omits unsupported features produces deceptive output. `kitty-pptx` establishes an honest visual contract:
1. **Zero Silent Degradation**: Every non-FULL element emission fires a typed `WarningEvent` ([PRD 06](./06-component-interface-contracts.md) §4).
2. **Determinism**: Same input model $\to$ identical vector display list across runs.
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
| Inset Stroke (`algn="in"`) | **FULL** | Path clipping mask with double-width stroke ([PRD 02b](./02b-vector-style-and-geometry-engine.md) §3.2). |
| Line-end Arrowheads | **DEGRADED** | Fixed-proportion arrowhead markers rendered at endpoints. |
| Shape Effects (shadow, glow, 3-D) | **OMITTED** | Geometry and fill compile normally; effects omitted + warning. |
| WordArt Text Warp | **DEGRADED** | Text compiles fully without warp envelope distortion. |

### 2.2 Picture & Image Features

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| PNG / JPEG / WebP / BMP / TIFF | **FULL** | Decoded via `image` crate with `SizeHint::AtMost` downscaling. |
| Image Crop (`srcRect`) | **FULL** | Sub-rectangle normalized mapping math (§4.1). |
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
| Cell Borders & Fills | **FULL** | Emits `DrawPath` for cell fills and borders with mitered joins. |
| Cell Text Formatting | **FULL** | Rich text spans compiled into `DrawText` with vertical centering (`anchor="ctr"`). |
| Merged Cells | **FULL** | `gridSpan`, `rowSpan`, `hMerge`, `vMerge` bounds consolidation. |
| Row Auto-Growth | **DEGRADED** | Authored heights respected; overflowing text clips to cell boundary. |

### 2.4 Charts & Miscellaneous

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| All Charts (classic + ChartEx) | **PLACEHOLDER** | Data-rich chart placeholder (§4.3) showing title, series names, counts, swatches. |
| OMML Equations (with fallback) | **FULL** | Renders authoring-time `math_fallback` image preview faithfully. |
| OMML Equations (no fallback) | **PLACEHOLDER** | Standard placeholder with equation label. |
| SmartArt (Diagrams) | **PLACEHOLDER** | Standard placeholder showing carried diagram title. |
| Embedded Video / Audio | **PLACEHOLDER** | Carried poster frame with media glyph; playback triggered via `M` ([PRD 04](./04-interactive-tui-presenter.md) §6). |
| Hyperlinks | **DEGRADED** | Visual underline and presentation hyperlink color; not interactive. |

---

## 3. Normative Placeholder Visual Language

All `PLACEHOLDER`-class elements compile into vector commands:

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

1. **Frame**: Dashed 1.5 px border (`dash=[4.0, 3.0]`) inset 2.0 px inside element bounding box. Stroke color is `presentation.default_text_color` at 60% opacity. No fill.
2. **Header Area**: Centered glyph icon and uppercase kind label (`CHART`, `DIAGRAM`, `EQUATION`, `METAFILE`, `MEDIA`) at $y = 0.35 \cdot H$, rendered in 13 pt bold font.
3. **Title Area**: If carried, centered at $y = 0.50 \cdot H$ in 11 pt italic font.
4. **Summary Area**: Up to 3 summary detail lines starting at $y = 0.65 \cdot H$ in 10 pt regular font.

---

## 4. Element Compilation Details

### 4.1 Picture Compositing & Crop Math
When compiling a picture with source dimensions $(W_{\text{src}}, H_{\text{src}})$ and crop attributes $l, t, r, b$ (in $100,000$ths):
1. **Normalized Crop Rect**:
   $$x_0 = \frac{l}{100,000},\qquad y_0 = \frac{t}{100,000}$$
   $$w_{\text{crop}} = 1.0 - \frac{l + r}{100,000},\qquad h_{\text{crop}} = 1.0 - \frac{t + b}{100,000}$$
2. Emits `VectorCommand::DrawImage { rel_id, src_rect: Rect::new(x0, y0, w_crop, h_crop), dest_rect, opacity, transform }`.

### 4.2 Table Compilation
1. Proportional column distribution from `gridCol` and minimum row heights `tr`.
2. Emits `DrawPath` for cell fills (banded styles) and borders.
3. Emits `DrawText` for cell text blocks with cell insets and vertical centering applied.

### 4.3 Data-Rich Chart Placeholder
Extracted from `pptx_model::ChartModel`:
1. Kind label `CHART` + humanized chart type (e.g. `barChart` $\to$ `"Bar Chart"`).
2. Title from `title_rich_runs` or plain `title`.
3. Series summary lines: `▮ <SeriesName> (<N> points)`. Swatch `▮` takes the series theme color.
