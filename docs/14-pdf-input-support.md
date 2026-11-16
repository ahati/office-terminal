# PRD 14: PDF Input Support — Slide Decks & Documents

> Status: **design proposal** (normative for implementing PRs once accepted).
> Depends on: [PRD 00](./00-system-overview.md) (architecture), [PRD 01](./01-parser-and-data-model.md) (loader & asset store), [PRD 02a](./02a-composition-core-and-render-pipeline.md) (compile stage & pipeline), [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) (fidelity policy), [PRD 03a](./03a-kitty-graphics-protocol-driver.md) (driver), [PRD 04](./04-interactive-tui-presenter.md) (TUI), [PRD 05](./05-cli-and-export.md) (CLI & export), [PRD 06](./06-component-interface-contracts.md) (seam contracts), [PRD 07](./07-quality-and-testing.md) (test levels).
> Reader UI designs (continuous scrolling, two-page spread): [PRD 14a](./14a-pdf-reader-ui-designs.md).
> External engine: [`hayro`](https://github.com/LaurenzV/hayro) 0.9.x (Apache-2.0 OR MIT), pure Rust.
> Input feature only — orthogonal to the post-v1 *native PDF export* track (`pdf-writer`, [docs/README.md](./README.md) roadmap).

---

## 1. Problem, Goals & Non-Goals

### 1.1 Current state

Every input path assumes PPTX end to end:

- `PresentationLoader::load(Source) -> (pptx_model::Presentation, Arc<dyn AssetStore>)` (`kitty-pptx-core/src/loader/presentation_loader.rs`) — the return type is hardwired to the OOXML model.
- `sniff_container(&bytes)` accepts ZIP unconditionally as PPTX and rejects everything else (`SourceFormat` guard only; no extension or magic dispatch).
- `DefaultRenderPipeline::new(Arc<pptx_model::Presentation>, …)` (`pipeline/mod.rs`, `pipeline/worker.rs`) indexes `presentation.slides` and reads deck-level `slide_width/slide_height` for every compile.
- The TUI receives `.with_slides(Vec<pptx_model::Slide>)` for text search, media, comments and hyperlink hit-testing.

The seam below `VectorSlide` is clean (rasterizer, SVG serializer, metafile, contracts carry **zero** `pptx_model` references), but nothing above it can describe a PDF. Users receive PDF decks exported from PowerPoint/Keynote/LaTeX (beamer) and PDF *documents* (papers, reports, manuals); today the tool cannot open either.

### 1.2 Goals

1. **PDF slide decks** (landscape, uniform pages — beamer, PowerPoint-exported decks) present through the **existing UX unchanged**: fullscreen, presenter console, sorter, TOC, blank, export.
2. **PDF documents** (portrait-majority, mixed-orientation, mixed-size pages) open in a new **Reader mode**: focusable, zoomable, scrollable page panes displayed side by side and **left-aligned** (the right page may be clipped by policy), with slide-vs-document mode detection derived from page geometry — interaction layer specified in [PRD 14a](./14a-pdf-reader-ui-designs.md). This PRD covers the data/pipeline substrate it needs (per-page dimensions, windowed prefetch, text corpus).
3. **Format auto-detection** by magic bytes (`%PDF-`), with a user override (`--mode slides|reader|auto`).
4. **Vector-first discipline preserved**: PDF pages compile into `VectorSlide`; the raster bridge is an explicit, labeled approximation phase, not the end state.
5. **Export parity**: `export --format png,svg` and `dump` work for PDFs. Phase R SVG wraps an embedded raster; Phase V emits native vector SVG.
6. **Honest fidelity** ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md) extended): every unsupported PDF feature degrades to a typed warning or placeholder — never an invisible hole, never a crash.
7. **Bounded work, pure Rust, license-compatible**: no C/C++ dependencies, no `unsafe`, Apache/MIT engine, deterministic output, resource limits on hostile input.

### 1.3 Non-goals

- PDF **writing/editing**, form filling, annotation editing, digital signatures (cf. `pdf-writer` export track).
- Re-flowing document text into terminal cells. PDF pages are fixed-geometry surfaces; reading is image-based, search is overlay-based.
- OCR of raster-only PDFs; PDF/A archival validation; DRM/encrypted-document decryption (typed error, §9).
- Embedded multimedia (sound/movie annotations) playback — consistent with the existing external-media policy.
- Pixel-identical Acrobat/PDFium output. The target is parity-class rendering (SSIM-scored, [PRD 07](./07-quality-and-testing.md)), not engine emulation.

