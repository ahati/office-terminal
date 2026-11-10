# PRD 02c: Typography & Text Layout Engine

> Part of the PRD 02 split. Specifies the text stack (block B8): font discovery and substitution, styled-run layout, paragraph and bullet geometry, autofit behavior, and script/vertical-text handling. Normative interfaces in [PRD 06](./06-component-interface-contracts.md).

## 1. Purpose & Scope

Text is the highest-fidelity-risk subsystem of the renderer: decks are mostly text, and wrong metrics cascade into wrong wrapping, wrong line counts, and wrong-looking slides. The engine wraps **`cosmic-text`** (which composes `fontdb` font discovery, `rustybuzz` shaping, and `bidi` reordering) and is consumed by the shape/table/chart-placeholder renderers of [PRD 02d](./02d-element-renderers-and-fidelity-policy.md).

```
┌─────────────────────────────────────────────────────────┐
│ TextLayoutEngine (this spec)                            │
│  1. FontRegistry: system scan + bundled + custom dirs   │
│  2. Substitution matrix (metric-compatible first)       │
│  3. Rich-run shaping: TextBody → cosmic-text Buffer     │
│  4. Paragraph geometry: margins, indents, spacing       │
│  5. Bullets & auto-numbering                            │
│  6. Autofit application                                 │
└─────────────────────────────────────────────────────────┘
```

## 2. Font Registry & Substitution

### 2.1 Registry Sources (in lookup order)
1. **Bundled static fonts** compiled into the binary via `include_bytes!` and registered as immutable `fontdb` sources:
   - **Carlito** (regular/bold/italic/bold-italic) — metric-compatible with **Calibri**, the default PowerPoint body font.
   - **Caladea** (regular/bold/italic/bold-italic) — metric-compatible with **Cambria**.
   - Both are SIL OFL-licensed; bundling them guarantees readable, metrically-correct text on minimal headless installs.
2. **System fonts** via `fontdb` scan of standard platform font directories.
3. **User font directories** from `[typography] custom_font_dirs` ([PRD 05](./05-cli-and-export.md) §3).

A per-thread `FontSystem` is cached for the compositor worker pool (fontdb loading is expensive; reuse is mandatory for the §8 latency budgets).

### 2.2 Substitution Matrix

Requested-but-unavailable families resolve through this ordered table:

| Requested family | Substitute | Relationship |
| :--- | :--- | :--- |
| Calibri | Carlito | **Metric-compatible** (bundled) — wraps identically |
| Cambria | Caladea | **Metric-compatible** (bundled) |
| Aptos / Aptos Display | Carlito | Approximate (similar proportions; no free metric clone exists) |
| Arial | Liberation Sans | Metric-compatible if installed; else Carlito |
| Times New Roman | Liberation Serif | Metric-compatible if installed; else Caladea |
| Consolas / Courier New | Liberation Mono → DejaVu Sans Mono | **Approximate** (no metric-compatible free Consolas; mono width still aligns) |
| Segoe UI | Carlito | Approximate |
| *anything else* | fontdb generic fallback (system sans) | Best-effort |

Every substitution where the relationship is *approximate* emits `WarningEvent::FontSubstituted { requested, substitute }` (surfaced in diagnostics, [PRD 01](./01-parser-and-data-model.md) §7).

**CJK**: no CJK font is bundled (binary-size budget, [PRD 00](./00-system-overview.md) §5). cosmic-text performs per-glyph fallback through the registry; on hosts without any CJK font, glyphs render as tofu and a warning is emitted at first miss. The docs recommend installing `fonts-noto-cjk` for East-Asian decks. This trade-off is deliberate and stated, not accidental.

## 3. Rich-Run Layout — Corrected API Usage

> **Change from the previous revision.** The earlier sketch called `Buffer::set_text` per run. `set_text` *replaces the whole buffer contents*, so per-run calls discard all prior runs and their styling. Multi-run styled paragraphs use `set_rich_text` with span attributes:

```rust
let mut buffer = Buffer::new(&mut font_system, metrics);

let spans: Vec<(&str, Attrs)> = paragraph.runs.iter().map(|run| {
    let mut attrs = Attrs::new()
        .family(Family::Name(&resolve_family(&run.font)))
        .weight(if run.bold.unwrap_or(false) { Weight::BOLD } else { Weight::NORMAL })
        .style(if run.italic.unwrap_or(false) { Style::Italic } else { Style::Normal });
    if let Some(size) = effective_size(run, paragraph, body) {
        attrs = attrs.metrics(Metrics::new(size, size * default_line_ratio));
    }
    if let Some(color) = effective_color(run, body, presentation) {
        attrs = attrs.color(color);
    }
    (run.text.as_str(), attrs)
}).collect();

buffer.set_rich_text(&mut font_system, &spans, Shaping::Advanced,
                     attrs_list_from_defaults(paragraph, body));
```

Run properties are **three-state** in the model (`bold: Option<bool>` — `None` = inherit): resolution order is run → paragraph defaults → body defaults (`default_bold`, `default_font_size`) → presentation defaults (`major_font`/`minor_font` + `default_text_color`). This mirrors the parser's inheritance resolution and must not be re-implemented ad hoc per renderer.

