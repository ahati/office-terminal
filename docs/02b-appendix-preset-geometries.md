# PRD 02b Appendix: Preset Geometry Formulas & Stand-In Recipes

> Normative companion to [PRD 02b](./02b-vector-style-and-geometry-engine.md). Specifies the exact parametric equations, adjustment defaults, and geometric constructions for Tier 1 (FULL) and Tier 2 (DEGRADED) preset shapes.

---

## 1. Tier 1 (FULL) Parametric Formulations

All coordinates are constructed in the element's local pixel space $(w, h)$, where $w, h > 0$. Adjustment parameters `adj1` through `adj8` are normalized from OOXML units ($0\dots100,000$) to the interval $[0.0, 1.0]$:
$$a_k = \frac{\text{adj}_k}{100000.0}$$

### 1.1 Rectangles & Basic Polygons
- **`rect`**:
  Points: $(0,0) \to (w,0) \to (w,h) \to (0,h) \to \text{Close}$.
- **`roundRect`**:
  Default $a_1 = 0.16667$ ($\approx 16.67\%$).
  Corner radius: $r = \min(w, h) \cdot \text{clamp}(a_1, 0.0, 0.5)$.
  Construct with four line segments and four $90^\circ$ circular arc corners.
- **`ellipse`**:
  Semi-axes: $r_x = w/2,\, r_y = h/2$, center $(r_x, r_y)$. Decomposed into four $90^\circ$ cubic Bezier segments.
- **`triangle`** (`isoscelesTriangle`):
  Default $a_1 = 0.5$ (apex horizontal position).
  Points: $(w \cdot a_1, 0) \to (w, h) \to (0, h) \to \text{Close}$.
- **`diamond`**:
  Points: $(w/2, 0) \to (w, h/2) \to (w/2, h) \to (0, h/2) \to \text{Close}$.
- **`parallelogram`**:
  Default $a_1 = 0.25$. Offset $dx = w \cdot \text{clamp}(a_1, 0.0, 1.0)$.
  Points: $(dx, 0) \to (w, 0) \to (w - dx, h) \to (0, h) \to \text{Close}$.
- **`trapezoid`**:
  Default $a_1 = 0.25$. Inset $dx = w \cdot \text{clamp}(a_1, 0.0, 0.5)$.
  Points: $(dx, 0) \to (w - dx, 0) \to (w, h) \to (0, h) \to \text{Close}$.
- **`pentagon`**:
  Regular polygon vertices inscribed in bounding box with top vertex centered at $(w/2, 0)$.
- **`hexagon`**:
  Default $a_1 = 0.25$. Inset $dx = w \cdot a_1$.
  Points: $(dx, 0) \to (w - dx, 0) \to (w, h/2) \to (w - dx, h) \to (dx, h) \to (0, h/2) \to \text{Close}$.
- **`octagon`**:
  Default $a_1 = 0.29289$. Inset $d = \min(w, h) \cdot a_1$.
  Points: $(d, 0) \to (w - d, 0) \to (w, d) \to (w, h - d) \to (w - d, h) \to (d, h) \to (0, h - d) \to (0, d) \to \text{Close}$.

### 1.2 Radial & Curved Shapes
- **`donut`**:
  Default $a_1 = 0.25$ (inner radius ratio).
  Outer ellipse $(w/2, h/2)$ plus inner ellipse $(w/2 \cdot (1 - a_1), h/2 \cdot (1 - a_1))$ with even-odd / nonzero winding fill.
- **`pie`**:
  Angles: $a_1$ (start angle, deg), $a_2$ (end angle, deg).
  Points: Center $(w/2, h/2) \to$ Arc to start point $\to$ Arc along sweep $\to \text{Close}$.
- **`chord`**:
  Same as `pie`, but connecting start and end points directly with a straight line instead of center vertex.
- **`arc`**:
  Open stroke path tracing elliptical arc from start to sweep angle.

### 1.3 Arrows & Directional Shapes
- **`rightArrow`**:
  Default $a_1 = 0.5$ (head length ratio), $a_2 = 0.5$ (shaft height ratio).
  Let $y_1 = h \cdot (1 - a_2) / 2$, $y_2 = h \cdot (1 + a_2) / 2$, $x_{\text{head}} = w \cdot (1 - a_1)$.
  Points: $(0, y_1) \to (x_{\text{head}}, y_1) \to (x_{\text{head}}, 0) \to (w, h/2) \to (x_{\text{head}}, h) \to (x_{\text{head}}, y_2) \to (0, y_2) \to \text{Close}$.
- **`leftArrow`**:
  Mirrored $x$-coordinates of `rightArrow`.