---

## 2. Normative Dependencies

### 2.1 Engine selection

| Crate | Version | Role | License |
| :--- | :--- | :--- | :--- |
| `hayro-syntax` | `=0.9.x` | Read-only PDF syntax layer: `Pdf`, `Page` (`crop_box`, `intersected_crop_box`, `rotation`, `render_dimensions`, `initial_transform`), annotations | Apache-2.0 OR MIT |
| `hayro-interpret` | `=0.9.x` | PDF interpreter emitting into an abstract `Device` (paths, clips, transparency groups, glyph runs, images); `InterpreterSettings::font_resolver` | Apache-2.0 OR MIT |
| `hayro` | `=0.9.x` | Bitmap renderer (`render`, `render_into` with arbitrary affine), `RenderCache`, `RenderSettings`, `PixmapSettings` | Apache-2.0 OR MIT |

Default features `embed-fonts` + `embed-cmaps` (standard-14 substitutes + CJK CMaps) — matching how the PPTX path already bundles Carlito/Caladea rather than depending on system fonts. `hayro` is `#![forbid(unsafe_code)]`, satisfying [AGENTS.md](../AGENTS.md) §D. MSRV 1.92 ≤ workspace toolchain 1.99.

**Consumption model:** pinned with `=` in `[workspace.dependencies]`, from crates.io (not vendored — no local patches expected). If a patch becomes necessary, follow the metafile policy ([PRD 13](./13-metafile-pictures-and-vector-playback.md) §2.2): in-tree copy under `crates/`, diff in `crates/patches/`, entry in `crates/VENDORED.md`, removed once released upstream.

**Optional dev-dependency:** `hayro-svg` (PDF → SVG) as a differential-oracle reference for Phase V goldens — never linked into release binaries (same pattern as `metafile-svg` in PRD 13).

### 2.2 Alternatives considered

| Candidate | Assessment |
| :--- | :--- |
| **`hayro`** | ✅ Most correct pure-Rust rasterizer (1,600+ PDF regression corpus sourced from pdf.js/PDFBox); `Device` trait enables the vector bridge; same author ecosystem as `pdf-rs`; active. ⚠️ Pre-1.0, API in flux; robustness on hostile input still being hardened (§8 limits). |
| `pdf-render` (PDFluent fork) | Stable releases, SSIM-gated; heavy SDK-shaped dependency tree (~33 MB, `vello_cpu` pinned "obsolete"); docs behind vendor site. Kept as fallback engine candidate behind the `PdfEngine` trait. |
| `pdf_oxide` | Fastest text extraction in the ecosystem (100% pass on 3,830-PDF corpus); rendering is raster-only via `tiny-skia` (same rasterizer as this repo). Attractive as a **text-extraction** companion (Phase T); rendering path lacks a vector bridge. |
| `pdfium-render` | Highest fidelity, but ships a C++ binary (static link or embedded via `pdfium-bind`); `unsafe` bindings violate the supply-chain posture; complicates single-binary SSH distribution. Rejected. |
| `mupdf-rs` | AGPL-3.0 — incompatible with this MIT project. Excluded. |
| `lopdf` / `pdf` (pdf-rs) | Structure manipulation only, no rendering. Insufficient alone; `hayro-syntax` already covers our read-only needs. |

### 2.3 Engine seam

All engine calls go through one internal trait so the engine remains swappable (and testable against oracles):

```rust
trait PdfEngine: Send + Sync {
    fn page_count(&self) -> usize;
    fn page_size_pt(&self, page: usize) -> (f32, f32);          // rotated, cropped
    fn page_rotation(&self, page: usize) -> u16;                 // 0/90/180/270
    fn render_rgba(&self, page: usize, scale: f32, cache: &mut RenderCache)
        -> Result<RgbaSlide, PdfError>;                          // Phase R
    fn interpret<D: Device>(&self, page: usize, device: &mut D)  // Phase V
        -> Result<(), PdfError>;
    fn outline(&self) -> Vec<TocEntry>;                          // Phase L
    fn links(&self, page: usize) -> Vec<PdfLinkAnnotation>;      // Phase L
    fn extract_text(&self, page: usize) -> Result<PageText, PdfError>; // Phase T
}
```

`HayroEngine` is the sole implementation at landing; the trait exists so the fork (`pdf-render`) or `pdf_oxide` can be introduced as a differential engine in tests without touching callers.