**Decorative run properties** (drawn by this engine over/under shaped glyphs):
- Underline: `sng` renders as a single rule (cosmic-text native); other `underline_style` values (`dbl`, `dotted`, `dash`, `wavy`, …) render as DEGRADED single rules with style-appropriate color (`underline_color` when carried) + warning.
- Strikethrough: `strikethrough: bool` draws one rule over the glyph advance; `strike_double: bool` draws two parallel rules (both model-carried).

## 4. Paragraph Geometry

All model values are EMU (margins) or hundredths of a point (spacing); the engine converts once via the [PRD 02a](./02a-composition-core-and-render-pipeline.md) §2.3 scale.

| Property | Model field | Behavior |
| :--- | :--- | :--- |
| Body insets | `l_ins / r_ins / t_ins / b_ins` (defaults 91440 / 91440 / 45720 / 45720 EMU) | Subtracted from shape bbox to yield the layout box. |
| Paragraph margins | `mar_l / mar_r`, `indent` | `indent < 0` = hanging indent (bullet geometry, §5). |
| Alignment | `alignment` (`left|center|right|just|dist|...`) | `just` → justified; `dist` treated as `just` (distributed is out of scope, DEGRADED). |
| Space before/after | `space_before/space_after` (points or %) | Added between paragraphs; % resolves against default font size. |
| Line spacing | `line_spacing` (percent or points) | Percent multiplies the line height; points set absolute line height. |
| Wrap | `wrap: none | square` | `none` → no wrapping, may overflow bbox (clipped at shape bounds); `square` → normal wrap. |
| Vertical anchor | `vertical_anchor` (`t | ctr | b | ...`) | Positions the laid-out block within the layout box. |

Overflow behavior (no autofit): text clips at the shape bbox — matching PowerPoint's default. Nothing is ever silently reflowed or shrunk without an explicit autofit instruction (§6).

## 5. Bullets & Auto-Numbering

### 5.1 Bullet Kinds (model `Bullet` enum)
- **None** (`buNone`) — plain paragraph.
- **Character** (`buChar`) — prefix glyph with independent **size** (`buSzPct`/`buSzPts`) and **color** (`buClr`) groups. The parser resolves these groups independently of the marker typeface; the layout engine must honor that independence (substituted bullet fonts do not inherit run fonts, and vice versa).
- **Auto-numbered** (`buAutoNum`) — sequence labels generated across *consecutive* paragraphs sharing the same numbering definition:

| Format family | Rendering examples |
| :--- | :--- |
| `arabicPeriod` | `1.` `2.` `3.` |
| `arabicParenR` / `ParenR` | `1)` `2)` |
| `alphaLcPeriod` / `alphaUcPeriod` | `a.` / `A.` |
| `alphaLcParenR` / `alphaUcParenR` | `a)` / `A)` |
| `romanLcPeriod` / `romanUcPeriod` | `i.` / `I.` |
| `romanLcParenR` / `romanUcParenR` | `i)` / `I)` |

Sequence state resets when a non-numbered paragraph (or a different definition) intervenes. Start values (`startAt`) honored when carried.

- **Graphic bullet** (`buBlip`) — DEGRADED: renders the image scaled to the bullet size when the asset decodes; falls back to `•`.

### 5.2 Hanging Indent Geometry
Bullet label occupies `[mar_l + indent, mar_l)`; the text block starts at `mar_l`. Label is right-aligned to the text edge (PowerPoint convention). Bullet glyph missing from the substituted font → fallback glyph `•` + warning.

## 6. Autofit

The model carries the authored autofit decision and — critically — **PowerPoint's stored scale**:

| `auto_fit` | Behavior |
| :--- | :--- |
| `noAutofit` | Fixed box; clip on overflow. |
| `normAutofit` | Apply the stored `font_scale` (e.g. 0.85) to all font sizes in the body and `line_spacing_reduction` to line spacing. **No iterative re-fit in v1** — the stored values are PowerPoint's own converged result, so applying them is correct-by-construction for decks last saved by PowerPoint. If both are absent (hand-authored XML), render at authored size and clip. |
| `spAutoFit` | Shape-size-to-fit is a layout-time (authoring) concept; the renderer keeps the authored bbox and clips overflow (DEGRADED, warning). |

## 7. Scripts, Bidirectionality & Vertical Text

- **Shaping & bidi**: delegated to cosmic-text (`rustybuzz` + `bidi`); RTL paragraphs lay out correctly, including mixed-direction runs.
- **Line breaking**: Unicode UAX #14 via cosmic-text, which covers CJK behavior (no space required between CJK glyphs).
- **Vertical text** (`bodyPr @vert`): `horz` FULL; `vert270`/`vert` DEGRADED (rotate the laid-out block ±90° within the bbox — glyphs remain upright-rotated, matching the common case); `eaVert`, `mongolianVert`, `wordArtVert*` PLACEHOLDER (plain horizontal render + warning) per the fidelity matrix in [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2.
- **WordArt text warp** (`TextWarp`, 40 presets carried by the model): **DEGRADED** — text renders fully, unwrapped from its warp envelope ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2.1); warp envelopes are a post-v1 fidelity item.

## 8. Performance & Determinism

- `FontSystem` cached per worker thread; registry never rebuilt mid-session.
- Layout results are pure functions of `(TextBody, defaults, target scale, font set)` — deterministic for golden testing.
- Latency budget: full text layout + raster for a 6-bullet body ≤ 5 ms at 1920×1080 (measured per [PRD 07](./07-quality-and-testing.md) §5).