- **`upArrow`**, **`downArrow`**:
  Rotated $90^\circ$ parametric equivalents.
- **`leftRightArrow`**:
  Shaft centered horizontally with symmetric arrowheads at both ends.
- **`chevron`**:
  Default $a_1 = 0.5$. Notch depth $dx = w \cdot a_1$.
  Points: $(0, 0) \to (w - dx, 0) \to (w, h/2) \to (w - dx, h) \to (0, h) \to (dx, h/2) \to \text{Close}$.
- **`homePlate`**:
  Points: $(0, 0) \to (w - h/2, 0) \to (w, h/2) \to (w - h/2, h) \to (0, h) \to \text{Close}$.

### 1.4 Stars & Math Shapes
- **`star5`**:
  Default $a_1 = 0.38197$ (inner radius ratio). 5 outer vertices at distance $R = \min(w, h)/2$, 5 inner vertices at $r = R \cdot a_1$, center $(w/2, h/2)$.
- **`star6`**, **`star8`**:
  6 and 8-pointed regular star polygons with parametric inner radius $a_1$.
- **`mathPlus`**:
  Default $a_1 = 0.25$ (bar thickness ratio). Symmetric cross centered in $(w, h)$.
- **`mathMinus`**:
  Horizontal centered bar of height $h \cdot a_1$.
- **`mathMultiply`**:
  Rotated $45^\circ$ centered cross.
- **`mathEqual`**, **`mathNotEqual`**:
  Two parallel horizontal bars with optional diagonal slash bar for `mathNotEqual`.

### 1.5 Flowchart Primitives
- **`flowChartProcess`**: Standard `rect`.
- **`flowChartDecision`**: Standard `diamond`.
- **`flowChartTerminator`**: `roundRect` with radius $r = \min(w, h)/2$ (stadium / pill shape).
- **`flowChartDocument`**: Bottom edge replaced with sinusoidal wave: $y(x) = h - \sin(\frac{2\pi x}{w}) \cdot (0.1 h)$.
- **`flowChartInputOutput`**: Standard `parallelogram`.
- **`flowChartPredefinedProcess`**: `rect` with two internal vertical rules at $x = 0.1 w$ and $x = 0.9 w$.

---

## 2. Tier 2 (DEGRADED) Stand-In Recipes

Tier 2 shapes represent complex 3-D, callout, or decorative presets that render recognizable 2-D approximations preserving the bounding box and primary silhouette:

| Shape Name | Stand-In Construction Recipe |
| :--- | :--- |
| `cube` | Front rectangle $[0, 0.2 h, 0.8 w, h]$, top parallelogram $[0\dots0.8w] \to [0.2w\dots w]$, right side parallelogram. |
| `can` (cylinder) | Top ellipse $(0, 0, w, 0.3 h)$, bottom ellipse arc, vertical connecting lines. |
| `foldedCorner` | Rectangle with top-right corner truncated; small folded triangle at $[w - 0.2w, 0]$ to $[w, 0.2h]$. |
| `heart` | Two touching circles on top, Bezier curves converging to bottom center vertex $(w/2, h)$. |
| `smileyFace` | Outer ellipse + two circular eyes + bottom circular arc smile with round caps. |
| `lightningBolt` | 7-point zigzag polygon: top-left $(0.3w, 0) \to (0.7w, 0) \to (0.4w, 0.45h) \to (0.65w, 0.45h) \to (0.2w, h) \to (0.35w, 0.55h) \to (0.1w, 0.55h) \to \text{Close}$. |
| `sun` | Central circle with radius $0.3 \min(w,h)$ plus 8 radial triangular rays. |
| `moon` | Outer circular arc from top to bottom, subtracted inner circular arc forming crescent. |
| `cloud` | Composite path of 5 overlapping circular arcs bounding a flat bottom. |
| `wedgeRectCallout` | `roundRect` with a triangular callout pointer projecting from bottom edge. |
| `wedgeEllipseCallout` | `ellipse` with a triangular pointer projecting toward target position. |
| `ribbon`, `ribbon2` | Central horizontal banner rectangle with notched folding tails on left and right. |
| `seal12`, `seal16`, `seal24`, `seal32` | Multi-pointed star polygons ($N=12, 16, 24, 32$) with inner radius $a_1 = 0.85$. |

---

## 3. Tier 3 (PLACEHOLDER) Fallback Semantics

Any preset name not present in §1 or §2:
1. Skips geometry path generation (returns `None`).
2. Emits `WarningEvent::UnsupportedPreset { name }`.
3. Dispatches directly to the standard placeholder renderer ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §3) with header label `SHAPE` and subtitle `"Preset: <name>"`.
