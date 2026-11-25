# PRD 08: Shape Effects & 3D Engine

> Part of the Core Render Engine specifications. Specifies DrawingML `<a:effectLst>` (drop shadow, inner shadow, glow, soft edges, reflection) and `<a:scene3d>` / `<a:sp3d>` (camera projection, extrusion depth, bevel profile curves, and lighting rigs) compilation into the resolution-independent `VectorSlide` IR, `tiny-skia` multi-pass rasterization, and standalone vector SVG filter serialization. Normative interface signatures align with [PRD 06](./06-component-interface-contracts.md).

---

## 1. Architectural Role & Two-Stage Pipeline Seam

Historically, shape styling in terminal presentation tools either omitted effects entirely or collapsed them into unaccelerated pixel shaders. In **Office Terminal**, shape effects and 3D scenes are first-class visual constructs governed by the strict **two-stage architecture**:

1. **Stage 1: Compilation (`SlideCompiler`)**: Compiles DrawingML `<a:effectLst>`, `<a:sp3d>`, and `<a:scene3d>` trees into a typed, resolution-independent intermediate representation (`ShapeEffects`) attached to `VectorCommand::DrawPath` and `VectorCommand::DrawImage` on `VectorSlide`.
2. **Stage 2: Target Rasterization & Serialization**:
   - **`tiny-skia` Rasterizer Backend**: Executes multi-pass GPU-efficient software image filtering (separable 3-box Gaussian convolutions, alpha mask clipping, Blinn-Phong normal shading, and SIMD composite blending).
   - **`SlideSvgSerializer` Backend**: Emits declarative, zero-rasterization SVG `<filter>` graphs (`feGaussianBlur`, `feOffset`, `feColorMatrix`, `feBlend`) and SVG masks preserving full resolution independence.

```
┌────────────────────────────────────────────────────────────────────────┐
│ Stage 1: SlideCompiler (Block B10)                                     │
│ DrawingML <a:effectLst>, <a:sp3d>, <a:scene3d>                         │
│ - Resolves shadow angles, distances, glow radii, reflection ramps     │
│ - Projects 3D camera homography & computes bevel profile splines       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Emits VectorSlide with ShapeEffects
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ VectorSlide Intermediate Representation (IR)                           │
│ - VectorCommand::DrawPath { path, fill, stroke, effects, transform }   │
│ - VectorCommand::DrawImage { rel_id, ..., effects, transform }         │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
           ┌────────────────────────┴────────────────────────┐
           ▼                                                 ▼
┌──────────────────────────────────────┐  ┌──────────────────────────────────────┐
│ tiny-skia Multi-Pass Rasterizer      │  │ SlideSvgSerializer (PRD 05)          │
│ - Separable O(1) box blur convolution│  │ - Standalone <filter> definitions    │
│ - Alpha mask inversion & clip (inner)│  │ - feGaussianBlur, feOffset, feBlend  │
│ - Blinn-Phong surface normal shader  │  │ - High-fidelity vector gradients     │
│ - Memory-pooled scratch pixmaps      │  │ - Zero rasterization overhead        │
└──────────────────┬───────────────────┘  └──────────────────┬───────────────────┘
                   ▼                                         ▼
         RgbaSlide (Terminal & PNG)                      slide.svg
```

### 1.1 The DrawingML Composite Order Invariant
DrawingML dictates a deterministic evaluation order for combined effects. When multiple effects are active on a single geometry, the engine applies them in the following strict bottom-to-top sequence:

$$\text{Reflection} \;\longrightarrow\; \text{Outer Shadow} \;\longrightarrow\; \text{Glow Halo} \;\longrightarrow\; \text{3D Extrusion/Bevel} \;\longrightarrow\; \text{Fill/Stroke} \;\longrightarrow\; \text{Inner Shadow} \;\longrightarrow\; \text{Soft Edges}$$

---

## 2. Vector Intermediate Representation (`ShapeEffects`)

`ShapeEffects` is defined in `office-terminal-core::contracts::effects` and embedded into `VectorCommand`:

```rust
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeEffects {
    pub outer_shadow: Option<VectorOuterShadow>,
    pub inner_shadow: Option<VectorInnerShadow>,
    pub glow: Option<VectorGlow>,
    pub soft_edge: Option<VectorSoftEdge>,
    pub reflection: Option<VectorReflection>,
    pub scene_3d: Option<VectorScene3d>,
    pub shape_3d: Option<VectorShape3d>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorOuterShadow {
    pub color: [u8; 4],         // Straight RGBA8
    pub blur_radius_emu: i64,   // Blur diameter in EMU
    pub distance_emu: i64,      // Offset distance in EMU
    pub direction_deg: f64,     // Angle clockwise from positive X-axis (degrees)
    pub scale_x: f32,           // Horizontal scale factor (default 1.0)
    pub scale_y: f32,           // Vertical scale factor (default 1.0)
    pub skew_x_deg: f32,        // Horizontal skew angle (degrees)
    pub skew_y_deg: f32,        // Vertical skew angle (degrees)
    pub rotate_with_shape: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorInnerShadow {
    pub color: [u8; 4],         // Straight RGBA8
    pub blur_radius_emu: i64,   // Inset blur radius in EMU
    pub distance_emu: i64,      // Inset distance in EMU
    pub direction_deg: f64,     // Angle clockwise from positive X-axis
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorGlow {
    pub color: [u8; 4],         // Straight RGBA8
    pub radius_emu: i64,        // Outer glow halo radius in EMU
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorSoftEdge {
    pub radius_emu: i64,        // Edge feathering / erosion radius in EMU
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorReflection {
    pub blur_radius_emu: i64,   // Reflection blur radius in EMU
    pub distance_emu: i64,      // Gap between shape bottom and reflection
    pub direction_deg: f64,     // Reflection angle (default 90.0 = downwards)
    pub start_alpha: f32,       // Opacity at shape boundary (0.0..=1.0)
    pub end_alpha: f32,         // Opacity at far end (0.0..=1.0)
    pub start_pos: f32,         // Ramp start position (0.0..=1.0)
    pub end_pos: f32,           // Ramp end position (0.0..=1.0)
    pub scale_y: f32,           // Typically -1.0 for vertical mirror
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorScene3d {
    pub camera_type: CameraPreset,
    pub field_of_view_deg: Option<f64>,
    pub zoom: f32,
    pub rotation: VectorRot3d,
    pub light_rig: VectorLightRig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorShape3d {
    pub z_emu: i64,             // Front face depth offset
    pub extrusion_emu: i64,     // Extrusion depth
    pub extrusion_color: Option<[u8; 4]>,
    pub contour_width_emu: i64,
    pub contour_color: Option<[u8; 4]>,
    pub material: MaterialPreset,
    pub bevel_top: Option<VectorBevel>,
    pub bevel_bottom: Option<VectorBevel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorBevel {
    pub width_emu: i64,
    pub height_emu: i64,
    pub preset: BevelPreset,    // Circle, Relaxed, Convex, Slant, Riblet
}
```

---

## 3. Comprehensive Effects Taxonomy & Mathematical Formulas

### 3.1 Outer Drop Shadow (`outerShdw`)
The outer shadow projects a blurred, offset, scaled, and skewed silhouette behind the source element.

1. **Pixel Space Conversion**:
   Given target viewport uniform scale $s = \min(W_{\text{px}} / W_{\text{emu}},\, H_{\text{px}} / H_{\text{emu}})$:
   $$d_{\text{px}} = d_{\text{emu}} \cdot s,\qquad r_{\text{px}} = r_{\text{emu}} \cdot s$$
2. **Offset Vector**:
   With direction angle $\theta$ (ECMA-376 default $0^\circ = \text{East}$, clockwise):
   $$\Delta x = d_{\text{px}} \cdot \cos(\theta),\qquad \Delta y = d_{\text{px}} \cdot \sin(\theta)$$
3. **Gaussian Standard Deviation ($\sigma$)**:
   DrawingML blur radius represents total blur spread diameter:
   $$\sigma = \frac{r_{\text{px}}}{2.0}$$
4. **Shadow Transformation Matrix**:
   Applying scale $(s_x, s_y)$ and skew $(k_x, k_y)$:
   $$M_{\text{shdw}} = \begin{bmatrix} s_x & \tan(k_x) & \Delta x \\ \tan(k_y) & s_y & \Delta y \\ 0 & 0 & 1 \end{bmatrix}$$

