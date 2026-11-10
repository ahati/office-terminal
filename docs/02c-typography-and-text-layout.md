# PRD 02c: Typography & Text Layout Engine

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies font discovery, substitution, rich-run shaping into `PositionedGlyphRun`s, paragraph geometry, the **cosmic-text $\to$ tiny-skia rasterization bridge**, and SVG text export modes. Normative interfaces live in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Architecture

The typography stack composes:
- **`fontdb`**: Font asset discovery (bundled, system, user).
- **`rustybuzz` & `bidi`**: Complex script shaping and bidirectional reordering.
- **`cosmic-text`**: Multi-run buffer layout, line breaking, and autofit scaling.
- **`VectorSlide` Text Seam**: Emits resolution-independent `PositionedGlyphRun` commands.
- **Pluggable Backends**: Swash alpha blitter for `tiny-skia` rasterization, and SVG `<text>` / `<path>` serializer for vector export.

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
│                                                │ shapes         │
│                                                ▼                │
│                                 ┌─────────────────────────────┐ │
│                                 │ PositionedGlyphRun (Vector) │ │
│                                 │ - Font ID, family, size     │ │
│                                 │ - Baseline (x, y) & glyph IDs││
│                                 └──────────────┬──────────────┘ │
│                                                │                │
│                 ┌──────────────────────────────┴──────────────┐ │
│                 ▼                                             ▼ │
│  ┌─────────────────────────────┐               ┌──────────────┴┐│
│  │ SwashCache (Rasterizer)     │               │ SVG Serializer││
│  │ - 4 subpixel bins (x axis)  │               │ - <text> spans││
│  │ - Blit to tiny-skia Pixmap  │               │ - <path> glyph││
│  └─────────────────────────────┘               └───────────────┘│
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
    
    let font_size = run.size_pt.unwrap_or(body_defaults.size_pt) * autofit_scale;
    attrs = attrs.metrics(Metrics::new(font_size, font_size * line_spacing_ratio));
    attrs = attrs.color(run_color(run, body_defaults, presentation_defaults));
    (run.text.as_str(), attrs)
}).collect();

buffer.set_rich_text(&mut font_system, &spans, Shaping::Advanced, default_attrs);
```

### 3.2 Spacing & Vertical Alignment
- **Line Spacing**: Percentage ($H_{\text{line}} = \text{font\_size} \cdot \frac{\text{spcPct}}{100,000}$) or absolute points ($H_{\text{line}} = \frac{\text{spcPts}}{100} \text{ pt}$).
- **Hanging Indent**: When `indent < 0`, bullet glyph occupies $[mar_l + indent,\, mar_l)$; text aligns to $mar_l$.
- **Vertical Alignment**: Total height $H_{\text{text}} = \sum \text{line\_heights}$. Offset inside shape box:
  - `Top`: $Y_{\text{start}} = t_{\text{ins}}$.
  - `Center`: $Y_{\text{start}} = t_{\text{ins}} + \max\left(0.0,\; \frac{H_{\text{avail}} - H_{\text{text}}}{2.0}\right)$.
  - `Bottom`: $Y_{\text{start}} = t_{\text{ins}} + \max\left(0.0,\; H_{\text{avail}} - H_{\text{text}}\right)$.

---

## 4. Backend Execution

### 4.1 Rasterization Bridge (`tiny-skia` + `swash`)
1. **Subpixel Binning**: Quantize horizontal glyph offset:
   $$bin = \lfloor (x - \lfloor x \rfloor) \cdot 4.0 \rfloor \pmod 4 \quad (0.0,\, 0.25,\, 0.5,\, 0.75)$$
2. **Coverage Mask LRU**: Rasterize via `SwashCache` into an 8-bit alpha mask (LRU cache budgeted at 8 MB per thread).
3. **Alpha Blit**: Blits glyph mask directly onto `tiny_skia::PixmapMut` with run color.

### 4.2 SVG Vector Text Serialization
The SVG backend supports two modes (`--svg-text <mode>`):
- **`SelectableText`**: Emits standard SVG `<text>` elements with `font-family`, `font-size`, and `<tspan>` nodes. Retains selectable/searchable text in web browsers.
- **`VectorGlyphs` (Default)**: Converts glyph outlines into exact SVG `<path d="...">` elements via `fontdue`/`swash`. Guarantees 100% pixel-faithful vector display even on machines lacking the original fonts.

---

## 5. Autofit & Underlines

- **Autofit**: Applies PowerPoint's precomputed `font_scale` (e.g. 0.85) and `line_spacing_reduction`. Because this scale was converged by PowerPoint at authoring time, applying it directly is correct-by-construction.
- **Underline**: Rendered as a rule at baseline $+ \max(1.0,\, 0.1 \cdot \text{size})$. Thickness $t = \max(1.0,\, 0.05 \cdot \text{size})$.
- **Strikethrough**: Horizontal rule across glyph advance at baseline $- 0.35 \cdot \text{size}$. Double strike draws two rules separated by $1.5 \cdot t$.
