# [Issue #40] DOCX input support: problem statement, crate evaluation, and implementation design

> **Issue:** [#40](https://github.com/ahati/office-terminal/issues/40)
> **Status:** design proposal — no implementation yet.
> **Scope:** read-only DOCX input for `view`, `present`, `export`, `info`, and `dump`.
> **Estimated effort:** 1–3 day spike; ~4–6 weeks for a vector MVP; multi-month for reference parity.
> **Related:** [PRD 14 — PDF input](./../14-pdf-input-support.md), [PRD 14a — reader UI](./../14a-pdf-reader-ui-designs.md), [PRD 02d — fidelity policy](./../02d-element-renderers-and-fidelity-policy.md).

---

## 1. Summary

Today Office Terminal (`ot` / `office-terminal`) opens `.pptx` and `.pdf`. A `.docx` is rejected as a malformed PPTX because the loader treats every OOXML ZIP as a presentation (ZIP magic → `SourceFormat::Pptx`).

DOCX support is technically feasible. **Parsing is easy; layout is the hard part.**

- The upstream project already ships a DOCX parser and a rich document model in Rust (~62k lines). It compiles against our vendored `ooxml-common` after two small changes. But its **pagination/layout engine is TypeScript-only** (~66.6k lines of layout plus ~15.3k lines of line breaking).
- `rdocx` 0.17.0 is a mature pure-Rust DOCX implementation with a 63k-line layout engine. It renders ordinary documents well, but it diverges on some Word layout rules we tested (notably `w:docGrid` interaction with automatic line spacing).
- `zavora-docx` 0.1.4 is a fork of an old `rdocx`. Its 5.9k-line layout engine is not adequate for this project's fidelity bar.

**Recommendation:** take the `rdocx` route for an MVP, behind a `DocxEngine` trait and a direct `PageFrame → VectorSlide` bridge. Keep the upstream parser/layout port as the long-term parity path. Reject `zavora-docx`.

A 1–3 day throwaway spike — DOCX → PDF via `rdocx` → existing `PdfPageSource` — can validate the end-to-end UX before committing to the vector bridge.

---

## 2. Problem statement

### 2.1 Current behavior

`sniff_container_format` in `crates/office-terminal-core/src/loader/presentation_loader.rs` accepts any ZIP local-file header as `SourceFormat::Pptx`. A `.docx` therefore reaches `pptx_parser` and fails with an OOXML presentation error such as a missing `ppt/presentation.xml`, or worse, a confusing part-level error.

`SourceFormat` and `DeckSource` have exactly two variants:

```rust
pub enum SourceFormat { Pptx, Pdf }

pub enum DeckSource {
    Pptx(pptx_model::Presentation, Arc<dyn AssetStore>),
    Pdf(Arc<dyn PageSource>),
}
```

The UI and CLI have no concept of a word-processing document. Reader mode exists (introduced for PDF), but DOCX cannot reach it.

### 2.2 User impact

- A user who receives a `.docx` cannot view, search, or export it from the terminal.
- The error message is misleading: the file is treated as a corrupt presentation rather than an unsupported-but-valid document.
- `.docm`, `.dotx`, `.doc`, `.rtf`, and `.xlsx` all produce poor diagnostics.

### 2.3 Why now

The PDF work already generalized the render substrate:

- `PageSource` abstracts "a page-indexed document with per-page dimensions".
- `DefaultRenderPipeline` already runs on `Arc<dyn PageSource>`.
- Reader mode handles mixed page sizes, scrolling, page panes, thumbnails, and search.
- `PptxPageSource` and `PdfPageSource` prove the adapter pattern.

DOCX is the natural third input. Before PRD 14, adding it would have required re-plumbing the entire pipeline; now the remaining work is a document engine, an IR bridge, and format detection.

---

## 3. Goals and non-goals

### 3.1 Goals

1. `.docx` files open in `view` and `present` with the same terminal UX as PDF documents: reader mode by default, page panes, zoom/scroll, thumbnails, search.
2. `export` produces PNG and native vector SVG pages with selectable text where the backend supports it.
3. `info` reports page count, per-page dimensions, TOC, and media count.
4. `dump --markdown`, `dump --json`, and `dump --notes` / `--comments` degrade gracefully.
5. Format auto-detection by OPC package content, not filename.
6. Every unsupported DOCX construct produces a typed warning or placeholder — never a crash, never an invisible hole.
7. Pure Rust, no C/C++ toolchain, no LibreOffice/Pandoc subprocess.
8. Resource limits on hostile input, consistent with the PDF path.
9. No regressions in PPTX or PDF behavior.
10. Zero compiler and Clippy warnings under the existing workspace policy.

### 3.2 Non-goals

- DOCX editing, saving, round-tripping, or template replacement.
- Macro execution (`.docm` may be viewed; macros are never executed).
- Form filling, digital signatures, or document protection bypass.
- OCR of scanned DOCX images.
- Pixel-identical Microsoft Word output. The target is a documented fidelity class per PRD 02d, not engine emulation.
- Full tracked-changes UI in v1. The engine supports revision views; exposing them is optional follow-up.
- Comment balloons / margin annotations in the TUI. Comments may be exposed by `dump` only.
- Replacing the PPTX renderer or the PDF engine.

---

## 4. Existing architecture this builds on

```
                         load_deck(Source)
                                │
                    sniff_container_format(&bytes)
             ┌──────────────────┼──────────────────┐
             │                  │                  │
          PPTX ZIP           DOCX ZIP            %PDF-
             │                  │                  │
     pptx_parser::parse   DOCX_LOADER hook    PDF_LOADER hook
             │                  │                  │
   DeckSource::Pptx     DeckSource::Docx     DeckSource::Pdf
             │                  │                  │
             └──────────── Arc<dyn PageSource> ────┘
                                │
                   DefaultRenderPipeline
                  compile_page / render_raster
                                │
                          VectorSlide
                     ┌──────────┴──────────┐
                 tiny-skia               SVG
```

Key seams already present:

| Seam | Location | Relevance |
|---|---|---|
| `PageSource` | `crates/office-terminal-core/src/contracts/deck.rs` | DOCX plugs in unchanged. |
| `DeckSource` | same file | Needs a `Docx` variant. |
| Loader hook | `loader/presentation_loader.rs` | Same `OnceLock` pattern as PDF. |
| Reader mode | `crates/office-terminal-tui/src/app/reader_*` | DOCX is a document; reuse. |
| `SearchCorpus` | `contracts/deck.rs` | Extract per-page text from layout. |
| `TocEntry` | `contracts/deck.rs` | Map DOCX headings. |
| `AssetStore` | `loader/asset_store.rs` | Back DOCX media from layout bytes. |
| `VectorSlide` / `VectorCommand` | `contracts/vector_slide.rs` | Target IR; no PPTX types below this line. |
| `WarningEvent` / `FidelityClass` | `contracts/pipeline.rs` | Honest degradation. |

What is missing:

- DOCX format detection.
- A DOCX document/layout engine in Rust.
- A `PageFrame → VectorSlide` bridge.
- Exact-face font plumbing for externally shaped glyph IDs.
- CLI/TUI match arms and document-mode defaults.

---

## 5. Candidate evaluation

### 5.1 Evaluation method

Criteria: license/supply chain, parsing coverage, layout coverage, Rust integration (API, output IR, fonts, limits, MSRV, deps), visual result on the upstream `sample-1.docx` (6 reference pages) and ordinary documents, and whether it builds against this repo. The upstream reference PNGs are the reference renderer's adopted output, not a Word export, but they are the best available parity target.

### 5.2 Candidate A — upstream `office-open-xml-viewer` DOCX (vendor parser/model, port layout)

**What exists**

| Component | Language | Size (non-test) |
|---|---|---|
| `packages/docx/model` + `packages/docx/parser` | Rust | 61,963 lines |
| `packages/docx/src/layout` | TypeScript | 66,587 lines |
| `packages/docx/src/line-breaker` | TypeScript | 15,306 lines |
| all `packages/docx/src` | TypeScript | 114,120 lines |

The Rust side is a rich parsed model with resolved style inheritance, numbering, section geometry, headers/footers, notes, anchors, VML, charts, and math. The TypeScript side performs pagination, line breaking, table splitting, float wrap, notes placement, and paint.

**Compile probe (performed, then reverted).** Copying `packages/docx/model` + `packages/docx/parser` into `crates/` and running `cargo check -p docx-parser` against the **vendored** `ooxml-common` (`4d6828b`) produced exactly two errors:

- Missing `ooxml_common::content_types::ImagePartTypes`.
- `docx-model` declares `ooxml-common/schema`, absent from our trimmed vendored copy.

After adding `ImagePartTypes` (~50 lines, self-contained upstream) and declaring the optional `schema` feature (plus its optional `schemars` dependency), **`docx-parser` compiles cleanly**. No semantic API drift was found; the parser/model are effectively a drop-in vendor.

**Why it is not the MVP.** There is no Rust layout engine to vendor. Porting 66.6k lines of layout plus 15.3k lines of line breaking is multi-month. A plain-body-text subset is ~5–10k lines; tables/numbering/headers/footers/footnotes add ~10–15k; full parity is the whole 80k+.

**Pros:** best fidelity ceiling; parser already compiles; MIT; same upstream as our vendored PPTX stack; reuses the `ooxml-common` resource governor (`OoxmlFormat::Docx` exists).

**Cons:** layout must be written/ported; vendoring adds 62k lines and a 37k-line `parser.rs`, requiring a documented vendored-crate exception to the 600-line rule (already the norm for `pptx-parser`); no short-term payoff.

**Verdict:** long-term parity path; do not start here.

### 5.3 Candidate B — `rdocx` 0.17.0

**What it is**

A mature pure-Rust WordprocessingML library (the original project that `zavora-docx` forked). Current version 0.17.0, edition 2024, MSRV 1.93, MIT (`LICENSE`; Cargo metadata `MIT OR Apache-2.0`). Repository commit tested: `eb980e94c8ae1f8b42612f918482bc627e0141a0` (2026-11-09).

**Size**

| Crate | Lines |
|---|---|
| `rdocx-layout` | 63,218 |
| `oxml-layout` (shared page/layout IR) | 14,243 |
| `rdocx-oxml` | 150,354 |
| high-level `rdocx` | 248,313 |

It uses `harfrust` (HarfBuzz port) for shaping, `fontdb 0.23`, `hypher`, `icu_segmenter`, `unicode-bidi`, and `unicode-linebreak`. It bundles metric-compatible fonts (Carlito, Caladea, Gelasio, Liberation) and supports deterministic layout.

**Public API relevant to us**

```rust
let doc = rdocx::Document::from_bytes(bytes)?;
let layout: Arc<rdocx_layout::WordLayoutResult> = doc.layout_deterministic()?;
let pages: &[Arc<oxml_layout::PageFrame>] = &layout.layout.pages;
```

- `PageFrame { page_number, displayed_page_number, width, height, elements, background }`
- `PositionedElement::{Text, MultilingualText, Line, FilledRect, Image, LinkAnnotation, Path, Group, MarkedContent}`
- `LayoutResult { pages, fonts, metadata, outlines, diagnostics, structure }`
- `FontData { id, family, data: Arc<[u8]>, face_index, bold, italic, variations, synthetic_bold, synthetic_italic }`
- `WordLayoutResult::{bookmark_page_section, page_sections, field_placements}`
- `PackageReadLimits { max_entries, max_part_uncompressed_bytes, max_total_uncompressed_bytes }`

**Render results.** `rdocx-cli` 0.17.0 at 96 DPI produced **9 pages** for the 6-page reference. Page 1 kept the hero image, `CANOPY`, and `FEATURE`, but vertical spacing was wrong; both title lines moved to page 2 with a huge gap. rdocx-generated ordinary documents (cover-page report, proposal) rendered cleanly and looked professional, though those are self-authored samples rather than independent fixtures.

**Root cause found.** The sample section declares `<w:docGrid w:type="lines" w:linePitch="360"/>` and `<w:spacing w:line="840" w:lineRule="auto"/>`. Upstream/Word multiplies automatic spacing against the **grid pitch** (`18 pt × 3.5 ≈ 63 pt`); `rdocx` multiplies against the **natural font line height** (`≈52 pt × 3.5 ≈ 182 pt`, rounded to 11 grid rows `≈198 pt`). This is a genuine layout-rule divergence, fixable upstream, not just font substitution.

**Dependency footprint.** `cargo tree -p rdocx` reports **147 unique normal dependency names**. `fontdb 0.23` and `ttf-parser 0.25` add duplicates next to our `fontdb 0.16.2` and `ttf-parser 0.20/0.21/0.24`; `zip 9.0.3` with `deflate-flate2-zlib-rs` unifies with our `=9.0.4`, so ZIP is not duplicated. Full `rdocx-cli` release binary: ~39 MB (not the incremental cost inside office-terminal); release build ~3 minutes. `default-features = false` disables the system-font scan and uses deterministic bundled fonts. No C/C++ observed: pure-Rust `zlib-rs` and `fontconfig-parser`.

**Pros:** only candidate producing paginated, positioned output in Rust today; rich coverage (sections, tables, headers/footers, footnotes, floats, fields, bidi, East Asian, VML, charts, math, tracked changes); backend-neutral `oxml-layout` IR maps naturally to `VectorSlide`; deterministic bundled fonts; MIT; no subprocess; resource limits; isolatable behind a trait.

**Cons:** pre-1.0 API (pin `=0.17.0`); large dependency tree and duplicate font stacks; observed fidelity gaps (docGrid, anchored-frame spacing); high-level `rdocx` pulls PDF/HTML/chart crates unconditionally; exact-face font bridging required.

**Verdict:** best available MVP foundation.

### 5.4 Candidate C — `zavora-docx` 0.1.4

**What it is.** A fork of an old `rdocx` (README credits `tensorbee/rdocx`). Release 0.1.4, commit `2be0ad47f5454c16936664c60d625fffe2bb2d3f` (2026-08-25), MIT OR Apache-2.0. Sizes: `zavora-docx-layout` 5,919 lines, `zavora-docx-oxml` 12,679, high-level `zavora-docx` 7,231. It uses `fontdb 0.23`, `rustybuzz 0.20`, `ttf-parser 0.25`, `tiny-skia`, and `pdf-writer`; its workspace pins `zip = "8.1"` (a duplicate of our `9.0.4`).

**Render results.** Same reference at 96 DPI: page 1 lost the hero image, printed `&amp;` literally, and used a dashed rule; “The Living / Cathedral” moved to page 2; images and structural elements were missing. It does render paragraphs, tables, simple headers/footers, and basic anchoring.

**Verdict: not suitable.** Its layout is an order of magnitude smaller than `rdocx` and the result is clearly below this project's fidelity bar. Keep it only as a minimal API reference, not a dependency.

### 5.5 Other options considered

| Option | Assessment |
|---|---|
| `docx-rs` / `docx-rust` | Read/write models, no pagination or rendering. Insufficient. |
| `ooxmlsdk` | Low-level generated OOXML types, no layout. Insufficient. |
| LibreOffice / `soffice` subprocess | High fidelity, but violates the single-binary, no-external-runtime, pure-Rust posture; also complicates SSH/container distribution. Rejected. |
| Pandoc → HTML/PDF | Lossy for layout, external runtime, no page geometry. Rejected. |
| `docx2pdf` / Word automation | Requires Microsoft Word. Rejected. |
| Browser/JS DOCX viewer (upstream TS) embedded via a JS engine | Layout depends on Canvas text measurement and browser APIs; enormous runtime, non-deterministic. Rejected. |
| DOCX → PDF via `rdocx`, then existing PDF path | Excellent short-term spike; raster-first, double conversion. Recommended as Phase 0, not the final architecture. |

### 5.6 Comparison

| Criterion | Upstream parser + TS layout | `rdocx` 0.17 | `zavora-docx` 0.1.4 |
|---|---|---|---|
| Parsing completeness | ★★★★★ | ★★★★☆ | ★★☆☆☆ |
| Layout completeness | ★★★★★ (TS) | ★★★★☆ | ★★☆☆☆ |
| Rust-native layout | ✗ | ✓ | ✓ |
| Reference parity | target | close, known gaps | poor |
| Integration effort | months | weeks | days |
| External deps | none beyond vendoring | 147 crates | ~80 crates |
| License | MIT | MIT | MIT/Apache-2.0 |
| API stability | vendored | pre-1.0, pin exact | pre-1.0 |
| Resource limits | `ooxml-common` governor | `PackageReadLimits` | partial |
| Recommendation | long-term parity | **MVP now** | reject |

### 5.7 Reproducing the evaluation

```bash
# parser probe (reverted): copy model/parser into crates/, add to workspace,
# add ImagePartTypes + `schema` feature to ooxml-common, then:
cargo check -p docx-parser

# render comparison
cargo build -p rdocx-cli --release
./target/release/rdocx render sample-1.docx -o /tmp/rdocx_out --dpi 96
```

---

## 6. Decision

**Adopt `rdocx` 0.17.0 as the initial DOCX engine, wrapped behind a `DocxEngine` trait, with a direct `PageFrame → VectorSlide` bridge.**

Rationale:

1. It is the only candidate that produces paginated, positioned output in Rust today.
2. Its output IR is already backend-neutral and maps cleanly to `VectorSlide`.
3. It can be isolated: `DocxPageSource` only knows `DocxEngine`; a future native engine can replace `RdocxEngine` without touching the pipeline, CLI, or TUI.
4. It preserves the upstream parser/model option. If reference parity becomes mandatory, we can vendor the upstream parser and add a `NativeDocxEngine` that emits the same `oxml_layout` page IR.

**Reject `zavora-docx`.** **Do not port the upstream TypeScript layout in the MVP.** Phase 6 below is listed for context only; open a separate parity issue before starting it.

**Phase 0 spike (1–3 days, throwaway):** convert DOCX to PDF with `rdocx` and feed the bytes to `PdfPageSource`. This validates reader mode, search, export, and CLI ergonomics with almost no code. It is not the final design because it is raster-first and double-converts.

---

## 7. Design

### 7.1 Architecture

```
                    office-terminal-cli
                          │
             DeckSource::Docx(Arc<dyn PageSource>)
                          │
                office-terminal-docx
        ┌─────────────────┴─────────────────┐
        │                                   │
  DocxPageSource                     DocxAssetStore
  (PageSource impl)                  (media bytes → decoded)
        │
  ┌─────┴──────┐
  │            │
DocxEngine   bridge/
(trait)      PageFrame → VectorSlide
  │            │
RdocxEngine    ├─ text.rs
  │            ├─ path.rs
  │            ├─ paint.rs
  │            ├─ image.rs
  │            ├─ group.rs
  │            └─ link.rs
  │
rdocx 0.17 (external)
  ├─ Document::from_bytes
  ├─ layout_deterministic
  └─ WordLayoutResult.layout: oxml_layout::LayoutResult

office-terminal-core
  ├─ PageSource / VectorSlide / VectorCommand
  ├─ DocumentFace registry (new; exact external faces)
  ├─ rasterizer text/document path (new)
  └─ SVG document text path (new)
```

### 7.2 New crate

`crates/office-terminal-docx/` — single responsibility: **turn a DOCX package into a `PageSource`**. No CLI logic, no TUI logic, no PPTX types.

```
crates/office-terminal-docx/
├── Cargo.toml
└── src/
    ├── lib.rs              # re-exports + register()
    ├── error.rs            # DocxError, mapping helpers
    ├── detect.rs           # OPC main-part detection
    ├── engine.rs           # DocxEngine trait, RdocxEngine
    ├── source.rs           # DocxPageSource: PageSource
    ├── fonts.rs            # FontData → DocumentFace registration
    ├── media.rs            # DocxAssetStore
    ├── search.rs           # page glyph runs → TextLine corpus
    ├── cache.rs            # bounded IR cache
    ├── limits.rs           # package/page/image budgets
    └── bridge/
        ├── mod.rs          # compile_page entry point
        ├── page.rs         # page box, background
        ├── text.rs         # GlyphRun/MultilingualGlyphRun → PositionedGlyphRun
        ├── path.rs         # PathCommand → PathSegment, fill rules
        ├── paint.rs        # Paint/Gradient → VectorPaint
        ├── image.rs        # Image → DrawImage, media registration
        ├── group.rs        # Group/MarkedContent, transforms, clips, opacity
        ├── effects.rs      # Effect → ShapeEffects / warnings
        └── link.rs         # LinkAnnotation → SlideLink
```

Every file stays under 600 lines per `AGENTS.md`.

```toml
[package]
name = "office-terminal-docx"
version = "0.1.0"
edition = "2021"

[dependencies]
office-terminal-core = { workspace = true }
ooxml-common = { workspace = true }  # ResourceViolation for typed limits
oxml-layout = "=0.15.0"          # backend-neutral page/font/media types
rdocx = { version = "=0.17.0", default-features = false, optional = true }
thiserror = { workspace = true }
image = { workspace = true }

[features]
default = ["rdocx-backend"]
rdocx-backend = ["dep:rdocx"]
```

`rdocx` is optional so a future native engine, or a mock in tests, can build without it.

### 7.3 Format detection

`sniff_container_format` must stop treating every ZIP as PPTX. Detection should be authoritative from the OPC package, not the filename.

Proposed in `loader/presentation_loader.rs`: keep the existing `%PDF-`/CFB checks, then route ZIP magic to an OPC-aware resolver instead of returning `Pptx`:

```rust
fn detect_ooxml_format(bytes: &[u8]) -> Result<SourceFormat, ParseError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    // OPC main part may be relocated by _rels/.rels; check it first.
    if let Some(name) = opc_main_part(&mut zip)? {
        return main_part_format(&name);
    }
    for (part, format) in [
        ("word/document.xml", SourceFormat::Docx),
        ("ppt/presentation.xml", SourceFormat::Pptx),
    ] {
        if zip.by_name(part).is_ok() { return Ok(format); }
    }
    if zip.by_name("xl/workbook.xml").is_ok() {
        return Err(ParseError::ArchiveError("spreadsheet (.xlsx) input is not supported".into()));
    }
    Err(ParseError::ArchiveError(
        "OOXML package has no recognized main document part".into()))
}
```

`main_part_format` compares `word/document.xml` / `ppt/presentation.xml` case-insensitively and rejects anything else. Match ZIP entries case-insensitively too, since OPC part names are case-insensitive even though ZIP names are not. Check ZIP magic before scanning the first 1 KiB for `%PDF-`, so a stored PDF entry inside an OOXML package cannot misclassify it.

Rules:

| Input | Behavior |
|---|---|
| `.docx` / `.dotx` / `.docm` | `SourceFormat::Docx`; macros never executed. `.docm` may emit a warning. |
| `.pptx` / `.pptm` | `SourceFormat::Pptx` (unchanged). |
| `.xlsx` / `.xlsm` | Typed error: spreadsheet not supported. |
| Legacy `.doc` | CFB `WordDocument` stream → typed error: convert to `.docx`. |
| Encrypted OOXML | CFB `EncryptedPackage` → `ParseError::PasswordProtected` (existing path). |
| `.rtf`, `.odt`, random bytes | Typed error: unsupported document format. |
| Extension mismatch | Ignored; content wins. A `.pptx`-named DOCX opens as DOCX. |

No extraction to disk.

### 7.4 Contracts and loader changes

Minimal contract delta:

```rust
// contracts/deck.rs
pub enum SourceFormat { Pptx, Pdf, Docx }

pub enum DeckSource {
    Pptx(pptx_model::Presentation, Arc<dyn AssetStore>),
    Pdf(Arc<dyn PageSource>),
    Docx(Arc<dyn PageSource>),
}
```

Do **not** rename `Pdf` to `Paged` in this PR; that is a separate cleanup. The CLI treats `Pdf | Docx` identically in most match arms. Update every exhaustive `SourceFormat`/`DeckSource` match (`sniff_container`, `view`, `present`, `export`, `info`, `dump`).

Loader hook, mirroring PDF:

```rust
// loader/presentation_loader.rs
pub type DocxLoaderFn = fn(Arc<[u8]>) -> Result<Arc<dyn PageSource>, ParseError>;
static DOCX_LOADER: OnceLock<DocxLoaderFn> = OnceLock::new();

pub fn register_docx_loader(loader: DocxLoaderFn) {
    let _ = DOCX_LOADER.set(loader);
}

// in load_deck:
SourceFormat::Docx => {
    let loader = DOCX_LOADER.get().ok_or_else(|| {
        ParseError::ArchiveError("DOCX engine is not registered or linked".to_string())
    })?;
    Ok(DeckSource::Docx(loader(bytes)?))
}
```

Registration in `crates/office-terminal-cli/src/runner.rs`:

```rust
pub fn run() -> ExitCode {
    office_terminal_pdf::register();
    #[cfg(feature = "docx")]
    office_terminal_docx::register();
    // ...
}
```

`office_terminal_docx::register()`:

```rust
pub fn register() {
    office_terminal_core::loader::presentation_loader::register_docx_loader(|bytes| {
        DocxPageSource::open(bytes)
            .map(|src| Arc::new(src) as Arc<dyn PageSource>)
            .map_err(|e| match e {
                DocxError::Encrypted => ParseError::PasswordProtected,
                DocxError::ResourceLimit(v) => ParseError::ResourceLimit(v),
                other => ParseError::ArchiveError(other.to_string()),
            })
    });
}
```

### 7.5 `DocxEngine` trait

The pipeline must not depend on `rdocx`. Define a small, mockable seam:

```rust
// engine.rs — the only type the bridge/pipeline knows
pub trait DocxEngine: Send + Sync {
    fn page_count(&self) -> usize;
    fn page(&self, index: usize) -> Option<Arc<PageFrame>>;
    fn fonts(&self) -> &[FontData];
    fn media(&self, id: MediaId) -> Option<MediaBytes>;
    fn outlines(&self) -> Vec<OutlineEntry>;
    fn bookmark_page(&self, name: &str) -> Option<usize>; // 0-based physical
    fn diagnostics(&self) -> &[String];
}

pub struct MediaBytes { pub data: Arc<[u8]>, pub content_type: String }
```

`RdocxEngine` holds only `Arc<WordLayoutResult>` and a media map. It does **not** need to retain `rdocx::Document` after layout, which avoids Send/Sync and lifetime questions. (Confirm `WordLayoutResult` is `Send + Sync`; its data is plain/Arc. If a field is not, wrap the engine in a mutex or extract owned pages/fonts/media into our own structs.)

A future `NativeDocxEngine` can emit the same `oxml_layout::PageFrame` values from the upstream parser/model. The bridge and `DocxPageSource` stay unchanged. Tests use a `FixtureDocxEngine` that returns hand-built `PageFrame`s.

### 7.6 `DocxPageSource`

```rust
pub struct DocxPageSource {
    engine: Arc<dyn DocxEngine>,
    media: Arc<DocxAssetStore>,
    ir_cache: IrCache,                    // Mutex<LruMap<usize, Arc<VectorSlide>>>
    corpus: OnceLock<Arc<DocxSearchCorpus>>,
    warnings_emitted: Mutex<HashSet<&'static str>>,
}

impl DocxPageSource {
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, DocxError> {
        let engine = RdocxEngine::open(bytes, &DocxLimits::default())?;
        let media = Arc::new(DocxAssetStore::from_engine(&engine));
        Ok(Self::new(Arc::new(engine), media))
    }

    pub fn from_engine(engine: Arc<dyn DocxEngine>) -> Self { /* ... */ }
}

impl PageSource for DocxPageSource {
    fn len(&self) -> usize { self.engine.page_count() }

    fn page_size_emu(&self, index: usize) -> (i64, i64) {
        let (w, h) = self.engine.page(index).map(|p| (p.width, p.height)).unwrap_or((0.0, 0.0));
        (pt_to_emu(w), pt_to_emu(h))
    }

    fn compile_page(
        &self,
        index: usize,
        warnings: &Sender<WarningEvent>,
    ) -> Result<VectorSlide, RenderError> {
        if index >= self.len() {
            return Err(RenderError::CompilationFailed(format!(
                "DOCX page {} out of bounds (total: {})", index, self.len()
            )));
        }
        self.ir_cache.get_or_compile(index, || {
            let page = self.engine.page(index)
                .ok_or_else(|| RenderError::CompilationFailed(format!("DOCX page {index} missing")))?;
            bridge::compile_page(&page, self.engine.as_ref(), self.media.as_ref(), warnings)
        })
    }

    fn notes(&self, _index: usize) -> Option<String> { None }

    fn toc(&self) -> Vec<TocEntry> {
        self.engine.outlines().into_iter().map(|o| TocEntry {
            title: o.title,
            page_index: o.page_index,
            level: o.level as usize,
        }).collect()
    }

    fn search_corpus(&self) -> Arc<dyn SearchCorpus> {
        Arc::clone(self.corpus.get_or_init(|| Arc::new(DocxSearchCorpus::build(self.engine.as_ref()))))
    }

    fn asset_store(&self) -> Option<Arc<dyn AssetStore>> {
        Some(Arc::clone(&self.media) as Arc<dyn AssetStore>)
    }
}
```

Notes:

- `render_raster` stays at the default `None`: DOCX pages are vector; the pipeline's compile→rasterize path is correct.
- `compile_page` should be cheap enough to skip an IR cache, but prefetch at multiple terminal sizes re-compiles the same page. A bounded LRU (e.g., 16 pages / 32 MB) avoids repeated work. The cache is keyed by page index only; `VectorSlide` is resolution-independent.
- Emit engine diagnostics once per document, not once per page.
- `DocxPageSource` must be `Send + Sync`; `Arc<dyn DocxEngine>` + `Mutex` caches satisfy that.

### 7.7 `PageFrame → VectorSlide` mapping

Entry point:

```rust
pub fn compile_page(
    page: &PageFrame,
    engine: &dyn DocxEngine,
    media: &DocxAssetStore,
    warnings: &Sender<WarningEvent>,
) -> Result<VectorSlide, RenderError> {
    let mut slide = VectorSlide::new(pt_to_emu(page.width), pt_to_emu(page.height));
    slide.background = page.background.as_ref().and_then(|p| map_background(p, media, warnings));
    map_elements(&page.elements, Transform::IDENTITY, &mut slide, engine, media, warnings)?;
    Ok(slide)
}
```

#### 7.7.1 Coordinates and units

`oxml_layout` geometry is in **typographic points**; `VectorSlide` is **EMU** (`1 pt = 12,700 EMU`). Convert once in f64 and round; never accumulate rounded values. Coordinates inside a `Group` are child-local: keep them and carry the accumulated transform on the leaf command. Page backgrounds are page-space. `PageFrame.width/height` are already rotated/cropped. Mixed page sizes are supported by `PageSource` and the reader.

#### 7.7.2 Page background

| `PageFrame.background` | `VectorSlide.background` |
|---|---|
| `Paint::Solid(c)` | `VectorBackground::Solid(rgba(c))` |
| `Paint::Linear { start, end, stops, .. }` | `VectorBackground::Gradient` (convert direction to `angle_deg`, stops) |
| `Paint::Radial { .. }` | `VectorBackground::Gradient` with an approximate linear angle, or solid average; warning if approximated |
| `Paint::Tile { image, tile, transform }` | `VectorBackground::Image` with media `rel_id`; map `tile`/`transform` to `BackgroundTile` where possible, else stretch |
| `Paint::Grid { .. }` | Rasterize to a small texture if cheap, else average color; warning |
| `None` | `None` (renderer paints its normal transparent/white base) |

#### 7.7.3 Text

`GlyphRun` (monolingual):

```text
origin: Point            // baseline of the first glyph, in points
font_id: FontId          // index into engine.fonts()
font_size: f64           // points
glyph_ids: Vec<u16>
advances: Vec<f64>       // points, one per glyph
text: String             // logical text for selectable SVG/search
color: Color             // 0..1 floats
bold, italic: bool
```

Map to one `PositionedGlyphRun`: `font_family` from `FontData.family`, `font_size_pt`, normalized `color`, `bold`/`italic`, `text`; glyph `i` at `x = origin.x + Σ_{j<i} advances[j]`, `y = origin.y`, `advance = advances[i]`, all converted to EMU once. `decorations` is empty because rdocx emits underline/strike as separate `Line` elements; `reflection` is `None`; `face = Some(DocumentFaceId)`; `single_face = false` (the face path takes precedence).

`MultilingualGlyphRun` adds per-glyph two-axis positioning (`x_advances`, `y_advances`, `x_offsets`, `y_offsets`, points), `clusters`, `logical_text`, `direction`, and `bidi_level` (visual order is already applied). Accumulate `x_advances[i]` and add offsets; use `logical_text` for `text`; preserve `face`. Ligatures/combining marks can make glyph count differ from cluster count: glyph IDs remain authoritative for rendering, while `text` serves search/selectable SVG; `VectorGlyphs` uses exact outlines and is unaffected. Precompute a dense `Vec<Option<FaceInfo>>` indexed by `FontId.0` for O(1) lookup.

#### 7.7.4 Paths, paints, strokes

`PathCommand → PathSegment`:

| `oxml_layout::PathCommand` | `office_terminal_core::PathSegment` |
|---|---|
| `MoveTo(p)` | `MoveTo { x, y }` |
| `LineTo(p)` | `LineTo { x, y }` |
| `CurveTo { c1, c2, to }` | `CubicTo { x1, y1, x2, y2, x, y }` |
| `Close` | `Close` |

- `FillRule` maps directly.
- `Stroke { paint, width, cap, join, dash }`: width points → `width_emu`; cap/join direct; dash lengths are absolute points, while `VectorStroke.dash_segments` uses **width multiples** — divide by the width in points and drop the dash if width is zero; use miter limit `10.0`, `compound: Sng`, no head/tail ends; solid paint → `color`, gradient/pattern → `paint: Some(Box<VectorPaint>)`.

| `oxml_layout::Paint` | `VectorPaint` |
|---|---|
| `Solid(c)` | `Solid(rgba(c))` |
| `Linear { start, end, stops, extend }` | `LinearGradient` (angle + stops) |
| `Radial { center, radius, focal, stops }` | `RadialGradient`; approximate focal if unrepresentable + warning |
| `Tile { image, tile, transform }` | `Image`/`Texture` with media bytes; preserve repeat/stretch |
| `Grid { .. }` | Rasterize to a small texture and use `Texture`; warning |

`PositionedElement::Line` and `FilledRect` map to `DrawPath` with an open segment / solid fill. Skip degenerate zero-length paths with a debug warning.

**Transforms.** `GroupElement.transform` maps child-local to parent; compose the accumulated transform. Carry it on each `DrawPath`/`DrawText`/`DrawImage` rather than baking it into every point (preserves gradient orientation and text rotation). `PushClip` has no transform field, so pre-transform the clip path into page space. Test rotated/mirrored groups explicitly; text must use the affine path.

#### 7.7.5 Images

`PositionedElement::Image { rect, data, content_type, media_id }`:

- Register `data` under `rel_id = format!("docx-media-{:016x}", media_id.0)` and emit `DrawImage { rel_id, src_rect: full, dest_rect: rect in EMU, opacity: 1.0, transform: accumulated, effects: empty }`.
- Metafiles (`image/x-emf`, `image/x-wmf`, `image/emf`, `image/wmf`): route through `office_terminal_core::metafile` playback to paths, or the standard placeholder on failure. Do not hand them to `image`.
- SVG: the core asset path has `resvg` support; decode/render at the size hint.
- External images: never fetched; placeholder + `WarningEvent::MediaFailed`.
- Deduplicate by `MediaId`.

#### 7.7.6 Groups, clips, opacity, effects

`PositionedElement::Group(GroupElement)`:

```rust
pub struct GroupElement {
    pub transform: Transform,
    pub clip: Option<Path>,
    pub opacity: f64,
    pub effects: Vec<Effect>,
    pub children: Vec<PositionedElement>,
}
```

1. Compose `parent_transform * group.transform`.
2. Clip: transform to page space, emit `PushClip`/recurse/`PopClip`.
3. `opacity < 1.0`: no group-opacity command exists. Multiply alpha into descendant paint/image opacity and warn `FidelityDegraded` (approximate for overlapping children; a future `PushOpacity` can fix it).
4. `Effect::OuterShadow`: map `dx`/`dy`/`blur` from points to EMU, with `direction_deg = atan2(dy, dx)`, `distance_emu = hypot(dx, dy)`, and the given color, onto `ShapeEffects::outer_shadow` for the single path child; otherwise attach to the topmost path and warn. Group-level shadow geometry is not representable.
5. `Effect::LinearOpacityMask`: not representable; drop with a warning (future mask command can reuse the rasterizer's mask machinery).
6. `MarkedContent`: recurse; `structure` is tagged-PDF metadata, ignored for rendering.

`PositionedElement` is `#[non_exhaustive]`; always include a wildcard warning arm.

#### 7.7.7 Links

`PositionedElement::LinkAnnotation { rect, url }`:

- External URL: `sanitize_hyperlink_url(&url)` from `contracts/hyperlink.rs`; on success emit
  ```rust
  SlideLink {
      target: HyperlinkTarget::ExternalUri(sanitized),
      bounds_emu,
      tooltip: None,
  }
  ```
- Fragment (`#bookmark` or a bare bookmark name, depending on how the engine projects `w:anchor`): resolve via `DocxEngine::bookmark_page(name)` → `HyperlinkTarget::SlideIndex(page)`; if unresolved, drop with a warning.
  - Define `DocxEngine::bookmark_page` as returning a **0-based physical page index**. `rdocx` exposes `WordLayoutResult::bookmark_page_section(name)` with 1-based `physical_page`; subtract 1. Do not use `bookmark_page()`, which returns the *displayed* page number and diverges after a section restart.
- Other schemes: rejected by the sanitizer; drop with a warning. No `javascript:`, `data:`, or control characters reach the terminal/SVG/OSC 8 path.
- Do **not** emit link annotations for empty rects.

#### 7.7.8 Unsupported elements

For any of the following, emit a typed warning and, where the contract allows, a visible placeholder:

| Situation | Action |
|---|---|
| Unknown `PositionedElement` variant | `WarningEvent::FidelityDegraded { feature: "docx-element-unknown", class: FidelityClass::Degraded }` |
| Paint the core cannot express | Nearest approximation + warning |
| Effect the core cannot express | Drop effect + warning |
| Missing/undecodable image | Placeholder + `WarningEvent::MediaFailed` |
| Missing font | Engine fallback + `WarningEvent::FontSubstituted` |
| Page with zero drawable elements | Blank page (valid) + debug log |

Warnings are deduplicated per document/feature to avoid flooding the channel.

### 7.8 Fonts: exact-face bridging

This is the most important non-obvious design point. `rdocx` shapes with its own `FontManager` and emits **glyph IDs** plus `FontData { id, family, data, face_index, bold, italic, variations, synthetic_bold, synthetic_italic }`. Our rasterizer/SVG currently resolve a face by **family + bold/italic** through `fontdb`; if that differs from rdocx's face, glyph IDs render as garbage and positions drift. Family registration is insufficient because embedded fonts and substitutes can share a family, TTCs have multiple face indices, variable-font instances differ, and synthetic bold/italic is not represented in `PositionedGlyphRun`.

**Design: a document-face registry in core.** The registry is process-global/append-only for v1, which is fine for the single-document CLI; if the TUI gains document reload, add a session-scoped handle and drop it with the source.

```rust
// crates/office-terminal-core/src/text/document_faces.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentFaceId(u32);

pub struct DocumentFace {
    pub data: Arc<[u8]>,
    pub face_index: u32,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub variations: Vec<([u8; 4], f32)>,
}

pub fn register_document_faces(faces: Vec<DocumentFace>) -> Vec<DocumentFaceId>;
pub fn with_document_face<R>(id: DocumentFaceId, f: impl FnOnce(&DocumentFace) -> R) -> Option<R>;
```

- Registry is global (`OnceLock<RwLock<Vec<Arc<DocumentFace>>>>`), append-only, read-mostly; immutable after doc open, so workers need no setup and the thread-local cosmic `FontSystem` is untouched.
- `PositionedGlyphRun` gains `pub face: Option<DocumentFaceId>`; `DocxPageSource` registers `engine.fonts()` once and maps `FontId → DocumentFaceId`. The field addition is mechanical but touches every constructor and test; land it in one Phase 2 PR with `cargo check --all-targets`.
- Rasterizer: new `rasterizer/text/document.rs` paints glyph IDs from registered face bytes (swash/ttf-parser), bypassing `fontdb` and shaping; SVG `VectorGlyphs` uses the same outlines.
- SVG `SelectableText`/`TextTags` use `font_family` + `text` (logical text, not glyph IDs).
- `single_face` remains the PPTX fast path; `face: Some(..)` takes precedence. If `face` is `None`, fall back to family lookup with `single_face=false`.

**Why:** not DOCX-specific (the dormant PDF vector bridge and metafile fonts have the same "external engine supplied glyph IDs" problem); deterministic, independent of host fonts; avoids fragile family aliases; face bytes live for the source lifetime.

**Variations / synthetic styles.** v1 uses the default face instance and carries `variations` without applying them (warn on non-default axes); v2 extends the raster cache key with variation coordinates and applies them through `swash`. Synthetic bold/italic falls back to the family path with a warning in v1; v2 synthesizes stroke/shear.

**SVG text modes**

| Mode | DOCX behavior |
|---|---|
| `SelectableText` | Emit `<text>` with `font-family`, `font-size`, positions, and logical `text`. |
| `TextTags` | Same, with per-run tags. |
| `VectorGlyphs` | Emit glyph outlines from the exact `DocumentFace` bytes. |

### 7.9 Media and `DocxAssetStore`

```rust
pub struct DocxAssetStore {
    entries: HashMap<String, Arc<DocxMedia>>, // rel_id -> raw bytes + content type
    decoded: Mutex<LruMap<String, DecodedImage>>,
    stats: Mutex<AssetStoreStats>,
}

impl AssetStore for DocxAssetStore {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError>;
    fn get_raw_bytes(&self, rel_id: &str) -> Result<Vec<u8>, AssetError>;
    fn archive_bytes(&self) -> Option<&[u8]> { None }
    fn stats(&self) -> AssetStoreStats;
}
```

- Build `entries` from every `PositionedElement::Image` in the layout (walk once). Key by `MediaId`. Later duplicate references share the same bytes.
- `get` decodes with the `image` crate (PNG/JPEG/WebP/GIF/BMP/TIFF), honors `SizeHint::AtMost`, and caches decoded RGBA with a byte budget (e.g., 64 MiB, matching `DefaultAssetStore`).
- SVG: render through `resvg` at the hint size; cache the raster.
- EMF/WMF: do **not** decode here. The bridge should convert metafile pictures to vector commands or the standard placeholder before they become `DrawImage`.
- `get_raw_bytes` serves `dump`/debug and any raw consumers.
- `archive_bytes` is `None`: DOCX media is not a ZIP to serve as a whole archive.
- Stats feed `info`.

### 7.10 Search corpus

```rust
pub struct DocxSearchCorpus { pages: Vec<Vec<TextLine>> }

impl SearchCorpus for DocxSearchCorpus {
    fn page_count(&self) -> usize { self.pages.len() }
    fn page_text(&self, page: usize) -> &[TextLine] { /* ... */ }
}
```

Build strategy:

1. Walk each page's `Text` / `MultilingualText` elements (including groups).
2. Group runs into visual lines and order them (`search_order.rs`):
   - Primary: group by `GlyphRun.source.node` (source paragraph/cell) when available, one line per baseline band (a wrapped paragraph spans several), runs sorted by `source.char_start` so logical order survives RTL.
   - Runs **without** a source node (list labels such as `1.` / `•`, field results) attach to the nearest attributed line on the same baseline when within 2.5 font sizes of it, so `1. Headings` reads as one line; otherwise they form their own line sorted by `x`.
   - Page order: the engine's logical structure tree (`LayoutResult.structure`, carried on each `MarkedContent`) marks lines below a `Table`/`TableRow`/`TableCell` role; those stay row-major `(y, x)`. Body lines are cut into strips by wide lines (≥ 60 % of the page width) and table lines; inside a strip, text forming two or more horizontally disjoint bands (gap ≥ 24 pt — Word's default column gap is 36 pt, a cell margin gap ≈ 11 pt) reads band by band (column-major), else top-down left-to-right.
   - **Found with `sample3.docx`:** the first implementation ordered nodes by first-run `(y, x)` only, which read a two-column section row by row and trailed every list label at the end of the page.
3. Insert a space when the horizontal gap between adjacent runs exceeds a small fraction of the font size and neither side already has whitespace.
4. Compute the line rect as the union of run bounds (`origin`, advances, font size).
5. Skip hidden/generated-empty runs; keep field results (page numbers) because they are visible text.
6. Never reorder text inside a run; use `text`/`logical_text` as authored.
7. Cap lines per page (e.g., 10,000) and total text bytes with a resource warning.

The corpus is built lazily on the first search/`dump --markdown`, not on open. It is immutable and shared.

### 7.11 TOC, metadata, notes, comments

- **TOC:** map `LayoutResult.outlines` to `TocEntry`; page indices are already 0-based. Filter duplicate/empty headings.
- **Metadata:** proxy `title`, `author`, `subject`, `keywords`, `creator` into `info` and `dump --json` where useful. Do not make it part of `PageSource` for v1; the CLI can read it from the engine via a `DocxPageSource::metadata()` method.
- **Notes:** `PageSource::notes` returns `None`. DOCX has no speaker notes. Presenter view shows an empty notes pane.
- **Comments:** not rendered. `dump --comments` may be added later by reading `rdocx::Document::comments()` before dropping the document, or by a separate comments API. For v1, print “(No comments)” like the PDF path, or omit.
- **Bookmarks:** `bookmark_page` supports internal hyperlinks and `dump` navigation. Not exposed as TOC entries unless they coincide with headings.

### 7.12 CLI and TUI

#### CLI

- `view`: accept `.docx`; use reader mode by default; `--slide`/`-n` selects a 1-based page.
- `present`: accept `.docx`; default to reader mode (`DeckModeArg::Auto` → reader for DOCX). Presenter split view still works; notes are empty.
- `export`: page range semantics are pages; PNG/SVG sizes use per-page dimensions.
- `info`: print `Pages`, `Dimensions` (first page), `Aspect Ratio`, `Bookmarks/TOC`, `Media`, `Title/Author` when available.
- `dump`:
  - `--markdown`, `--json`: use `SearchCorpus` exactly like PDF.
  - `--notes`: print “(No speaker notes)”.
  - `--comments`: v1 prints “(No comments)” or is documented as unsupported.
  - `--ast`: reject with a clear message: “AST dump is PPTX-only; use --markdown or --json for DOCX.”
- Usage strings and `--help` should say `.pptx`, `.pdf`, or `.docx`, and “slide/page”.
- `--mode` help: document `auto|slides|reader` and that DOCX defaults to reader.
- `--slides` help: “page range (1-based)”.

CLI match arms (six sites):

```rust
match deck {
    DeckSource::Pptx(presentation, asset_store) => { /* unchanged */ }
    DeckSource::Pdf(source) | DeckSource::Docx(source) => { /* shared paged path */ }
}
```

For `dump`, split `dump_pdf` into a generic `dump_paged(Arc<dyn PageSource>, ...)`; reuse for DOCX.

#### TUI

The TUI is already page-source driven:

- `enable_reader_mode(total_pages, page_sizes)` works with mixed sizes.
- Reader layout, centering, scroll, thumbnails, find, and pan use `PageSource`/cache APIs.
- No DOCX type may enter the TUI. It sees only pages, rasters, text, and links.
- The presenter thumbnails for a DOCX are page thumbnails; that is acceptable.
- Decide whether the title bar says “Page x / y” for DOCX/PDF versus “Slide x / y” for PPTX. This can be a `DocumentKind` hint later, not part of this PR.

### 7.13 Export

- PNG: existing pipeline; per-page dimensions; `--width`/`--dpi` semantics unchanged.
- SVG:
  - `VectorGlyphs` (default): exact glyph outlines from `DocumentFace`; text becomes paths. This is the most faithful, but text is not selectable.
  - `SelectableText`/`TextTags`: emit logical text with positions; shape with the registered family where possible. Useful for accessibility/search.
- Page backgrounds, images, links, and clips must survive both backends.
- `export` should reject a page range outside `1..=len` with a clear error, not silently clamp.
- Export tests compare raster vs SVG bounding boxes and text content.

### 7.14 Errors and resource limits

```rust
#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("DOCX package error: {0}")]
    Package(String),
    #[error("DOCX layout error: {0}")]
    Layout(String),
    #[error("encrypted/password-protected DOCX is not supported")]
    Encrypted,
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(Box<ResourceViolation>),
    /// Caught third-party panic (adopted below; see "Defensive handling").
    #[error("DOCX layout engine panicked: {0}")]
    LayoutEnginePanic(String),
}
```

As shipped. The first draft also listed `Unsupported(&'static str)`; it was
dropped because every unsupported construct degrades to a warning or
placeholder per §7.19, so no code path ever produced it.

Mapping:

| Source | Destination |
|---|---|
| ZIP/OPC/package parse | `ParseError::ArchiveError` |
| `PackageReadLimits` / layout budget | `ParseError::ResourceLimit` (construct `ResourceViolation { stage: "docx-package", .. }`) |
| encrypted OOXML | `ParseError::PasswordProtected` (detected before the engine) |
| layout failure | `RenderError::CompilationFailed` |
| missing page when compiling | `RenderError::CompilationFailed` |
| image decode | `WarningEvent::MediaFailed` + placeholder |
| font substitution | `WarningEvent::FontSubstituted` |
| unsupported paint/effect/element | `WarningEvent::FidelityDegraded` |

Proposed limits (configurable via `DocxLimits`):

| Limit | Default |
|---|---|
| ZIP entries | 10,000 |
| Uncompressed bytes per part | 64 MiB |
| Total uncompressed bytes | 512 MiB |
| Pages | 10,000 |
| Text bytes per page (search) | 8 MiB |
| Decoded image bytes (cache) | 64 MiB |
| IR cache pages | 16 |
| IR cache bytes | 32 MiB |
| Layout deadline | 30 s (soft; log/warn, do not kill mid-layout) |

`rdocx::Document::from_bytes_with_limits(bytes, PackageReadLimits { .. })` accepts the package limits. Map its error text/type to ours; do not re-label a limit crossing as “corrupt archive”.

Defensive handling: consider `std::panic::catch_unwind` around the third-party parse/layout boundary and convert a panic to `DocxError::Layout`. This is a policy decision; if adopted, document it and never catch panics in first-party code.

### 7.15 Security

- **No macro execution.** `.docm` is parsed as a document; VBA parts are ignored.
- **No network access.** External images/relationships are not fetched; emit a warning/placeholder.
- **No subprocess.** Everything is in-process.
- **Hyperlink gate.** Every URL goes through `sanitize_hyperlink_url` before `SlideLink`, OSC 8, SVG, or an OS opener. Reject control characters, over-long URLs, and non-http(s)/mailto schemes. Internal `#bookmark` links resolve to page indices, never to arbitrary URLs.
- **ZIP bombs.** Package limits above; no extraction to disk.
- **Path traversal.** Media is served from in-memory keys (`docx-media-<hash>`), never from archive paths; no filesystem writes.
- **Untrusted fonts.** Embedded fonts are parsed by `ttf-parser`/`swash`; add the DOCX corpus to fuzzing and keep `resvg`/`image` decode limits.
- **Panic safety.** A malformed document must produce an error, not abort the TUI. Catch at the boundary if rdocx is not fully hardened.
- **No `unsafe`.** `#![forbid(unsafe_code)]` in the new crate. Third-party `rdocx` is reviewed as a dependency; no C/C++.

### 7.16 Performance and caching

- Layout is computed **once** at open (`layout_deterministic`), not per render. `WordLayoutResult` holds all pages; the cost is O(document), and rdocx reports very high pagination throughput.
- `compile_page` maps one page to `VectorSlide`; it is CPU-light (geometry conversion) compared with rasterization.
- Add a bounded IR cache (page index → `Arc<VectorSlide>`) because the pipeline may compile the same page for multiple target sizes. `VectorSlide` is resolution-independent.
- The pipeline already prefetches pages on worker threads; `DocxPageSource` must be `Send + Sync`.
- Font registration is O(used faces) once; no per-page font loading.
- Search corpus is lazy and immutable.
- Media decoding is lazy and LRU-cached.
- Avoid cloning `Arc<[u8]>` per image; `DocxAssetStore` owns one copy per `MediaId`.
- Memory target: a 1,000-page text document should stay within the existing pipeline budget; measure and add a regression test. If layout memory is too high, consider an LRU over `PageFrame`s later (rdocx computes all pages, so this is a post-MVP optimization).

### 7.17 Feature gating and dependencies

Add a `docx` feature to the CLI and make `office-terminal-docx` optional:

```toml
# crates/office-terminal-cli/Cargo.toml
[features]
default = []          # development; flip to ["docx"] for the release that documents it
docx = ["dep:office-terminal-docx"]

[dependencies]
office-terminal-docx = { workspace = true, optional = true }
```

Gate the registration call: `#[cfg(feature = "docx")] office_terminal_docx::register();`. During development keep `docx` off by default to avoid a 147-crate build for PPTX-only changes; enable it in CI and release builds. If the team prefers a single always-on binary, drop the feature and accept the build cost.

Dependency impact (measured on this machine):

- `rdocx` 0.17.0 normal tree: 147 unique crate names.
- New duplicates likely: `fontdb 0.23` and `ttf-parser 0.25`, plus `harfrust`, `icu_segmenter`, `hypher`, `pdf-writer`, `html5ever`/`scraper`, `pulldown-cmark`. `zip 9.0.3` unifies with our `=9.0.4`, so ZIP is not duplicated.
- No C/C++ observed: `zip` uses the pure-Rust `zlib-rs` backend, and `fontconfig-parser` is pure Rust.
- `cargo build -p rdocx-cli --release`: ~3 minutes; full CLI binary ~39 MB.
- Pin `rdocx = "=0.17.0"` and `oxml-layout = "=0.15.0"`. Re-evaluate on every upgrade; rdocx is pre-1.0.

Licensing: `rdocx` is MIT (`LICENSE`; Cargo metadata `MIT OR Apache-2.0`), compatible with this MIT project. Record it in `crates/VENDORED.md` only if vendored; as a crates.io dependency, add a note to the README/third-party notices if the project keeps one.

### 7.18 Known fidelity gaps (observed or expected)

| Gap | Severity | Mitigation |
|---|---|---|
| `w:docGrid` + `w:lineRule="auto"` line spacing | High for CJK/gridded docs | Patch `rdocx` upstream, or post-process? Layout positions are already computed, so a patch is required. Add a regression fixture. |
| Anchored frames/floats spacing on the magazine sample | Medium | Build a focused fixture; compare with upstream reference; patch/upstream. |
| Group opacity / linear opacity masks | Low–medium | Approximate with warning; future IR commands. |
| Variable font axes / synthetic bold+italic | Medium | Exact-face default instance + warning; extend `DocumentFace`/raster cache. |
| EMF/WMF pictures | Medium | Convert through the existing metafile engine or placeholder. |
| Internal bookmark hyperlinks | Low–medium | **Confirmed during implementation**: `rdocx` 0.17 projects external relationship hyperlinks as `LinkAnnotation` elements but not `w:anchor` internal links, so the bridge's `bookmark_page` resolution currently has no annotation to consume (bridge code and tests are in place for the day the engine projects them). External links resolve and sanitize correctly. |
| Multi-column sections (`w:cols`) | **Medium** | **Confirmed with `sample3.docx`.** Column tracks themselves work (text flows down column 1, then column 2, then the next page) and the corpus reads column by column. But `rdocx-layout` `paginator.rs` groups a *continuous* section onto the current page only when `continuous_geometry_matches` — which requires identical column tracks — so a continuous section that changes the column count **starts on a new page** (Word: same page, under the preceding content). With short column text that means a stacked single column with the heading stranded on the previous page. There is also no column balancing (Word only balances at a *following* continuous break, so that part is defensible). Upstream fix required; the bridge is faithful (the engine's own PDF shows the same layout). Tracked by the `#[ignore]`d `continuous_column_change_starts_on_the_current_page` test. |
| Comments/margin annotations | Low | Out of scope for rendering; `dump` only. |
| Tracked changes | Low | Engine supports accepted/tracked; CLI option optional. |
| Vertical text / East Asian specifics | Unknown | rdocx has coverage; test with fixtures. Do not assume parity. |
| Charts/math/VML | Unknown | rdocx renders to paths; test with fixtures. |
| Page background gradients/radial | Low | Approximate where the core model is linear-only. |
| Font fallback differences | Medium | Exact-face bridge; rdocx diagnostics → warnings. |

These gaps must be visible in the test matrix, not hidden. A DOCX page that is “mostly right” is acceptable for the MVP; a silently dropped image or an unreadable page is not.

### 7.19 Degradation policy

Follow PRD 02d:

- **Full**: mapped exactly (text, paths, images, solid/gradient paints, links).
- **Degraded**: rendered approximately (group opacity, radial backgrounds, shadows on groups). Emit `WarningEvent::FidelityDegraded { class: FidelityClass::Degraded }`.
- **Placeholder**: unsupported visual element (metafile playback failure, missing image). Emit `MediaFailed`/`FidelityDegraded` and draw a visible placeholder.
- **Dropped**: never silently. Every dropped element emits a warning with the page index and element kind.

Warnings are surfaced once per document in the CLI/TUI diagnostics log, not per raster.

---

## 8. Testing

**Fixtures** (`crates/office-terminal-docx/tests/fixtures/`, generated with Word/LibreOffice, not rdocx): minimal paragraph; headings + TOC; table with merges/borders/shading; inline + anchored images; headers/footers + page fields; footnotes/endnotes; multilevel numbering; `w:docGrid` + `lineRule="auto"` regression; floats/wrap; external + internal hyperlinks; vertical text; encrypted; corrupt ZIP/XML. `sample-1.docx` + reference PNGs only if redistribution is licensed; otherwise keep cache-gated.

**Unit / golden:** format detection; pt→EMU rounding; text advances/ligatures/multilingual offsets; path/paint/stroke mapping; group transform/clip/opacity; effect warnings; link sanitization; search ordering (paragraphs, table cells, RTL, gaps); warning dedup; error mapping. Snapshot a stable JSON projection of `VectorSlide` per fixture (rounded floats, command counts, key positions), regenerate deliberately per engine version.

**Visual / integration:** render fixtures at 96 DPI and compare against upstream reference PNGs with SSIM/perceptual thresholds; rdocx's own PNG output is a bridge cross-check, not ground truth. Known-fail `sample-1` until the docGrid gap is fixed; never silently pass degraded output. SVG: assert text content, page size, images/links, and rasterize with `resvg` for comparison. CLI smoke tests: `view`/`present`, PNG/SVG export, `info`, `dump --markdown`/`--json`, `--ast` error, stdin, and extension mismatch.

**Fuzzing:** `cargo fuzz` target for `open` + `compile_page`; assert no panics/OOM, typed errors. Fuzz the bridge with synthetic `PageFrame`s (NaN/infinite coordinates, empty paths, huge gradients). CI: `cargo check/clippy/test --workspace --features docx` plus a no-`docx` build.

---

## 9. Implementation plan

| Phase | Work | Est. |
|---|---|---|
| 0 — Spike | Temp `rdocx` → PDF → `PdfPageSource`; verify view/reader/search/export/info; record gaps; delete spike | 1–3 d |
| 1 — Skeleton | New crate; `DocxEngine`/`RdocxEngine`/`DocxPageSource`; `SourceFormat::Docx`, `DeckSource::Docx`, loader hook; OPC detection; typed errors; CLI arms behind `docx` | 2–4 d |
| 2 — Bridge | Page/background; monolingual + multilingual text; paths/paints/strokes; groups/clips/opacity/effects; `DocumentFace` registry + raster/SVG face paths; golden IR tests | 5–8 d |
| 3 — Media/links | `DocxAssetStore`; image/metafile/SVG routing; bookmark links; search corpus; TOC; `dump --markdown/--json`; `info` | 3–5 d |
| 4 — UX | Reader default; page ranges/help; PNG/SVG export; presenter smoke; `--ast` error; README | 3–4 d |
| 5 — Hardening | Fixtures + visual tests; fix docGrid and anchored-frame gaps; fuzz/limits/panic boundary; 1,000-page perf; zero-warning CI | 5–10 d |
| 6 — Parity (separate issue) | Vendor upstream model/parser; port layout in slices; `NativeDocxEngine`; differential tests | multi-month |

Do not start Phase 6 until Phase 5 is stable and parity is a concrete requirement.

---

## 10. Acceptance criteria

- [ ] `view` opens DOCX in reader mode, renders every page, exits cleanly; `present` navigates without panic (notes empty).
- [ ] `export --format png` writes correctly sized pages; `--format svg` writes valid pages with text (VectorGlyphs / SelectableText).
- [ ] `info` reports pages, dimensions, TOC, media; `dump --markdown`/`--json` returns reading-order text; `--ast` errors clearly.
- [ ] `.docx`/`.pptx`/`.xlsx`/`.pdf`/`.doc`/random inputs produce the documented typed errors; extension is ignored.
- [ ] Images, backgrounds, paths, gradients, links, and headers/footers survive raster + SVG.
- [ ] Exact faces used by layout render correctly; no garbled glyph IDs on embedded/substituted fonts.
- [ ] Every unsupported construct warns or draws a placeholder; no silent drops; limits produce typed errors, not OOM/panics.
- [ ] `cargo check`, `clippy -D warnings`, `test` pass with and without `--features docx`; no PPTX/PDF regressions.
- [ ] New files <600 lines; new crate `#![forbid(unsafe_code)]`; docs state DOCX support, limitations, fidelity class.

Stretch: fix docGrid so `sample-1` matches the reference page count; `--revision-view accepted|tracked`; comments in `dump`.

---

## 11. Risks and mitigations

| Risk | Mitigation |
|---|---|
| `rdocx` API churn | Pin exact versions; isolate behind `DocxEngine`; upgrade with golden tests. |
| Fidelity gaps unacceptable | Document + fixture them; patch upstream; keep native-port path open. |
| Build/binary size | Feature-gate `docx`; measure; consider `rdocx-layout` + `rdocx-oxml` only later. |
| Wrong glyphs from font bridging | Exact `DocumentFaceId` path; visual tests on embedded/substituted fonts; fallback to re-shaping. |
| Layout memory on huge docs | Page count/size limits; measure; later LRU/page streaming. |
| Third-party panic on malformed input | `catch_unwind` at the boundary; fuzzing; typed errors; never crash the TUI. |
| Parity target ambiguity (Word vs reference) | Decide explicitly; test against both where possible. |
| Scope creep (editing/comments/tracked) | Keep non-goals explicit; separate issues. |

---

## 12. Future work

1. Native layout engine: vendor upstream `docx-model`/`docx-parser`, port the TS paginator in slices, implement `NativeDocxEngine`.
2. Reuse `DocumentFaceId` for the dormant PDF vector bridge and metafile-embedded fonts.
3. Add `PushOpacity`/mask commands to `VectorCommand` if DOCX effects justify them.
4. Tracked-changes UI, comments pane, word-level search highlighting.
5. Other WordprocessingML inputs (`.dotx`, view-only `.docm`); spreadsheet input stays out of scope.
6. Remove the Phase 0 PDF spike once the direct bridge lands (or keep it as a test oracle).

---

## 13. Open questions

1. **Feature gate:** off during development, on for the first documented DOCX release? (Recommended.)
2. **Fidelity target:** upstream reference renderer or Microsoft Word? The docGrid case shows they differ.
3. **Dependency size:** accept ~39 MB-class binary / 147-crate tree, or rebuild the package loader on `rdocx-layout` + `rdocx-oxml`?
4. **Exact-face contract:** approve `DocumentFaceId` on `PositionedGlyphRun` + new raster/SVG branches (robust), or start with fragile family-name registration?
5. **Reader default:** every DOCX opens in reader mode, even one-page landscape? (Recommended yes, with `--mode slides`.)
6. **Corpus licensing:** can upstream `sample-1.docx` + reference PNGs be redistributed, or must they stay cache-gated?

---

## 14. References

### Code and issues

- `crates/office-terminal-core/src/loader/presentation_loader.rs` — format sniffing, PDF hook.
- `crates/office-terminal-core/src/contracts/deck.rs` — `PageSource`, `DeckSource`, `SearchCorpus`, `TocEntry`.
- `crates/office-terminal-core/src/contracts/vector_slide.rs` — `VectorSlide`, `VectorCommand`.
- `crates/office-terminal-core/src/contracts/text.rs` — `PositionedGlyphRun`.
- `crates/office-terminal-core/src/contracts/paint.rs` — `VectorPaint`, `VectorGradient`, `VectorStroke`.
- `crates/office-terminal-core/src/contracts/hyperlink.rs` — URL sanitizer, `SlideLink`.
- `crates/office-terminal-core/src/loader/asset_store.rs` — `AssetStore`.
- `crates/office-terminal-core/src/text/fonts.rs` — existing font runtime (thread-local).
- `crates/office-terminal-core/src/rasterizer/text/` — straight/buffer/affine text paths.
- `crates/office-terminal-core/src/svg/text.rs` — SVG text modes.
- `crates/office-terminal-pdf/src/source.rs` — `PageSource` adapter precedent.
- `crates/office-terminal-cli/src/commands/{view,present,export,info,dump}.rs` — match sites.
- `docs/14-pdf-input-support.md`, `docs/14a-pdf-reader-ui-designs.md` — reader architecture.
- `docs/02d-element-renderers-and-fidelity-policy.md` — degradation policy.
- `docs/06-component-interface-contracts.md` — contract conventions.
- `docs/07-quality-and-testing.md` — test levels.

### External

- Upstream office-open-xml-viewer: <https://github.com/yukiyokotani/office-open-xml-viewer>
  - `packages/docx/model`, `packages/docx/parser` — Rust parser/model.
  - `packages/docx/src/layout`, `packages/docx/src/line-breaker` — TypeScript layout.
  - `packages/docx/public/demo/sample-1.docx` — reference fixture.
  - Commit evaluated: `59b28ea9eb0dd9e8770e5bdcb0437592b8098cd3`.
- `rdocx` 0.17.0: <https://github.com/tensorbee/rdocx>
  - Crates: `rdocx`, `rdocx-layout`, `rdocx-oxml`, `oxml-layout`.
  - Commit evaluated: `eb980e94c8ae1f8b42612f918482bc627e0141a0`.
  - License: MIT.
- `zavora-docx` 0.1.4: <https://github.com/zavora-ai/zavora-docx>
  - Commit evaluated: `2be0ad47f5454c16936664c60d625fffe2bb2d3f`.
  - License: MIT OR Apache-2.0.
- ECMA-376 Part 1:
  - §17.3.1.33 `w:spacing` / `w:lineRule`.
  - §17.6.5 `w:docGrid`.
  - §17.3.1.20 `w:lastRenderedPageBreak`.
  - §20.4.3.x anchor positioning.
- ISO/IEC 29500 (same content).

---

## Labels / metadata

**Labels:** `enhancement`, `docx`, `parser`, `layout`, `needs-design`
**Milestone:** DOCX input MVP
**Estimate:** 1–3 day spike + 4–6 week vector MVP (1 engineer)
**Dependencies:** `rdocx = "=0.17.0"`, `oxml-layout = "=0.15.0"`; upstream vendoring only for the parity track.

<details>
<summary>Appendix — Measured evaluation summary</summary>

| Measurement | Result |
|---|---|
| Upstream DOCX Rust parser/model | 61,963 non-test lines |
| Upstream DOCX TypeScript layout + line breaker | 66,587 + 15,306 non-test lines |
| Upstream DOCX TypeScript total | 114,120 non-test lines |
| `rdocx-layout` / `oxml-layout` | 63,218 / 14,243 lines |
| `rdocx-oxml` / high-level `rdocx` | 150,354 / 248,313 lines |
| `zavora-docx-layout` / `-oxml` | 5,919 / 12,679 lines |
| `rdocx` normal dependency names | 147 |
| `rdocx-cli` release binary / build | ~39 MB / ~3 min |
| `sample-1.docx` pages: reference / rdocx / zavora | 6 / 9 / 6 (broken flow) |
| Upstream parser compile probe | Pass after `ImagePartTypes` + `schema` feature |

</details>