### 3.2 Inner Shadow (`innerShdw`)
The inner shadow renders an inset, recessed shadow inside the perimeter of the shape.
1. **Mask Inversion & Offset**:
   Let $A_{\text{shape}}(x, y) \in [0, 1]$ be the rasterized alpha coverage of the shape. An inverted mask represents the surrounding void:
   $$A_{\text{inv}}(x, y) = 1.0 - A_{\text{shape}}(x, y)$$
2. **Offset Convolution**:
   Shift $A_{\text{inv}}$ by $(\Delta x, \Delta y)$ and convolve with Gaussian kernel $G_\sigma$:
   $$B_{\text{inner}}(x, y) = \left( G_\sigma * A_{\text{inv}}\right)(x - \Delta x,\, y - \Delta y)$$
3. **Interior Clipping**:
   The effective inner shadow alpha is strictly masked by the shape interior:
   $$A_{\text{final}}(x, y) = B_{\text{inner}}(x, y) \cdot A_{\text{shape}}(x, y)$$
4. Tint with shadow color $C$ and alpha-composite over the shape fill using `BlendMode::Multiply` or `BlendMode::SourceOver`.

### 3.3 Glow Halo (`glow`)
A glow effect radiates an omnidirectional, colored aura outwards from the geometry boundaries.
1. **Expansion Profile**:
   Effective radius $R_{\text{glow}} = r_{\text{emu}} \cdot s$.
   Gaussian standard deviation: $\sigma_{\text{glow}} = R_{\text{glow}} / 2.5$.
2. **Non-Linear Falloff**:
   To match the perceptual density of Office glow halos, the blurred alpha $A_{\text{blur}}$ undergoes a smooth non-linear expansion:
   $$A_{\text{glow}}(x, y) = \alpha_{\text{glow}} \cdot \left( 1.0 - \left(1.0 - A_{\text{blur}}(x, y)\right)^2 \right)$$
3. **Underlay Compositing**:
   The glow mask is tinted with glow RGBA and composited behind the shape geometry (`BlendMode::SourceOver`).

### 3.4 Soft Edges (`softEdge`)
Soft edges produce progressive feathered transparency inwards from the shape perimeter without expanding total bounding bounds.
1. **Gaussian Convolution**:
   Convolve shape alpha mask $A_{\text{shape}}$ with kernel standard deviation $\sigma_{\text{soft}} = (r_{\text{emu}} \cdot s) / 2.0$.
2. **Inset Erosion Remap**:
   To prevent perimeter expansion and ensure the outermost boundary reaches $0.0$ alpha:
   $$A_{\text{soft}}(x, y) = \text{clamp}\left( \frac{(G_{\sigma_{\text{soft}}} * A_{\text{shape}})(x, y) - A_{\text{thresh}}}{1.0 - A_{\text{thresh}}},\; 0.0,\; 1.0 \right)$$
   where threshold $A_{\text{thresh}} = 0.5 \cdot \text{erfc}\left( \frac{1}{\sqrt{2}} \right) \approx 0.1587$.

### 3.5 Mirrored Reflection (`reflection`)
The reflection clones the rendered element, mirrors it vertically, applies an optional blur, and attenuates opacity along a linear vertical falloff ramp.
1. **Mirror Transformation**:
   Let the element bounding box bottom be $y_{\text{bot}}$. The reflection origin begins at $y_{\text{refl\_top}} = y_{\text{bot}} + d_{\text{px}}$.
   $$y' = y_{\text{refl\_top}} + (y_{\text{bot}} - y)$$
2. **Linear Opacity Ramp**:
   For vertical span $H_{\text{refl}} = H_{\text{elem}} \cdot |s_y|$:
   $$t(y) = \text{clamp}\left( \frac{y - y_{\text{refl\_top}}}{H_{\text{refl}}},\; 0.0,\; 1.0 \right)$$
   $$\alpha_{\text{ramp}}(t) = \begin{cases}
   \alpha_{\text{st}}, & t \le pos_{\text{st}} \\
   \alpha_{\text{st}} + (t - pos_{\text{st}}) \frac{\alpha_{\text{end}} - \alpha_{\text{st}}}{pos_{\text{end}} - pos_{\text{st}}}, & pos_{\text{st}} < t < pos_{\text{end}} \\
   \alpha_{\text{end}}, & t \ge pos_{\text{end}}
   \end{cases}$$
