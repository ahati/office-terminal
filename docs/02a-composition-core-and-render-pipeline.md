# PRD 02a: Composition Core & Render Pipeline

> Part of the PRD 02 split (02a/02b/02c/02d). Specifies the **slide compiler** (block B10), the **tiny-skia rasterizer backend**, and the **render pipeline/scheduler** (block B11). Normative interface signatures are defined in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Two-Stage Architecture

The Composition Core converts parsed PPTX slides into display output using a clean **two-stage architecture**:
1. **Stage 1: Scene Compilation (`SlideCompiler`)**: Translates PPTX DrawingML models into a resolution-independent 2D display list ([`VectorSlide`](./06-component-interface-contracts.md) §2.1).
2. **Stage 2: Target Rasterization (`SlideRasterizer`)**: Traverses `VectorSlide` with `tiny-skia` and `swash` to produce straight-alpha [`RgbaSlide`](./06-component-interface-contracts.md#L45-L51) frames for terminal display and PNG export.

```
┌────────────────────────────────────────────────────────┐
│ Stage 1: SlideCompiler (Block B10)                     │
│ - Traverses Slide AST & background inheritance         │
│ - Parametric presets -> VectorPath (PRD 02b)           │
│ - Table layout -> cell rects & text (PRD 02d)          │
│ - cosmic-text shaping -> PositionedGlyphs (PRD 02c)    │
└───────────────────────────┬────────────────────────────┘
                            │ Emits pure vector display list
                            ▼
┌────────────────────────────────────────────────────────┐
│ VectorSlide (Resolution-Independent Display List)      │
│ - DrawPath { path, fill, stroke, transform }           │
│ - DrawText { runs: Vec<PositionedGlyphRun>, transform }│
│ - DrawImage { rel_id, src_rect, dest_rect, opacity }   │
│ - PushClip / PopClip                                   │
└───────────────────────────┬────────────────────────────┘
                            │
       ┌────────────────────┴────────────────────┐
       ▼ (Target Scale s)                        ▼ (Direct XML)
┌───────────────────────────────┐ ┌──────────────────────────────┐
│ SlideRasterizer (tiny-skia)   │ │ SlideSvgSerializer (PRD 05)  │
│ - Path rasterization          │ │ - Clean <path d="...">       │
│ - Swash glyph alpha blit      │ │ - <text> or vector glyphs    │
│ - Pixmap -> Straight RgbaSlide│ │ - Embedded / linked images   │
└──────────────┬────────────────┘ └──────────────┬───────────────┘
               ▼                                 ▼
   RgbaSlide (Terminal & PNG)                slide.svg
```

---

## 2. Coordinate System & Viewport Mapping

### 2.1 Units & Base Geometry
PPTX coordinates are defined in English Metric Units (EMU):
- $1\text{ in} = 914,400\text{ EMU}$, $1\text{ pt} = 12,700\text{ EMU}$, $1\text{ px (96 DPI)} = 9,525\text{ EMU}$.
- 16:9 widescreen: $9,144,000 \times 5,143,500\text{ EMU}$ ($10.0 \times 5.625\text{ in}$, base $960 \times 540\text{ px}$ at 96 DPI).

### 2.2 Uniform Scaling & Letterbox Offsets
When rasterizing `VectorSlide` $(W_{\text{emu}}, H_{\text{emu}})$ to target $(W_{\text{target}}, H_{\text{target}})$:
1. **Uniform Scale Factor**:
   $$s = \min\left(\frac{W_{\text{target}}}{W_{\text{emu}}},\; \frac{H_{\text{target}}}{H_{\text{emu}}}\right)$$
2. **Centered Offsets (Letterbox / Pillarbox)**:
   $$dx = \frac{W_{\text{target}} - (W_{\text{emu}} \cdot s)}{2.0},\qquad dy = \frac{H_{\text{target}} - (H_{\text{emu}} \cdot s)}{2.0}$$

### 2.3 Canvas Epilogue & Alpha Conversion
`tiny-skia` uses premultiplied RGBA8. The rasterizer epilogue converts pixels in-place to straight (non-premultiplied) RGBA8:
```rust
fn finish_canvas(pixmap: tiny_skia::Pixmap, target: &RenderTargetSpec) -> RgbaSlide {
    let mut pixels = pixmap.take();
    unpremultiply_rgba8(&mut pixels);
    RgbaSlide {
        width: target.width_px,
        height: target.height_px,
        pixels: Arc::from(pixels.into_boxed_slice()),
    }
}
```

---

## 3. The Two-Tier Caching Advantage

Separating compilation from rasterization provides a major performance and memory advantage:

```
┌───────────────────────────────┐
│ Tier 1: VectorSlide Cache     │ Key: slide_index (usize)
│ (Resolution-Independent)      │ Memory: ~15 KB / slide
└──────────────┬────────────────┘
               │ Replayed at target scale s
       ┌───────┴───────┐
       ▼               ▼
┌─────────────┐ ┌─────────────┐
│ 1920x1080   │ │ 320x180     │ Key: (slide_index, RenderTargetSpec)
│ Presentation│ │ Thumbnail   │ Memory: ~8.3 MB (1080p), ~0.2 MB (thumb)
└─────────────┘ └─────────────┘
```

1. **Zero Re-Shaping on Scale Change**: When the TUI needs both a 1080p slide and a 320×180 thumbnail, HarfBuzz text shaping (`rustybuzz`), preset shape decomposition, and table layout occur **only once**. Rasterizing the thumbnail simply replays `VectorSlide` at thumbnail scale ($< 0.8\text{ ms}$).
2. **Byte-Bounded Cache Budget**: Default budget is 64 MB (`[presenter] cache_mb`). 5 frames at 1080p take $\approx 41.5\text{ MB}$; higher DPI targets scale the retention window automatically.

---

## 4. Pipeline Scheduler & Priority Prefetch

```
Active Slide N ──► Prefetch Priority: N ──► N+1 ──► N-1 ──► N+2 ──► N-2
```

1. **Cooperative Cancellation**: Any navigation jump or target change increments an atomic `pipeline_generation: AtomicU64`. Worker tasks verify `generation == current_generation()` before allocating resources, dropping stale jobs instantly.
2. **Idle Staging Coordination**: In interactive mode (`stage == true`), rasterized frames pass to the staging thread off the UI thread, encoding PNG and transmitting via `stage_sink.stage(id, &bitmap)`. Presenting at flip time remains an $O(1)$ single sequence ($< 16\text{ ms}$).

---

## 5. Invalidation Rules

| Cause | Pipeline Action | Staging Action |
| :--- | :--- | :--- |
| **`Resize`** | **No re-compile, no re-raster.** Vector and raster caches remain valid. | Recompute `CellRect`; driver issues `reposition(id, new_cells)`. |
| **`TargetChanged`** | Replay cached `VectorSlide` at new scale $s_{\text{new}}$. Increment generation. | Staged pool released; new frames staged. |
| **`DeckReload`** | Invalidate both `VectorSlide` and raster caches. Rebuild pipeline. | Driver issues `release_all()`; pool reset. |

---

## 6. Performance Guarantees

| Metric | Target (Reference Hardware) | Measurement Protocol |
| :--- | :--- | :--- |
| Slide compilation to `VectorSlide` | $< 8\text{ ms}$ (simple) / $< 20\text{ ms}$ (complex) | [PRD 07](./07-quality-and-testing.md) §5 |
| Replay rasterization (`VectorSlide` $\to$ `RgbaSlide`) | $< 7\text{ ms}$ (1080p) / $< 0.8\text{ ms}$ (thumbnail) | PRD 07 §5 |
| Staged slide flip latency (keypress $\to$ display) | $< 16\text{ ms}$ (p99) | PRD 07 §5 |
| Full 20-slide thumbnail grid generation | $< 250\text{ ms}$ total (parallel) | PRD 07 §5 |