---

## 3. Architecture

### 3.1 Format detection & deck abstraction

```
Source (path | stdin)
        │
        ▼
sniff_container(&bytes)                    (loader/presentation_loader.rs)
        │  ZIP local-file header  ──► PPTX path (unchanged)
        │  "%PDF-" magic within first 1 KiB ──► SourceFormat::Pdf
        │  else                              ──► existing typed errors
        ▼
PresentationLoader::load ──► DeckSource enum
        ├── Pptx(pptx_model::Presentation, Arc<dyn AssetStore>)
        └── Pdf(Arc<dyn PageSource>)
        ▼
Arc<dyn PageSource>                        (NEW contract, PRD 06 §2 delta)
        ▼
DefaultRenderPipeline::new(Arc<dyn PageSource>, …)   (constructor generalized)
```

New core contract (the single abstraction the whole feature hangs on):

```rust
pub trait PageSource: Send + Sync {
    fn len(&self) -> usize;
    fn page_size_emu(&self, index: usize) -> (i64, i64);   // per-page: mixed sizes legal
    fn compile_page(&self, index: usize, warnings: &Sender<WarningEvent>)
        -> Result<VectorSlide, RenderError>;
    fn notes(&self, _index: usize) -> Option<String> { None }   // PDF: None
    fn toc(&self) -> Vec<TocEntry> { Vec::new() }                // PDF: outline (Phase L)
    fn search_corpus(&self) -> Arc<dyn SearchCorpus> { … }       // §5.3
}
```

A thin adapter (`PptxPageSource`) wraps the existing `pptx_model::Presentation` + `AssetStore`, so the PPTX pipeline is a client of the same contract and `DefaultRenderPipeline` loses its `pptx_model` type dependency (≈1.1 k LOC pipeline unchanged in behavior; constructor + slide indexing go through the trait). PPTX keeps deck-level dimensions reported per page — uniform for real decks.

`VectorSlide` already carries per-slide `width_emu/height_emu`, so **mixed page sizes survive to the rasterizer and SVG backend**; the deck-level-dimension assumptions in the pipeline and TUI are the parts that must relax (§5).

### 3.2 Phase R — raster bridge (first landing)

```
PdfPageCompiler::compile_page(i)
        │  engine.render_rgba(i, canonical_scale)     (worker thread)
        │  PNG-encode → Arc<[u8]>
        ▼
VectorSlide { width_emu, height_emu, commands: vec![
    VectorCommand::DrawBitmap { png, width, height,
        placement: AffineTransform::identity() mapped into page EMU,
        opacity: 1.0, sampling: Smooth } ] }
        ▼
TinySkiaRasterizer (unchanged; downsamples bitmap into any RenderTargetSpec)
```

`DrawBitmap` needs no `AssetStore` and both backends already handle it (`rasterizer/image.rs::render_bitmap`; SVG base64 data URI). A whole PDF page is therefore expressible **with zero contract changes below `VectorSlide`**.

**Canonical resolution:** pages render once at `canonical_scale` chosen so page width ≈ 2,560 px (clamped 1,024–4,096; deterministic per page size). The tier-1 `VectorSlide` cache stays page-keyed; resize/zoom reuses the cached bitmap and re-rasterizes cheaply. A zoom target beyond 1.25× canonical triggers a **hi-res recompile** (Phase R2: tier-1 cache key widens to `(page, TargetClass::Std | HiDPI)`), mirroring how the metafile expansion re-runs per target class.

Known trade-off: Phase R is *raster-in-vector's-clothing* — resolution-bound until zoom recompile, SVG export embeds a PNG. The fidelity section labels it `DEGRADED(vector-out)` honestly; Phase V removes the label.

### 3.3 Phase V — vector bridge

`hayro-interpret::Device` implemented by `VectorCommandRecorder`, mapping interpreter calls onto the existing IR:

| `Device` call | `VectorCommand` / paint |
| :--- | :--- |
| `draw_path(path, props, mode)` | `DrawPath` (fill → `VectorPaint`, stroke → `VectorStroke`; gradients/shading → `VectorGradient`) |
| `push_clip_path` / `push_clip_rect` | `PushClip` |
| `pop_clip` | `PopClip` |
| `push_transparency_group(opacity, mask, blend, …)` | group opacity / soft mask → best-effort paint composition; unsupported modes → warning + approximate |
| `draw_glyph_run(run, props, mode)` | `DrawText` (`PositionedGlyphRun`) when the face resolves through `fontdb` by family; **else glyph outlines → `DrawPath`** (subset embedded fonts carry no usable family name — outline fallback is the correctness-preserving route; the `SvgTextMode::VectorGlyphs` machinery is reused) |
| `draw_image(image, props)` | inline `DrawBitmap` (decoded once, content-addressed) |