3. Convolve with separable Gaussian blur ($r_{\text{px}}$) and multiply source alpha by $\alpha_{\text{ramp}}(t)$.

### 3.6 3D Scene, Camera & Bevel Engine (`sp3d`, `scene3d`)

```
      Light Vector L
           \
            \     ^ Normal N
             \    |
              v   |
          ┌───────┴───────┐   <-- Front Face (z = z_front)
         / Bevel Profile   \
        /───────────────────\
        │                   │
        │ Extrusion (depth) │
        │                   │
        └───────────────────┘
```

#### 3.6.1 Camera Projections & Homography
- **Orthographic Presets** (`orthographicFront`, `isometric*`):
  Projects $(X, Y, Z)$ via affine rotation without depth foreshortening:
  $$x_s = X \cos\phi + Z \sin\phi,\qquad y_s = Y \cos\theta - (X \sin\phi - Z \cos\phi)\sin\theta$$
- **Perspective Presets** (`perspectiveRelaxed`, `perspectiveContrasting`):
  FOV $\alpha_{\text{fov}} \in [20^\circ, 120^\circ]$, camera distance $d_{\text{cam}} = \frac{W_{\text{slide}}}{2 \tan(\alpha_{\text{fov}} / 2)}$:
  $$x_s = \frac{X \cdot d_{\text{cam}}}{Z + d_{\text{cam}}} + c_x,\qquad y_s = \frac{Y \cdot d_{\text{cam}}}{Z + d_{\text{cam}}} + c_y$$

#### 3.6.2 Parametric Bevel Curves
For normalized boundary distance $u \in [0.0, 1.0]$ ($0.0$ at outer edge, $1.0$ at inner bevel crest) and bevel dimensions $(w_b, h_b)$:

| Preset | Curve Equation $z = f(u)$ | Surface Tangent Slope $f'(u)$ |
| :--- | :--- | :--- |
| **Circle** | $h_b \sqrt{1.0 - (1.0 - u)^2}$ | $\frac{h_b (1.0 - u)}{\sqrt{1.0 - (1.0 - u)^2}}$ |
| **Relaxed** | $h_b \cdot \frac{1}{2}\left(1.0 - \cos(\pi u)\right)$ | $\frac{\pi h_b}{2} \sin(\pi u)$ |
| **Convex** | $h_b \sin\left(\frac{\pi u}{2}\right)$ | $\frac{\pi h_b}{2} \cos\left(\frac{\pi u}{2}\right)$ |
| **Slant** | $h_b \cdot u$ | $h_b$ |
| **Riblet** | $h_b \cdot \left( u^3 (10 - 15u + 6u^2) \right)$ | $30 h_b u^2 (1 - u)^2$ |

#### 3.6.3 Surface Normal Field & Blinn-Phong Shading
1. **Normal Construction**:
   Let distance-to-edge gradient be $\nabla d = \left(\frac{\partial d}{\partial x}, \frac{\partial d}{\partial y}\right)$.
   The unnormalized 3D normal vector on the bevel profile is:
   $$\vec{N}_{\text{raw}} = \left( -f'(u) \frac{\partial d}{\partial x},\; -f'(u) \frac{\partial d}{\partial y},\; 1.0 \right),\qquad \vec{N} = \frac{\vec{N}_{\text{raw}}}{\|\vec{N}_{\text{raw}}\|}$$
2. **Light Rig**:
   Unit light vector $\vec{L} = (L_x, L_y, L_z)$ derived from `ST_LightRigDirection` (e.g. `topLeft` $\to \frac{1}{\sqrt{3}}(-1, -1, 1)$).
   Unit viewer vector $\vec{V} = (0, 0, 1)$.
   Halfway vector $\vec{H} = \frac{\vec{L} + \vec{V}}{\|\vec{L} + \vec{V}\|}$.
