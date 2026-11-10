# PRD 02c: Typography & Text Layout Engine

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies font discovery, substitution, rich-run shaping, paragraph geometry, the **cosmic-text $\to$ tiny-skia rasterization bridge**, and bullet handling. Normative interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Architecture

Text accounts for the vast majority of visual information in presentations. Incorrect metrics cascade into reflow bugs, clipped text, and broken slide layouts. The typography stack composes:
- **`fontdb`**: Font asset discovery (bundled, system, user).
- **`rustybuzz` & `bidi`**: Complex script shaping and bidirectional reordering.
- **`cosmic-text`**: Multi-run buffer layout and line breaking.
- **`swash` / `tiny-skia` Blit Bridge**: Glyph rasterization and alpha mask composition onto the canvas.

```
┌─────────────────────────────────────────────────────────────────┐
│ TextLayoutEngine (Block B8)                                     │
│                                                                 │
│ ┌──────────────────────┐        ┌─────────────────────────────┐ │
│ │ FontRegistry         │        │ cosmic-text Layout Buffer   │ │
│ │ - Bundled Carlito    │ ───►   │ - Rich spans (color, weight)│ │
│ │ - Bundled Caladea    │        │ - Insets, alignment, wrap   │ │
│ │ - System / User dirs │        │ - Paragraph & line spacing  │ │
│ └──────────────────────┘        └──────────────┬──────────────┘ │
│                                                │ layout runs    │
│                                                ▼                │
│                                 ┌─────────────────────────────┐ │
│                                 │ SwashCache Glyph Rasterizer │ │
│                                 │ - 4 subpixel bins (x axis)  │ │
│                                 │ - 8-bit alpha mask cache    │ │
│                                 └──────────────┬──────────────┘ │
│                                                │ blit alpha     │
│                                                ▼                │
│                                 ┌─────────────────────────────┐ │
│                                 │ tiny-skia Pixmap Blitter    │ │
│                                 └─────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. Font Registry & Substitution Matrix

### 2.1 Registry Hierarchy
1. **Bundled Static Fonts** (compiled in-binary via `include_bytes!`, zero system dependencies):
   - **Carlito** (Regular, Bold, Italic, Bold-Italic): Metric-compatible clone of Microsoft **Calibri** (the default PowerPoint body font).
   - **Caladea** (Regular, Bold, Italic, Bold-Italic): Metric-compatible clone of Microsoft **Cambria**.
   - Both are SIL Open Font License (OFL) licensed, ensuring faithful wrap boundaries on minimal headless servers.
2. **System Fonts**: Standard platform directories (`/usr/share/fonts`, `/Library/Fonts`, etc.).
3. **User Fonts**: Custom paths from `[typography] custom_font_dirs` ([PRD 05](./05-cli-and-export.md) §3).

### 2.2 Ordered Substitution Table

| Requested Font Family | Substitute | Relationship | Diagnostics |
| :--- | :--- | :--- | :--- |
| **Calibri** | **Carlito** (bundled) | **Metric-compatible** (wraps identically) | Silent |
| **Cambria** | **Caladea** (bundled) | **Metric-compatible** (wraps identically) | Silent |
| **Aptos** / Aptos Display | Carlito | Approximate (similar proportions) | `WarningEvent::FontSubstituted` |
| **Arial** | Liberation Sans $\to$ Carlito | Metric-compatible if installed; else Carlito | Emits if fallback used |
| **Times New Roman** | Liberation Serif $\to$ Caladea | Metric-compatible if installed; else Caladea | Emits if fallback used |
| **Consolas** / Courier New | Liberation Mono $\to$ DejaVu Sans Mono | Fixed-pitch approximate | `WarningEvent::FontSubstituted` |
| *Unknown / uninstalled* | `fontdb` system default sans | Best-effort fallback | `WarningEvent::FontSubstituted` |

### 2.3 Legacy Bullet Font & Symbol Translation
Decks authored on Windows frequently use legacy symbol fonts (`Wingdings`, `Webdings`, `Symbol`) for bullet characters (`buChar`). On Linux/macOS systems lacking these fonts, characters render as broken glyphs or letters (`l`, `n`). The engine applies a normative mapping:

| Font Family | Char Code | Mapped Unicode Glyph | Description |
| :--- | :--- | :--- | :--- |
| `Wingdings` | `0x6C` / `0xF06C` | `●` (U+25CF) | Black Circle |
| `Wingdings` | `0x6E` / `0xF06E` | `■` (U+25A0) | Black Square |
| `Wingdings` | `0x75` / `0xF075` | `◆` (U+25C6) | Black Diamond |
| `Wingdings` | `0xD8` / `0xF0D8` | `➢` (U+27A2) | Three-D Top-Lighted Rightwards Arrowhead |
| `Wingdings` | `0xFC` / `0xF0FC` | `✔` (U+2714) | Heavy Check Mark |
| `Symbol` | `0xB7` / `0xF0B7` | `•` (U+2022) | Standard Bullet |

---

## 3. Rich-Run Shaping & Paragraph Geometry

### 3.1 Shaping Spans
A paragraph is shaped using `cosmic-text::Buffer::set_rich_text`:
```rust
let spans: Vec<(&str, Attrs)> = paragraph.runs.iter().map(|run| {
    let mut attrs = Attrs::new()
        .family(Family::Name(resolved_family(&run.font)))
        .weight(if run.bold.unwrap_or(body_defaults.bold) { Weight::BOLD } else { Weight::NORMAL })
        .style(if run.italic.unwrap_or(body_defaults.italic) { Style::Italic } else { Style::Normal });
    
    let font_size = run.size_pt.unwrap_or(body_defaults.size_pt) * scale * autofit_scale;
    attrs = attrs.metrics(Metrics::new(font_size, font_size * line_spacing_ratio));
    attrs = attrs.color(run_color(run, body_defaults, presentation_defaults));
    (run.text.as_str(), attrs)
}).collect();