Page CTM composes with `initial_transform(true)` so rotated/cropped pages land correctly. Phase V restores full resolution independence and native vector SVG export for PDF pages through the unchanged serializer.

### 3.4 Module layout (new workspace crate `kitty-pptx-pdf`)

Keeps vendored crates pristine and honors the < 600-line rule ([AGENTS.md](../AGENTS.md) §A):

| File | Responsibility | Budget |
| :--- | :--- | :--- |
| `src/lib.rs` | Public re-exports, `DeckSource` integration entry | < 120 lines |
| `src/engine.rs` | `PdfEngine` trait, `HayroEngine` (open, page geometry, `RenderCache` pool) | < 300 |
| `src/detect.rs` | `%PDF-` magic sniff, version line parse, encryption pre-check | < 150 |
| `src/source.rs` | `PageSource` impl (`PdfPageSource`): page count, EMU sizes, compile dispatch | < 250 |
| `src/raster_bridge.rs` | Phase R: `render_rgba` → PNG → `DrawBitmap` slide; canonical scale; hi-res recompile key | < 300 |
| `src/vector_bridge/mod.rs` | Phase V: `VectorCommandRecorder` state machine (CTM, clip stack) | < 400 |
| `src/vector_bridge/fonts.rs` | glyph-run policy: family-resolve vs outline fallback; standard-14 substitution table | < 400 |
| `src/vector_bridge/images.rs` | image XObject decode → inline `DrawBitmap`, caching | < 300 |
| `src/links.rs` | Phase L: link annotations (URI + GoTo) → `SlideLink`; internal jumps → nav actions | < 250 |
| `src/text.rs` | Phase T: text extraction → `SearchCorpus` pages; TOC outline mapping | < 300 |
| `src/limits.rs` | resource limits (§8) | < 200 |
| `tests/` | detection, geometry, golden scene snapshots | — |

---

## 4. Coordinate Systems & Units