3. **Blinn-Phong Illumination**:
   $$I = k_a + k_d \max(\vec{N} \cdot \vec{L},\, 0) + k_s \left( \max(\vec{N} \cdot \vec{H},\, 0) \right)^\gamma$$
   Material constants map directly to material presets:
   - `warmMatte`: $k_a = 0.35, k_d = 0.70, k_s = 0.05, \gamma = 4$
   - `plastic`: $k_a = 0.20, k_d = 0.65, k_s = 0.45, \gamma = 32$
   - `metal`: $k_a = 0.10, k_d = 0.30, k_s = 0.90, \gamma = 64$

---

## 4. Backend Implementation: `tiny-skia` Multi-Pass Rasterizer

### 4.1 Separable Fast Gaussian Blur ($O(1)$ Box Blur Approximation)
True Gaussian convolution with kernel size $K$ has $O(K^2)$ complexity per pixel. By the **Central Limit Theorem**, cascading $M=3$ successive box filters approximates a continuous Gaussian distribution with $< 1.2\%$ relative error.

```
Input Pixmap ──► Pass 1: Horiz Box ──► Pass 2: Vert Box ──► Pass 3: Horiz Box ──► ... ──► Output
```

1. **Box Kernel Radius Calculation**:
   Given target standard deviation $\sigma = \frac{r_{\text{px}}}{2.0}$:
   $$w_{\text{ideal}} = \sqrt{\frac{12 \sigma^2}{M} + 1} = \sqrt{4 \sigma^2 + 1}$$
   Compute odd integers $w_l = 2 \lfloor w_{\text{ideal}} / 2 \rfloor - 1$ and $w_u = w_l + 2$.
   Number of iterations with width $w_u$:
   $$m = \text{round}\left( \frac{12 \sigma^2 - M w_l^2 - 4 M w_l - 3 M}{-4 w_l - 4} \right)$$
   Iterations use radii $r_i = \lfloor w / 2 \rfloor$.
2. **Sliding Window Accumulator ($O(1)$ per pixel)**:
   For a 1D scanline of length $W$, maintaining a running sum accumulator yields $O(1)$ operations per pixel, completely independent of blur radius $r$:
   $$\text{Sum}_{x} = \text{Sum}_{x-1} + \text{Pixel}[x + r] - \text{Pixel}[x - r - 1]$$
   $$\text{Output}[x] = \text{round}\left( \frac{\text{Sum}_x}{2r + 1} \right)$$

### 4.2 Reusable Scratch Pixmap Memory Pool
Allocating temporary `tiny_skia::Pixmap` buffers during slide playback causes heap fragmentation and violates latency budgets.
- `ScratchPixmapPool`: Provides pre-allocated, thread-local scratch buffers up to $1920 \times 1080 \times 4$ bytes.
- Workers lease a scratch buffer, perform multi-pass convolutions, and return it cleanly upon completion.

```rust
pub struct ScratchPixmapPool {
    pool: std::sync::Mutex<Vec<tiny_skia::Pixmap>>,
    max_retained: usize,
}

impl ScratchPixmapPool {
    pub fn lease(&self, width: u32, height: u32) -> tiny_skia::Pixmap {
        let mut guard = self.pool.lock().unwrap();
        if let Some(mut pixmap) = guard.pop() {
            if pixmap.width() >= width && pixmap.height() >= height {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                return pixmap;
            }
        }
        tiny_skia::Pixmap::new(width, height)
            .expect("Fatal: failed to allocate effect scratch buffer")
    }

    pub fn release(&self, pixmap: tiny_skia::Pixmap) {
        let mut guard = self.pool.lock().unwrap();
        if guard.len() < self.max_retained {
            guard.push(pixmap);
        }
    }
}
```

---

## 5. Backend Implementation: Standalone Vector SVG Serializer

In SVG exports (`ot export --format svg`), shape effects serialize into pure, standard W3C SVG filters within `<defs>`. This ensures zero rasterization artifacts and microsecond export speeds.

### 5.1 Normative SVG `<filter>` Templates

