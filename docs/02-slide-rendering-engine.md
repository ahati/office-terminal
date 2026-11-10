# PRD 02: Slide Rendering Engine

## 1. Overview & Objectives
The Slide Rendering Engine translates the intermediate `pptx-model::Slide` representation into a high-resolution, pixel-perfect 2D raster image (`tiny-skia::Pixmap`). 

Because `yukiyokotani/office-open-xml-viewer` implemented its rendering engine in TypeScript on the browser's Canvas 2D API, `kitty-pptx` provides a native, pure-Rust headless rasterizer built on **`tiny-skia`** (2D graphics) and **`cosmic-text`** (typography & layout).

---

## 2. Rendering Pipeline

```
  pptx_model::Slide (AST)
            │
            ▼
┌────────────────────────────────────────────────────────┐
│ 1. Canvas Setup                                        │
│    - Compute target pixel dimensions (e.g., 1920x1080) │
│    - Allocate tiny-skia::Pixmap                        │
│    - Render Slide Background (Solid / Gradient / Blip) │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│ 2. Element Traversal (Z-Ordered)                       │
│    For each SlideElement in slide.elements:            │
│    a. Push Transform Matrix (Translate, Rotate, Flip)  │
│    b. Dispatch by Element Type:                        │
│       - ShapeElement   ──► Path & Fill Rasterizer      │
│       - PictureElement ──► Blip / Image Compositor     │
│       - TableElement   ──► Grid & Cell Rasterizer      │
│       - ChartElement   ──► Vector / Fallback Renderer  │
│    c. Pop Transform Matrix                             │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│ 3. Text Layout & Rasterization                         │
│    - Calculate shape text bounding box (minus margins) │
│    - Shape text runs with cosmic-text                  │
│    - Compute line wraps, bullet indentation, margins   │
│    - Blit glyph bitmaps into Pixmap                    │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
                     tiny-skia::Pixmap (RGBA8888)
```

---

## 3. Coordinate Systems & Transforms

### 3.1 Dimensions & Scaling
- PPTX defines coordinates in English Metric Units (EMU) or points ($1\text{ pt} = 12700\text{ EMU}$, $1\text{ inch} = 72\text{ pt} = 914400\text{ EMU}$).
- Standard 16:9 widescreen slides are $9144000 \times 5143500\text{ EMU}$ ($720 \times 405\text{ pt}$ or $1280 \times 720\text{ px}$ at 96 DPI).
- The engine uses a configurable render resolution (default: $1920 \times 1080$ for high-DPI terminal screens, or dynamically scaled to the terminal pixel geometry).

### 3.2 Affine Transforms
Each shape specifies position (`x`, `y`), dimensions (`width`, `height`), rotation angle, and mirror flags (`flip_h`, `flip_v`):
```rust
let mut transform = tiny_skia::Transform::identity();
transform = transform.post_translate(elem.transform.x, elem.transform.y);

// PowerPoint rotates around the center of the shape bounding box
let cx = elem.transform.width / 2.0;
let cy = elem.transform.height / 2.0;
transform = transform.post_translate(cx, cy);
transform = transform.post_rotate(elem.transform.rotation);
if elem.transform.flip_h { transform = transform.post_scale(-1.0, 1.0); }
if elem.transform.flip_v { transform = transform.post_scale(1.0, -1.0); }
transform = transform.post_translate(-cx, -cy);
```

---

## 4. Color Pipeline & Theme Resolvers

Office DrawingML files do not store hardcoded hex colors for most elements; they reference theme slots with modifiers.

### 4.1 Theme Color Resolution
Base slots include `dk1`, `lt1`, `dk2`, `lt2`, and `accent1` through `accent6`. The parser resolves them to base sRGB colors.

### 4.2 Color Modifiers (ECMA-376 Math)
The renderer applies modifier transformations in HSL or RGB space:
- **`lumMod` & `lumOff` (Luminance modulation / offset)**:
  $$L' = L \times \text{lumMod} + \text{lumOff}$$
- **`tint` (Mix with white)**:
  $$C' = C \times \text{tint} + (1.0 - \text{tint}) \times 255$$
- **`shade` (Mix with black)**:
  $$C' = C \times \text{shade}$$
- **`alpha`**: Multiplies opacity ($0.0 \dots 1.0$).

---

## 5. Shape & Vector Geometry

### 5.1 Preset Shapes
PowerPoint defines over 180 preset shapes (`rect`, `roundRect`, `ellipse`, `diamond`, `rightArrow`, etc.).
- Preset geometries are implemented via parametric path generators that return a `tiny_skia::Path`.
- Example for `roundRect`: accepts an adjustable corner radius modifier (`adj` in OOXML, default 16.67% of the smaller dimension).