| Domain | Rule |
| :--- | :--- |
| PDF user space | points (1 pt = 1/72"); page origin bottom-left, y-up |
| EMU mapping | `emu = round(pt × 12_700)` — the existing `PT_PER_EMU`-family constant ([AGENTS.md](../AGENTS.md) §4) |
| Orientation | `render_dimensions()` / `intersected_crop_box()` already fold `page.rotation()` into a top-left, y-down pixel space; the bridge consumes those, never raw `MediaBox` |
| Slide-mode aspect | landscape pages aspect-fit via the unchanged `compute_slide_rect`; portrait pages in slide mode letterbox (correct, just unusual) |
| Reader mode | layout math in **canvas pixel space** derived from per-page EMU sizes and the live fit mode ([PRD 14a](./14a-pdf-reader-ui-designs.md) §5) |

Mixed page sizes are first-class: `page_size_emu(index)` is queried per page; nothing in the PDF path assumes a deck-level dimension.

---

## 5. Contract Delta (PRD 06)

1. **`PageSource`** (new, §3.1) — the deck abstraction. PPTX and PDF are interchangeable behind it (Liskov: presenter/export/dump depend only on the trait).
2. **`PresentationLoader`** — returns `DeckSource`; `sniff_container` gains the `%PDF-` branch. CFB/OLE2 and non-archive rejections unchanged.
3. **`DefaultRenderPipeline`** — constructor takes `Arc<dyn PageSource>`; `PptxPageSource` adapter preserves current behavior byte-for-byte (golden-verified).
4. **`SearchCorpus`** (new):

   ```rust
   pub trait SearchCorpus: Send + Sync {
       fn page_count(&self) -> usize;
       fn page_text(&self, page: usize) -> &[TextLine];   // y-ordered, EMU boxes
   }
   ```

   Replaces the TUI's `with_slides(Vec<pptx_model::Slide>)` dependency for find/search; PPTX implements it from slide runs, PDF from `text.rs` extraction. Media/comments simply report empty for PDF decks.
5. **`PipelineCache` tier-1 key** — `usize → (usize, TargetClass)` when Phase R2 lands; documented here so the cache change is not a surprise.
6. **`PipelineEntry`** — `Failed { … }` gains a PDF error class mapping (§9); `NotLoaded` prefetch windows extended by [PRD 14a](./14a-pdf-reader-ui-designs.md) §8.

---

## 6. Fidelity Policy (PRD 02d delta)

| PDF feature | Class | Behavior |
| :--- | :--- | :--- |
| Path fills/strokes, clips, images, text w/ resolvable fonts | **FULL** | native vector (Phase V); Phase R renders whole-page raster |
| Transparency groups (isolated, non-knockout), soft masks | **FULL/DEGRADED** | compose where the paint model allows; otherwise flatten approximate + warning |
| Knockout groups, CMYK blending spaces, DeviceN | **DEGRADED** | composite approximation + `WarningEvent::PdfFeatureApproximated` |
| Subset embedded fonts | **DEGRADED(text-outlines)** | glyphs as paths (correct geometry, non-selectable text in SVG unless Phase V text pass succeeds) |
| 3D content, embedded multimedia, U3D, rich-media annotations | **OMITTED** | labeled placeholder + warning |
| JavaScript, launch actions, file attachments | **OMITTED** | ignored + warning (security posture) |
| Encrypted documents | **ERROR** | typed `PdfError::EncryptedUnsupported` at load (§9) |
| Malformed xref/streams | **DEGRADED** | engine recovery or page-level `Failed` placeholder — never a crash |

Speaker notes: absent by definition; presenter console shows "— no notes (PDF)".

---

## 7. CLI Integration (PRD 05 delta)

| Subcommand | PDF behavior |
| :--- | :--- |
| `view deck.pdf -n 3` | works: compile → rasterize → stage/present (scrollback path unchanged) |
| `deck.pdf` (default present) | mode per §7.1 |
| `export deck.pdf --format svg` | Phase R: standalone SVG wrapping the page raster; Phase V: native vector SVG |
| `export --format png` | unchanged pipeline |
| `dump --format text/markdown/json` | Phase T extraction; JSON includes per-page boxes |
| `info deck.pdf` | page count, version, per-page sizes, encrypted flag, title/author metadata |
| stdin (`-`) | magic sniff makes `cat doc.pdf | kitty-pptx -` work without flags |

### 7.1 Mode selection (`--mode auto`, default)

The classifier is specified **normatively in [PRD 14a](./14a-pdf-reader-ui-designs.md) §2** (deterministic, page-metadata-only): a deck is **slides** when ≥ 90 % of pages are effectively landscape (post-`/Rotate` crop-box orientation), ≥ 90 % share the median landscape aspect within 2 %, and the median aspect is ≥ 1.25; everything else — including any **portrait-majority** document — opens in **reader** mode. Mid-document rotated pages never flip the decision (effective-orientation majority rules) and render letterboxed (slides) or contained-in-slot (reader) with no special cases. `--mode slides|reader` always overrides; the decision and its evidence print once at startup.

---

## 8. Resource Limits, Security & Determinism

- **Rendering bounds**: per-axis raster cap 8,192 px (fit-scale clamps; hi-res recompile refuses beyond cap and pins to cap + warning).
- **Time budget**: page render deadline (configurable, default 10 s) enforced around `interpret`/`render`; overrun → page-level `Failed` placeholder + warning, deck keeps running ([AGENTS.md](../AGENTS.md) §D robustness).
- **Memory**: `RenderCache` bounded per worker; decoded image XObjects content-addressed and capped; canonical-resolution policy (§3.2) bounds the steady-state footprint (kitty's own image quota — 320 MB/buffer — is protected by the resident-id discipline, [PRD 03a](./03a-kitty-graphics-protocol-driver.md)).
- **Hostile input**: all engine work on pipeline worker threads; panics caught at the worker boundary already map to `PipelineEntry::Failed`. Hayro's own hardening (timeouts/OOM) is immature upstream — our limits are the outer defense; fuzz targets from the hayro corpus feed PRD 07's fuzz level.
- **Determinism**: canonical scale derived only from page geometry; identical bytes → identical `VectorSlide` (same contract as PRD 13 §1.2).
- **No network, no exec**: link annotations route through the existing `OpenExternal` consent path (PRD 10); launch/file actions never execute.

---

## 9. Error Handling Matrix

| Condition | Error | User-visible result |
| :--- | :--- | :--- |
| Not a PDF / not a PPTX / corrupt | existing `ParseError` variants | unchanged startup failure |
| `%PDF-` magic but truncated/circular xref | `PdfError::Malformed { page }` | page placeholder + warning; deck opens |
| Encrypted (any R except empty-user) | `PdfError::EncryptedUnsupported` | typed startup error with hint |
| Page render deadline / OOM guard | `PdfError::BudgetExceeded` | page `Failed` card + warning |
| Zero pages | `ParseError::Empty` | startup error |

---

## 10. Milestones

| ID | Scope | Exit criteria |
| :--- | :--- | :--- |
| **P0** | Contracts: `PageSource`, `DeckSource`, loader sniff, `PptxPageSource` adapter; pipeline constructor generalization | all existing tests green; zero behavior change for PPTX (goldens byte-identical) |
| **P1** | Phase R: `kitty-pptx-pdf` crate, `HayroEngine`, raster bridge, slide-mode presenting of landscape decks | beamer deck + PowerPoint-exported deck present fullscreen/presenter/sorter; `< 120 ms` p95 to first page (PRD 00 NFR, warm cache) |
| **P2** | Reader mode (with [PRD 14a](./14a-pdf-reader-ui-designs.md)) for portrait/mixed documents | scroll/spread/fit NFRs of 14a met |
| **P3** | Phase V vector bridge + native SVG export; fidelity matrix wired | vector-scene goldens (L2a) for PDF corpus; SVG text selectable where fonts resolve |
| **P4** | Phase L + T: outline → TOC modal, link annotations → `SlideLink`/link picker, `SearchCorpus` find/search, `dump text/markdown` | parity with PPTX feature surface |
| **P5** | Hardening: parity harness vs external reference (pdftoppm/pdfium corpus, DSSIM-scored, cache-gated like `external_libreoffice_renderer_comparison`), fuzz, NFR measurement | regression suite merged into PRD 07 regime |

Dependency order is strict: P0 → P1 → P2 → (P3 ∥ P4) → P5.

---

## 11. Risks & Mitigations

| Risk | Mitigation |
| :--- | :--- |
| `hayro` pre-1.0 API churn (0.9 → 1.x) | pin `=0.9.x`; all contact behind `PdfEngine`; vendored-patch policy if a fix is needed (PRD 13 §2.2 precedent) |
| Engine robustness on hostile PDFs | worker isolation + deadlines + axis caps (§8); page-level degradation, never process death |
| Subset-font text quality in Phase V | outline fallback is correctness-preserving; family resolution is opportunistic only |
| Full-page raster memory at canonical scale | 2,560-px-wide RGBA ≈ 10–34 MB/page; tier-1 cache budget already 64 MB with oldest-first eviction; canonical clamp bounds it |
| Mode heuristic surprises users | heuristic printed at startup; explicit `--mode` always wins |
| Scope creep toward a general PDF editor | non-goals §1.3; this PRD covers *viewing/presenting/exporting* only |

---

## 12. Open Questions

1. Text extraction source in Phase T: `hayro-interpret` glyph-run capture vs `pdf_oxide` as a second dependency — decide at P4 with corpus quality data (both remain behind `SearchCorpus`).
2. Should Phase V text prefer `TextTags` SVG mode (invisible selectable text over outlines) for PDF pages? Matches the existing PPTX selectable-export UX.
3. Password-protected PDFs: prompt-less typed error at landing; interactive prompt in the TUI later?
4. PDF page labels ("iv, 1, 2…") — display alongside physical indices in reader status bar and TOC?
5. Kitty scrollback `view` mode for PDFs: stage whole pages as `ScrollbackBlock` (already supported) vs viewport-accurate bands?

---

## 13. References

- hayro repository & README (crates: `hayro-syntax`, `hayro-interpret`, `hayro`, `hayro-svg`), MSRV 1.92, Apache-2.0 OR MIT — https://github.com/LaurenzV/hayro
- Kitty graphics protocol (source-rect placements `x,y,w,h`; re-placement without retransmission) — https://sw.kovidgoyal.net/kitty/graphics-protocol/
- Prior art, terminal PDF readers over Kitty graphics: termpdf.py (fit-width/fit-page, scrolling, two-page spread), treader, kitpdf, pdfterm (PDFium), MeowPDF (continuous scroll)
- pdf_oxide (text-extraction benchmarks & corpus) — https://github.com/yfedoseev/pdf_oxide
- PDF 2.0 specification (ISO 32000-2) for coordinate/annotation semantics referenced by the engine layers