#### Outer Drop Shadow Filter
```xml
<filter id="fx-outer-shdw-1" x="-30%" y="-30%" width="160%" height="160%">
  <feGaussianBlur in="SourceAlpha" stdDeviation="4.50" result="blurOut"/>
  <feOffset in="blurOut" dx="3.54" dy="3.54" result="offsetOut"/>
  <feFlood flood-color="#000000" flood-opacity="0.40" result="colorOut"/>
  <feComposite in="colorOut" in2="offsetOut" operator="in" result="shadow"/>
  <feMerge>
    <feMergeNode in="shadow"/>
    <feMergeNode in="SourceGraphic"/>
  </feMerge>
</filter>
```

#### Inner Shadow Filter
```xml
<filter id="fx-inner-shdw-1" x="-10%" y="-10%" width="120%" height="120%">
  <feOffset in="SourceAlpha" dx="2.00" dy="2.00" result="offset"/>
  <feComposite in="SourceAlpha" in2="offset" operator="out" result="inverse"/>
  <feGaussianBlur in="inverse" stdDeviation="3.00" result="blur"/>
  <feFlood flood-color="#000000" flood-opacity="0.50" result="color"/>
  <feComposite in="color" in2="blur" operator="in" result="coloredBlur"/>
  <feComposite in="coloredBlur" in2="SourceAlpha" operator="in" result="innerShadow"/>
  <feMerge>
    <feMergeNode in="SourceGraphic"/>
    <feMergeNode in="innerShadow"/>
  </feMerge>
</filter>
```

#### Glow Halo Filter
```xml
<filter id="fx-glow-1" x="-50%" y="-50%" width="200%" height="200%">
  <feGaussianBlur in="SourceAlpha" stdDeviation="6.00" result="blur"/>
  <feFlood flood-color="#4F81BD" flood-opacity="0.75" result="glowColor"/>
  <feComposite in="glowColor" in2="blur" operator="in" result="glowHalo"/>
  <feMerge>
    <feMergeNode in="glowHalo"/>
    <feMergeNode in="SourceGraphic"/>
  </feMerge>
</filter>
```

#### Soft Edges Filter
```xml
<filter id="fx-soft-edge-1" x="0%" y="0%" width="100%" height="100%">
  <feGaussianBlur in="SourceAlpha" stdDeviation="5.00" result="maskBlur"/>
  <feComposite in="SourceGraphic" in2="maskBlur" operator="in"/>
</filter>
```

#### Reflection via Linear Opacity SVG Mask
```xml
<mask id="refl-mask-1">
  <rect x="0" y="0" width="100%" height="100%" fill="url(#refl-grad-1)"/>
</mask>
<linearGradient id="refl-grad-1" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0%" stop-color="#ffffff" stop-opacity="0.50"/>
  <stop offset="70%" stop-color="#ffffff" stop-opacity="0.00"/>
</linearGradient>
<!-- Mirrored clone element referencing the mask -->
<use href="#shape-1" transform="translate(0, 520) scale(1, -1)" mask="url(#refl-mask-1)"/>
```

---

## 6. Performance, Memory & Latency Guarantees

All shape effect and 3D implementations are bound by rigorous Non-Functional Requirements (NFRs):

| Metric | Bound / Limit | Enforcement Mechanism |
| :--- | :--- | :--- |
| **Separable Blur Complexity** | $O(W \times H)$ | Sliding accumulator 3-box blur algorithm (§4.1). |
| **Maximum Blur Radius** | $\le 64\text{ px}$ | Clamped in `SlideCompiler`; emits `WarningEvent::FidelityDegraded`. |
| **Scratch Memory Pool RSS** | $\le 16\text{ MB}$ / worker thread | Bounded `ScratchPixmapPool` with max 2 cached full-slide pixmaps. |
| **Replay Rasterization Latency** | $< 15\text{ ms}$ (1080p complex slide) | Pre-compiled vector commands + separable SIMD convolution. |
| **Replay Rasterization (Thumbnail)** | $< 1.5\text{ ms}$ (320×180 thumbnail) | Scaled blur kernel footprint ($r_{\text{thumb}} = r_{\text{slide}} \cdot \frac{320}{1920}$). |
| **SVG Filter Graph Size** | $< 1.2\text{ KB}$ / filter | Pure declarative SVG nodes; zero raster data bloat. |