### 5.2 Custom Paths (`PathCmd`)
Custom shapes from DrawingML (`<a:custGeom>`) supply vector instructions:
```rust
let mut pb = tiny_skia::PathBuilder::new();
for cmd in &shape.paths {
    match cmd {
        PathCmd::MoveTo(p) => pb.move_to(p.x, p.y),
        PathCmd::LineTo(p) => pb.line_to(p.x, p.y),
        PathCmd::QuadTo(ctrl, to) => pb.quad_to(ctrl.x, ctrl.y, to.x, to.y),
        PathCmd::CubicTo(c1, c2, to) => pb.cubic_to(c1.x, c1.y, c2.x, c2.y, to.x, to.y),
        PathCmd::Close => pb.close(),
    }
}
let path = pb.finish().unwrap();
```

### 5.3 Fills & Strokes
- **Solid Fills**: Drawn with `tiny_skia::Paint` using sRGB colors with alpha.
- **Linear Gradients**: Computed with start/end points derived from the gradient angle, with intermediate color stops.
- **Strokes**: Configured with line width, `tiny_skia::LineCap` (Butt, Round, Square), `tiny_skia::LineJoin` (Miter, Round, Bevel), and dash pattern arrays.

---

## 6. Typography & Text Layout (`cosmic-text`)

Slide readability depends on high-quality text layout, correct line breaks, and font styling.

```
┌────────────────────────────────────────────────────────┐
│                   cosmic-text Engine                   │
├────────────────────────────────────────────────────────┤
│ 1. fontdb: Loads system fonts + bundled open fonts    │
│    (Inter, Liberation Sans, Roboto fallback)          │
│ 2. Text Shaping: Uses rustybuzz (HarfBuzz) for ligatures│
│ 3. Layout: Word wrapping within bounding box           │
│ 4. Glyph Rasterization: Swash engine renders to pixels │
└────────────────────────────────────────────────────────┘
```

### 6.1 Font Matching & Fallbacks
- Primary font matching: Attempts to match requested PPTX font families (e.g. `Calibri`, `Segoe UI`, `Arial`).
- Fallback chain: If the requested font is unavailable, falls back to standard system sans-serif, serif, or monospace fonts registered in `fontdb`.
- Bundled fallback font: Packages a compact, permissive open-source font (e.g., `Liberation Sans`) inside the binary so slides always render readable text even on minimal headless Linux installs.

### 6.2 Multi-Run Paragraphs & Formatting
A single paragraph can contain multiple runs with varying weights and sizes:
```rust
let mut buffer = cosmic_text::Buffer::new(&mut font_system, metrics);
buffer.set_size(&mut font_system, shape_width - (left_margin + right_margin), shape_height);

for run in &paragraph.runs {
    buffer.set_text(
        &mut font_system,
        &run.text,
        cosmic_text::Attrs::new()
            .weight(if run.bold { cosmic_text::Weight::BOLD } else { cosmic_text::Weight::NORMAL })
            .style(if run.italic { cosmic_text::Style::Italic } else { cosmic_text::Style::Normal })
            .color(run.color),
        cosmic_text::Shaping::Advanced,
    );
}
```

### 6.3 Bullets & Numbered Lists
- Supports bullet character prefixes (`•`, `-`, `➢`), bullet offsets, and hanging indents.
- Automatically calculates auto-numbering values (`1.`, `2.`, `a.`, `b.`) across consecutive paragraphs based on upstream numbering definitions.

---

## 7. Images & Media Compositing

1. **Decoding**: Embedded images (`ppt/media/image*.png|jpg`) are decompressed and decoded into RGBA byte arrays using the `image` crate.
2. **Aspect Ratio Preservation**:
   - `stretch`: Scales image to fill entire shape bounds.
   - `crop`: Calculates source crop rectangle (`srcRect`) and maps it to target destination.
3. **Alpha Channel & Blending**: Preserves 8-bit PNG alpha channels for icons and transparent logos.

---

## 8. Tables & Grids

Tables are rendered as hierarchical cells:
1. Calculates column widths from `gridCol` definitions and row heights from `tr` heights.
2. Renders cell backgrounds (solid fills or alternating band styles).
3. Renders inner and outer cell border strokes.
4. Renders cell text body inside cell padding bounds (`marL`, `marR`, `marT`, `marB`).

---

## 9. Performance & Concurrency

- **Stateless Design**: The `SlideRenderer` is completely thread-safe (`Send + Sync`).
- **Parallel Pre-rendering**: Adjacent slides ($N-1$, $N+1$) are rasterized asynchronously on background threads via `rayon` or `tokio::task::spawn_blocking`, ensuring instant slide flips in presentation mode.
- **Render Latency Budget**:
  - Simple slide (Title + bullet list + logo): `< 15 ms`.
  - Complex slide (10+ shapes, images, table): `< 40 ms`.