buffer.set_rich_text(&mut font_system, &spans, Shaping::Advanced, default_attrs);
```

### 3.2 Paragraph Spacing Math
- **Line Spacing**:
  - Percentage (`spcPct`): Line height $H_{\text{line}} = \text{font\_size} \cdot \frac{\text{spcPct}}{100,000}$.
  - Absolute points (`spcPts`): $H_{\text{line}} = \frac{\text{spcPts}}{100} \text{ pt} \cdot s$.
- **Space Before / After**:
  - Added between paragraph layout boxes: $Y_{\text{space}} = \frac{\text{space}}{100} \text{ pt} \cdot s$.
- **Hanging Indent & Margins**:
  - Body left margin $mar_l$, right margin $mar_r$, and indent:
    - If `indent < 0` (hanging indent): Bullet occupies $[mar_l + indent,\, mar_l)$; text aligns to $mar_l$.

### 3.3 Vertical Alignment
After laying out all paragraphs, the total rendered text height is $H_{\text{text}} = \sum \text{line\_heights}$.
Available height within shape: $H_{\text{avail}} = h_{\text{px}} - (t_{\text{ins}} + b_{\text{ins}}) \cdot s$.
- `Top` (`t`): $Y_{\text{start}} = t_{\text{ins}} \cdot s$.
- `Center` (`ctr`): $Y_{\text{start}} = t_{\text{ins}} \cdot s + \max\left(0.0,\; \frac{H_{\text{avail}} - H_{\text{text}}}{2.0}\right)$.
- `Bottom` (`b`): $Y_{\text{start}} = t_{\text{ins}} \cdot s + \max\left(0.0,\; H_{\text{avail}} - H_{\text{text}}\right)$.

---

## 4. The Cosmic-Text $\to$ Tiny-Skia Rasterization Bridge

Because `tiny-skia` does not contain a font rendering engine, `kitty-pptx` implements a high-performance rasterization bridge:

```rust
pub struct GlyphKey {
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    pub font_size_bits: u32,
    pub subpixel_bin: u8, // 0..=3 for x subpixel offsets: 0.0, 0.25, 0.5, 0.75
}

pub struct RasterizedGlyph {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub mask: Vec<u8>, // 8-bit coverage alpha mask
}
```

### 4.1 Subpixel Binning & Rasterization
1. For each glyph in `buffer.layout_runs()`:
   - Position: $x = \text{run.x} + \text{glyph.x}$, $y = \text{run.y} + \text{glyph.y}$.
   - Horizontal subpixel bin: $bin = \lfloor (x - \lfloor x \rfloor) \cdot 4.0 \rfloor \pmod 4$.
2. Lookup glyph in thread-local `SwashCache`:
   - If missing, rasterize via `swash::scale::Render::render()` in alpha mode to produce an 8-bit coverage mask.
3. Cache glyph mask in an LRU (bounded at 8 MB per worker thread).

### 4.2 Pixel Blit Operation
The rasterized mask is blitted directly into `tiny_skia::PixmapMut`:
```rust
for row in 0..glyph.height {
    for col in 0..glyph.width {
        let alpha = glyph.mask[(row * glyph.width + col) as usize];
        if alpha == 0 { continue; }
        
        let dest_x = glyph.left + col as i32;
        let dest_y = glyph.top + row as i32;
        if dest_x >= 0 && dest_x < canvas_w && dest_y >= 0 && dest_y < canvas_h {
            // Premultiplied alpha composition onto tiny_skia Pixmap
            pixmap.blend_pixel(dest_x as u32, dest_y as u32, color, alpha);
        }
    }
}
```
This achieves subpixel precision without color fringing and maintains throughput meeting the $< 5\text{ ms}$ text layout budget.

---

## 5. Autofit Semantics

| Mode | Behavior |
| :--- | :--- |
| **`noAutofit`** | Fixed font metrics; text clips at the shape bounding box on overflow. |
| **`normAutofit`** | Apply PowerPoint's precomputed `font_scale` (e.g. 0.82) and `line_spacing_reduction`. Because this scale was converged by PowerPoint at authoring time, applying it directly is correct-by-construction. |
| **`spAutoFit`** | Shape-grow-to-fit: In a slide viewer, geometry cannot expand without breaking layout; text renders at authored scale and clips at boundary (DEGRADED + warning). |

---

## 6. Underline & Strikethrough Geometry

- **Underline**: Rendered as a horizontal rule at baseline $+ \max(1.0,\, 0.1 \cdot \text{size})$. Thickness $t = \max(1.0,\, 0.05 \cdot \text{size})$.
- **Strikethrough**: Horizontal rule across glyph advance at baseline $- 0.35 \cdot \text{size}$. Double strike draws two rules separated by $1.5 \cdot t$.