### 6.1 Pathological Slide XML Protection
DrawingML XML may maliciously or erroneously specify unbounded blur radii (e.g. `blurRad="21600000"` = 24 inches).
- The engine clamps blur radius:
  $$r_{\text{clamped}} = \min(r_{\text{authored}},\, 64.0\text{ px})$$
- When clamping occurs, the compiler fires:
  ```rust
  WarningEvent::FidelityDegraded {
      element_id: shape_id.clone(),
      reason: format!("Blur radius {} exceeds safety cap of 64px; clamped", r_authored),
  }
  ```

---

## 7. Error Handling, Fallback Matrix & Graceful Degradation

### 7.1 Updated Normative Fidelity Matrix

With PRD 08, shape effects and 3D scenes transition from `OMITTED` to fully supported contracts:

| Feature | Class | Execution Details |
| :--- | :--- | :--- |
| **Outer Drop Shadow (`outerShdw`)** | **FULL** | Exact trigonometric offset, affine skew/scale, 3-box Gaussian blur. |
| **Inner Shadow (`innerShdw`)** | **FULL** | Inverted alpha mask convolution clipped to shape interior. |
| **Glow Halo (`glow`)** | **FULL** | Dilated boundary blur with non-linear exponential alpha falloff. |
| **Soft Edges (`softEdge`)** | **FULL** | Inset erosion convolution remap preserving outer element bounds. |
| **Reflection (`reflection`)** | **FULL** | Vertical affine mirror with linear alpha gradient falloff ramp. |
| **3D Scene Camera (`scene3d`)** | **FULL** | Orthographic & perspective homography projections onto slide canvas. |
| **3D Bevel & Extrusion (`sp3d`)** | **DEGRADED** | Bevel normal lighting & side-wall extrusion; complex intersection meshes approximate as layered planar contours. |
| **3D Material Lighting** | **FULL** | Blinn-Phong ambient/diffuse/specular shading for matte, plastic, metal. |

### 7.2 Memory Pressure Fallback
If system allocation fails or scratch buffer limits are breached:
1. Bypass the effect filter pass.
2. Render base geometric path and solid/gradient fill cleanly.
3. Emit `WarningEvent::FidelityDegraded { element_id, reason: "Memory allocation failed for effect filter pass" }`.
4. Never panic or terminate presentation rendering.

---

## 8. Verification & Quality Regime

In conformance with [PRD 07](./07-quality-and-testing.md), the effects engine is validated through L1–L5 gates:

```
        ┌────────────────────────────────────────────────────────┐
        │ L1: Unit Tests (Math, Trig, Box Blur, Blinn-Phong)     │
        └───────────────────────────┬────────────────────────────┘
                                    ▼
        ┌────────────────────────────────────────────────────────┐
        │ L2a: Vector Scene AST Goldens (JSON snapshots)         │
        └───────────────────────────┬────────────────────────────┘
                                    ▼
        ┌────────────────────────────────────────────────────────┐
        │ L2b: Perceptual Diff Goldens (DSSIM <= 0.02 vs Pinned) │
        └────────────────────────────────────────────────────────┘
```

### 8.1 L1 Unit Verification
- **Box Blur Radii Calculation**: Test that 3 successive passes with $(r_1, r_2, r_3)$ match target Gaussian variance $\sigma^2 = \sum \frac{r_i(r_i + 1)}{3}$.
- **Trigonometric Offset**: Verify offset vector $(\Delta x, \Delta y)$ across quadrant boundaries ($0^\circ, 90^\circ, 180^\circ, 270^\circ$).
- **Normal Vector Norm**: Guarantee $\|\vec{N}(x, y)\| = 1.0 \pm 10^{-6}$ across all bevel curve types.

### 8.2 L2a & L2b Golden Baselines
- **`tests/corpus/composition/effects_shadow_glow.pptx`**: Exercises drop shadow, inner shadow, and glow combinations. Verified against committed PNG baseline with $\text{DSSIM} \le 0.02$.
- **`tests/corpus/composition/effects_3d_bevel.pptx`**: Exercises bevel presets (`circle`, `relaxed`, `slant`), lighting directions (`tl`, `tr`), and material presets.
- **SVG Goldens**: Headless export generates `slide.svg`; validated with `xmllint` and verified to contain exact valid `<filter>` elements.
