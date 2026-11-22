# PRD 16: XLSX Input Support — SpreadsheetML Workbooks, Sheet Grid & Print Pagination

> **Status:** Normative Design Specification (complete implementation; no phased rollout).
> **Feature Track:** Read-only SpreadsheetML (`.xlsx`, `.xlsm`, `.xltx`, `.xltm`) input for `view`, `present`, `export`, `info`, and `dump`.
> **Depends on:**
> - [PRD 00: System Overview](./00-system-overview.md) (architecture, CLI binaries, and invariants)
> - [PRD 01: Parser & Data Model](./01-parser-and-data-model.md) (loader, container sniffing, asset store)
> - [PRD 02a: Composition Core & Pipeline](./02a-composition-core-and-render-pipeline.md) (two-tier caching, prefetch queue)
> - [PRD 02b: Vector Style & Geometry Engine](./02b-vector-style-and-geometry-engine.md) (colors, theme resolution, strokes)
> - [PRD 02c: Typography & Text Layout](./02c-typography-and-text-layout.md) (font matching, glyph shaping, measurement)
> - [PRD 02d: Element Compilers & Fidelity Policy](./02d-element-renderers-and-fidelity-policy.md) (FULL/DEGRADED/PLACEHOLDER/OMITTED matrix)
> - [PRD 06: Component Interface Contracts](./06-component-interface-contracts.md) (`VectorSlide`, `PageSource`, `DeckSource`, `SearchCorpus`)
> - [PRD 07: Quality & Testing](./07-quality-and-testing.md) (test levels, DSSIM scoring, golden scenes)
> - [PRD 08: Shape Effects & 3D Engine](./08-shape-effects-and-3d-engine.md) (chart/shape effects)
> - [PRD 13: Metafile Pictures & Vector Playback](./13-metafile-pictures-and-vector-playback.md) (EMF/WMF in worksheets)
> - [PRD 14: PDF Input Support](./14-pdf-input-support.md) (`PageSource` abstraction)
> - [PRD 14a: PDF & Document Reader UI Designs](./14a-pdf-reader-ui-designs.md) (source-rect placements, pane residency, keymap conventions)
> - [PRD 15: DOCX Input Support](./15-docx-input-support.md) (flow-document precedent, degradation taxonomy)

---

## 1. Executive Summary & Problem Statement

### 1.1 Context
Office Terminal (`ot`) renders PowerPoint (`.pptx`), PDF (`.pdf`) and — per [PRD 15](./15-docx-input-support.md) — Word (`.docx`) documents to the terminal through the Kitty Graphics Protocol, and exports vector SVG / raster PNG headlessly.

Every OOXML ZIP package is currently presumed to be a PresentationML deck. `sniff_container_format` (`crates/office-terminal-core/src/loader/presentation_loader.rs:170`) returns `SourceFormat::Pptx` for any `PK\x03\x04` header, so a workbook fails deep inside the presentation parser:

```
$ ot view budget.xlsx
Error: Failed to load presentation: Missing required OOXML part: ppt/presentation.xml
```

### 1.2 Why a spreadsheet is neither a slide nor a page
PPTX stores explicit coordinates; PDF stores pre-computed glyph positions; DOCX is a flow document with a pagination algorithm. SpreadsheetML is a **fourth** category:

- There is **no page** in the file. Pages are a **print-time computation** defined by `printArea`, `printTitles`, `pageSetup` (scale / fit-to / paper size / orientation / order), `rowBreaks`, `colBreaks`, `pageMargins`, `printOptions` and `headerFooter` field codes.
- There is **no object list for layout**. A sheet is a sparse grid of cells addressed `A1`; geometry is implied by column widths (`cols/col@width`, expressed in *max-digit-width* units), row heights (`row@ht`, in points) and defaults.
- **Cell geometry depends on font metrics**, not on stored units. Excel's column pixel width derives from the workbook default font's maximum digit advance at 96 DPI, preferring the font's embedded bitmap metrics. The ECMA-376 §18.3.1.13 conversion is documented as incorrect even by implementers ([ClosedXML Cell Dimensions](https://github.com/ClosedXML/ClosedXML/wiki/Cell-Dimensions)).
- Objects (charts, pictures, shapes) are **anchored to cells** (`from`/`to` with per-cell offsets in EMU), so object placement depends on the same grid solver as text.
- **Row/column counts are unbounded in practice** (1,048,576 × 16,384): a "whole sheet" render is not a page, it is a mosaic that must be windowed and tiled.

### 1.3 Strategic solution
XLSX becomes a first-class input with **one engine and two surfaces**:

1. **Sheet Grid surface (primary).** A 2D viewport over a worksheet: cell cursor, sheet tabs, frozen panes, statistics, find/go-to. Rendered as `VectorSlide` **tiles** through the unchanged two-stage pipeline, presented via Kitty source-rect placements.
2. **Print-Page surface (derived).** A full pagination engine that reproduces Excel's printed output (print area, fit-to scaling, repeated titles, breaks, headers/footers, centering, clipping) and projects each printed page into the very same `VectorSlide` compiler. This surface is the one used by `present`, by `--mode print|slides|reader`, and by the default `export`.

Both surfaces call a **single canvas compiler**. There is no second renderer and no raster shortcut: the vector-first invariant of [PRD 02a](./02a-composition-core-and-render-pipeline.md) holds for spreadsheets exactly as it does for slides.

### 1.4 Resolved design decisions

| # | Decision | Rationale |
| :--- | :--- | :--- |
| **D1** | `view` opens **Sheet Grid** by default; `--mode print` selects the paginated surface | A spreadsheet user expects a grid; printing is a derived projection, not the primary identity of the file |
| **D2** | `present` and `export` operate on **Print-Page** output | The presenter console is page-indexed, and printed pages are the only defensible decomposition of a grid into discrete units. `--sheet` overrides for whole-sheet export |
| **D3** | A print page is **a viewport + page chrome**, not a separate renderer | Guarantees byte-identical cell rendering between surfaces; one compiler, one set of fidelity rules |
| **D4** | Parsing is **in-house SpreadsheetML on `ooxml-common`** | No candidate crate exposes what is required (see §3) |
| **D5** | **No formula evaluation engine**; cached results are displayed | ECMA-376 persists computed values alongside formulas; a calc engine is a separate product |
| **D6** | Charts are rendered by the **existing `compiler/charts/` engine** | Spreadsheet chart parts are ordinary DrawingML charts; the core already consumes `series[].values` |
| **D7** | Grid geometry is resolved from a **`MetricsProfile`**, overridable through config | Excel's own numbers depend on bitmap metrics we cannot read; make the assumption explicit and tunable |
| **D8** | Grid tiling and panning use **Kitty source-rect placements** | Proven by [PRD 14a](./14a-pdf-reader-ui-designs.md) §3.1: pan re-places, never re-encodes |

---

## 2. Goals & Non-Goals

### 2.1 Goals
1. **Both surfaces, complete.** Sheet Grid with cell cursor, frozen panes, sheet tabs, find, go-to-cell and live selection statistics; Print-Page surface with a faithful pagination engine.
2. **True vector parity.** Every surface compiles to `VectorSlide`; SVG export carries selectable text; PNG export works at arbitrary DPI/width.
3. **Fidelity where it matters.** Number formats, styles, merged cells, text alignment/wrap/shrink/rotation, conditional formatting, charts, pictures and metafiles.
4. **Honest degradation.** Every unsupported construct becomes a typed `WarningEvent` plus a visible placeholder — never a silent hole, never a panic ([PRD 02d](./02d-element-renderers-and-fidelity-policy.md)).
5. **Bounded work.** Windowed tile compilation, LRU tile cache, decoded-image budgets, hard sheet/cell/sheet-count limits, `catch_unwind` around third-party-free but hostile input.
6. **Zero new third-party dependencies.** Reuse `ooxml-common`, `quick-xml`, `roxmltree`, `image`, `resvg`, `rustybuzz`, `fontdb`, `tiny-skia`.
7. **Strict engineering standards.** `#![forbid(unsafe_code)]`, hard `< 600` lines per source file ([AGENTS.md](../AGENTS.md) §A), zero compiler and Clippy warnings.

### 2.2 Non-Goals
- **Editing, saving, round-tripping** of any kind (formulas, styles, sheets).
- **Formula recalculation.** Cached values only (§8.6); uncalculated cells degrade visibly.
- **Pivot table and pivot chart recomputation.** Materialised cells render; the pivot model does not.
- **Macro execution.** `.xlsm`/`.xltm` are opened read-only with `vbaProject.bin` ignored.
- **Legacy and adjacent formats.** `.xls` (CFB), `.xlsb`, `.ods`, `.csv`/`.tsv`, `.wk*` are rejected with typed, actionable errors.
- **External subprocesses.** No LibreOffice, Excel, Python or Node at runtime.

---

## 3. Normative Dependencies & Parser Evaluation

| Candidate | Values / types | Styles | Merges | Drawings & charts | Print settings | Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`calamine`** | ✅ thorough, lazy per-sheet | ⚠️ partial (font/alignment basics), no theme resolution | ❌ not exposed | ❌ none | ❌ none | **Rejected** — supplies ~20 % of what the feature needs, and would still force a parallel style/drawing parser |
| **`umya-spreadsheet`** | ✅ | ✅ | ✅ | ⚠️ model-level only, no rendering semantics | ⚠️ model-level | **Rejected** — whole-workbook eager model, write-oriented, large trait surface, no line-budget separation from our fidelity policy |
| **Upstream SpreadsheetML (`packages/xlsx/parser` from `office-open-xml-viewer`) on `ooxml-common`** | ✅ | ✅ | ✅ | ✅ (charts mapped onto core `Chart`) | ✅ | **Selected** — reuses proven pure-Rust SpreadsheetML parser and AST from the upstream monorepo (`yukiyokotani/office-open-xml-viewer`), sharing `ooxml-common`, partitioned into `< 600`-line submodules under `crates/office-terminal-xlsx/src/package/` |

**Dependency policy.** No additions to `[workspace.dependencies]`. The crate reuses `ooxml-common` (vendored from upstream), `quick-xml`/`roxmltree`, `image`, `resvg`, `office-terminal-core`. `zip = "=9.0.4"` remains the single archive backend ([AGENTS.md](../AGENTS.md) §E).

---

## 4. Architecture & Two-Stage Pipeline

```
Source (.xlsx file or stdin)
         │
         ▼
presentation_loader::sniff_container_format(&bytes)
         │
         ├── %PDF- magic                                ──► SourceFormat::Pdf
         ├── ZIP + ppt/presentation.xml                 ──► SourceFormat::Pptx
         ├── ZIP + word/document.xml                    ──► SourceFormat::Docx
         ├── ZIP + xl/workbook.xml                      ──► SourceFormat::Xlsx
         └── anything else                              ──► typed ParseError
         │
         ▼
PresentationLoader::load(&bytes) ──► DeckSource::Xlsx(Arc<XlsxSource>)
         │
         ├────────────────────────┬──────────────────────────┐
         ▼                        ▼                          ▼
  Arc<dyn SheetSource>     Arc<dyn PageSource>        SearchCorpus / TocEntry
  (grid surface)           (print surface)
         │                        │
         ▼                        ▼
  compile_viewport()        compile_print_page()
         └───────────┬────────────┘
                     ▼
        compile_canvas(CanvasSpec) ──► VectorSlide      (single compiler)
                     │
     ┌───────────────┼────────────────────────┐
     ▼               ▼                        ▼
TinySkia        Vector SVG              TileViewport
RgbaSlide       standalone .svg         (Kitty source-rect placements)
```

**Invariant C1.** `compile_print_page(page)` is exactly `compile_canvas(CanvasSpec { window: page_window(page), origin: page_origin(page), clip: page_rect, chrome: page_chrome(page) })`. Any cell, style, chart or picture compiles identically on both surfaces; the only difference is the window and the chrome.

**Invariant C2.** No surface may read `xl/` XML during rendering. Parsing produces an immutable `Workbook`; compilation is pure and deterministic on it.

### 4.1 Contracts (`office-terminal-core`)

```rust
/// A cell window of a worksheet, in absolute sheet EMU.
pub struct CanvasSpec {
    pub sheet: usize,
    pub window: EmuRect,        // scroll origin + size, in sheet EMU
    pub viewport: EmuRect,      // clip rect in canvas EMU (= window for grid)
    pub scale: f32,             // 1.0 == 100 % (96 DPI reference)
    pub chrome: CanvasChrome,   // None | GridChrome | PrintChrome
}

/// Sheet-level source consumed by the grid surface.
pub trait SheetSource: Send + Sync {
    fn sheet_count(&self) -> usize;
    fn sheet(&self, index: usize) -> Option<SheetInfo>;   // name, kind, used range, frozen panes
    fn compile_canvas(&self, spec: &CanvasSpec, warnings: &Sender<WarningEvent>)
        -> Result<VectorSlide, RenderError>;
    fn cell_display(&self, sheet: usize, cell: CellAddr) -> Option<String>;
    fn cell_formula(&self, sheet: usize, cell: CellAddr) -> Option<String>;
    fn selection_stats(&self, sheet: usize, range: CellRange) -> SelectionStats;
    fn search_corpus(&self) -> Arc<dyn SearchCorpus>;
}
```

`XlsxSource` implements **both** `SheetSource` and `PageSource`. `PageSource` remains untouched, so the pipeline, presenter, exporters and reader keep compiling unchanged ([PRD 06](./06-component-interface-contracts.md) §2 delta).

`DeckSource` gains one variant (a `Docx` variant joins it if and when [PRD 15](./15-docx-input-support.md) ships):

```rust
pub enum DeckSource {
    Pptx(pptx_model::Presentation, Arc<dyn AssetStore>),
    Pdf(Arc<dyn PageSource>),
    Xlsx(Arc<XlsxSource>),        // implements PageSource + SheetSource
}
```

### 4.2 Module layout (`crates/office-terminal-xlsx`)

```
crates/office-terminal-xlsx/
├── Cargo.toml
├── src/
│   ├── lib.rs                # entry points, feature registration          < 120
│   ├── detect.rs             # OPC content-type + part sniffing            < 160
│   ├── error.rs              # XlsxError, degradation taxonomy             < 180
│   ├── package/
│   │   ├── mod.rs            # part graph, relationship resolution         < 200
│   │   ├── workbook.rs       # sheets, definedNames, calcPr, date1904      < 400
│   │   ├── shared_strings.rs # SST, inline strings, rich runs, rPh         < 320
│   │   ├── styles.rs         # cellXfs, fonts, fills, borders, dxfs        < 520
│   │   ├── theme.rs          # clrScheme -> core theme tokens              < 200
│   │   └── sheet_xml.rs      # sheetData, cols, rows, merges, views        < 560
│   ├── grid/
│   │   ├── mod.rs            # GridModel: prefix sums, lookups             < 340
│   │   ├── metrics.rs        # MDW resolution + MetricsProfile             < 300
│   │   ├── columns.rs        # width -> px, hidden, outline groups         < 260
│   │   └── rows.rs           # height -> px, auto-height, hidden           < 300
│   ├── value/
│   │   ├── mod.rs            # cell value + type coercion                  < 260
│   │   ├── numfmt_parse.rs   # 4-section grammar, conditions, colors       < 460
│   │   ├── numfmt_format.rs  # General, numeric, currency, percent         < 520
│   │   ├── numfmt_date.rs    # date/time epochs, [h]ss, elapsed            < 420
│   │   └── numfmt_frac.rs    # fractions, scientific, accounting           < 320
│   ├── paint/
│   │   ├── mod.rs            # resolve CellStyle -> resolved paint         < 340
│   │   ├── fills.rs          # solid/pattern/gradient degradation          < 300
│   │   ├── borders.rs        # 14 line styles, diagonals, collapsed share  < 400
│   │   └── conditional.rs    # dxf, colorScale, dataBar, iconSet, rule     < 520
│   ├── text/
│   │   ├── mod.rs            # cell text layout orchestration              < 360
│   │   ├── wrap.rs           # wrap/shrink/clip/overflow-spill             < 440
│   │   ├── align.rs          # 9 alignments, indent, rotation, vertical    < 340
│   │   └── runs.rs           # rich string runs, special fonts             < 320
│   ├── draw/
│   │   ├── mod.rs            # drawing parts, z-order, groups              < 320
│   │   ├── anchor.rs         # twoCell/oneCell/absolute -> EMU             < 300
│   │   ├── pictures.rs       # blips, crops, transparency, svgBlip         < 420
│   │   ├── shapes.rs         # worksheet shapes/text boxes -> DrawPath     < 380
│   │   ├── charts.rs         # chartN.xml -> core Chart                    < 460
│   │   ├── chart_data.rs     # numCache/strCache, c:f range fallback       < 320
│   │   └── sparklines.rs     # line/column/winLoss inline marks            < 300
│   ├── print/
│   │   ├── mod.rs            # page list construction                      < 340
│   │   ├── area.rs           # printArea/printTitles defined names         < 260
│   │   ├── setup.rs          # pageSetup, margins, paper, order            < 300
│   │   ├── fit.rs            # fit-to-width/height page-count solver       < 300
│   │   ├── breaks.rs         # manual/automatic row & column breaks        < 280
│   │   ├── chrome.rs         # printOptions, gridlines, headings, center   < 340
│   │   └── header_footer.rs  # field codes (&L/&C/&R, &P, &D, fonts)       < 420
│   ├── canvas.rs             # CanvasSpec -> VectorSlide (single compiler)  < 560
│   ├── source.rs             # SheetSource + PageSource impls              < 340
│   ├── corpus.rs             # SearchCorpus over display text              < 240
│   ├── limits.rs             # bounds, budgets, catch_unwind               < 240
│   └── dump.rs               # workbook/sheet/cell serialisation           < 400
└── tests/
    ├── detect_tests.rs
    ├── geometry_tests.rs
    ├── numfmt_tests.rs
    ├── print_tests.rs
    ├── drawing_tests.rs
    └── golden_tests.rs
```

---

## 5. Package Model & Detection

### 5.1 Detection
`sniff_container_format` opens the ZIP central directory (bounded read, no extraction) and inspects `[Content_Types].xml` overrides:

| Condition | Result |
| :--- | :--- |
| `xl/workbook.xml` present, or content type `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml` (or `.template.main`, `.macroEnabled.main`, `.template.macroEnabled.main`) | `SourceFormat::Xlsx` |
| `ppt/presentation.xml` | `SourceFormat::Pptx` |
| `word/document.xml` | `SourceFormat::Docx` |
| CFB/OLE2 with `EncryptedPackage` | `ParseError::PasswordProtected` |
| CFB/OLE2 with `Workbook`/`Book` stream | `ParseError::LegacySpreadsheet` (convert to `.xlsx`) |
| ZIP with `xl/workbook.bin` | `ParseError::LegacySpreadsheet` (`.xlsb` not supported) |
| ZIP with `mimetype` = `application/vnd.oasis.opendocument.spreadsheet` | `ParseError::UnsupportedFormat` (`.ods`, convert to `.xlsx`) |
| other ZIP | `ParseError::UnsupportedFormat` naming the content types found |

Detection is content-driven; the file extension is never authoritative.

### 5.2 Parts consumed

| Part | Use |
| :--- | :--- |
| `xl/workbook.xml` | sheet list & order, `date1904`, `calcPr`, `definedNames`, active tab |
| `xl/_rels/workbook.xml.rels` | sheet part resolution (never by index guess) |
| `xl/worksheets/sheetN.xml` | `dimension`, `sheetFormatPr`, `cols`, `sheetData`, `mergeCells`, `sheetViews`/`pane`, `printOptions`, `pageMargins`, `pageSetup`, `headerFooter`, `rowBreaks`/`colBreaks`, `drawing`, `legacyDrawing`, `picture`, `extLst` (sparklines) |
| `xl/sharedStrings.xml` | SST, rich runs, `rPh` phonetic (recorded, not rendered) |
| `xl/styles.xml` | `numFmts`, `fonts`, `fills`, `borders`, `cellXfs`, `dxfs`, `cellStyles` |
| `xl/theme/theme1.xml` | `clrScheme`, font scheme for theme indices |
| `xl/drawings/drawingN.xml` | anchors, `xdr:pic`, `xdr:sp`, `xdr:grpSp`, `xdr:graphicFrame` |
| `xl/charts/chartN.xml` | DrawingML chart parts |
| `xl/charts/chartExN.xml` | new chart families (detected → PLACEHOLDER) |
| `xl/chartsheets/sheetN.xml` | chart-sheet tabs (full-page chart) |
| `xl/media/*` | images (PNG/JPEG/GIF/BMP/TIFF/EMF/WMF/SVG) |
| `xl/vbaProject.bin` | encountered, never read |

`calcChain.xml`, `pivotCache*`, `pivotTables/*`, `comments*`, `tables/*`, `slicers/*`, `externalLinks/*`, `queryTables/*` are **not** parsed; each presence that affects rendering emits one deduplicated warning (e.g. comments → `dump --comments` hint).

### 5.3 Part-level hardening
- XML parsed with external DTD resolution and entity expansion disabled.
- Element depth ≤ 256; attribute count ≤ 512 per element.
- Every part read under a byte budget; a governed crossing yields `ParseError::ResourceLimit` (never `CorruptPart`).
- Unknown namespaces/elements are skipped, not fatal; unknown *sheet* content produces a warning, not a load failure.

---

## 6. Grid Geometry & Metrics

### 6.1 Metrics profile
`MetricsProfile` is resolved once per workbook from the Normal style font (`cellStyles[xfId=0]` → `cellXfs[0]@fontId` → `fonts[i]`), adapting the verified geometry algorithms from upstream `packages/xlsx/src/grid-geometry.ts`, then cached:

| Field | Resolution |
| :--- | :--- |
| `mdw_px` | Maximum advance, in whole pixels at 96 DPI, over digits `0..=9`, **with bitmap-metric preference where available**; else `round(advance_funits / units_per_em * round(size_pt / 72 * 96))` |
| `pixel_padding` | `2 * ceil(mdw_px / 4) + 1` (Excel's own derivation; 5 px for MDW 7) |
| `row_px_per_pt` | `96 / 72` |
| `default_row_px` | `sheetFormatPr@defaultRowHeight` (default 15 pt → 20 px) |
| `default_col_px` | `px(col_width(8.43))` when `defaultColWidth` is absent |

**Bitmap-metric caveat (normative).** `ttf-parser`/`fontdb` expose outline metrics only; the EBDT/EBLC bitmap advance that Excel prefers is not available. A small built-in override table reproduces the observed Excel values for the bundled and most common families (Calibri 11 pt → MDW 7, Arial 10 pt → 7, Cambria 11 pt → 7, Courier New 10 pt → 6); everything else uses the outline formula. The profile is surfaced in `ot info` output and can be overridden with `xlsx.metrics = { mdw_px = 7 }` in `config.toml`, or per-invocation `--metrics-mdw`.

**Column width → pixels (normative):**

$$\text{px} = \operatorname{round}(\text{width} \times \text{MDW}) + \text{PP}$$

where `width` is `cols/col@width` (MDW units, fractional) or `sheetFormatPr@defaultColWidth`, and `PP` is the pixel padding above. `cols/col@width` may be absent while `customWidth="1"` is present; then `bestFit="1"` triggers content measurement (§7.4), otherwise the default width applies.

### 6.2 Prefix sums and lookup
`GridModel` precomputes monotone prefix arrays of column x-offsets and row y-offsets (f64 EMU) over the **used range only**, plus a `hidden` bitmap and outline-level array. Lookups are binary-search `O(log n)`; cell rects are `O(1)`. Rows/columns beyond the used range are synthesised from defaults and never materialised.

### 6.3 Coordinate conversion
- Spreadsheet units: column widths in MDW; row heights in points; EMU everywhere in the display list.
- `1 px = 9,525 EMU`, `1 pt = 12,700 EMU` ([AGENTS.md](../AGENTS.md) §4).
- Column `x` in sheet EMU = `prefix_col_emu(col)` where each column contributes `px * 9525` at scale 1.
- Row `y` in sheet EMU = `prefix_row_emu(row)` where each row contributes `pt * 12700`.
- A cell's visual rect spans `[prefix(n), prefix(n+1)]`; the **gridline** sits at `prefix(n)` and is stroked at 1 px (9525 EMU), drawn at half-pixel inset so adjacent cells do not double-stroke (§9.3).

### 6.4 Hidden, grouped and collapsed
`hidden="1"` contributes zero width/height but retains its index. `outlineLevel` is recorded; outline **glyphs** (the `+`/`-` margin buttons) are drawn in print chrome only when `printOptions@headings="1"` (§12.5). No interactive collapsing in this version (§23).

---

## 7. Cell Content Compilation

### 7.1 Cell kinds

| `c@t` | Meaning | Handling |
| :--- | :--- | :--- |
| *(absent)* / `n` | number | `f64` from `<v>`; display via number format (§8) |
| `s` | shared string | SST index; rich runs preserved |
| `inlineStr` | inline string | `<is>` with runs |
| `str` | formula string result | text as stored |
| `b` | boolean | `TRUE`/`FALSE` in the cell's own uppercase locale form |
| `e` | error | `#DIV/0!`, `#N/A`, … rendered verbatim, styled as text |
| `d` | ISO-8601 date | parsed to a serial date, then formatted |

### 7.2 Text layout pipeline
For each cell with text: resolve style → resolve font (family/size/bold/italic/underline/color, substituting through the [PRD 02c](./02c-typography-and-text-layout.md) substitution matrix) → measure → lay out:

1. **Explicit line breaks** (`\n`, `_x000D_`, `char(10)`) split into logical lines.
2. **`wrapText="1"`** wraps at word boundaries within the cell's inner width (cell width − left/right padding − indent); overflow words break by character when a single token exceeds the width.
3. **`shrinkToFit="1"`** scales the font by a single uniform factor `min(1, inner_w / text_w)` computed with [PRD 02c](./02c-typography-and-text-layout.md) metrics; never below 40 %, else falls back to clipping.
4. **Neither wrap nor shrink** → single line, clipped to the cell rect, except **overflow spill** (§7.3).
5. **Vertical alignment** places the line block inside the cell (top/center/bottom/justify/distributed); **horizontal** handles general/left/center/right/fill/justify/centerContinuous/distributed, plus `indent` in MDW units.
6. **`textRotation`**: `0..=90` = counter-clockwise degrees, `91..=180` = clockwise `(value − 90)`, `255` = stacked vertical text (one glyph per line, centred). Implemented as a rotated `PositionedGlyphRun` transform about the cell centre; the cell rect is unchanged (Excel does not reflow neighbours).

### 7.3 Overflow spill
Text spills into empty neighbouring cells when: alignment is `general` or `left`, the cell is not wrapped/shrunk, and each traversed neighbour is genuinely empty (no value, no style-only fill that would clip). Spill is clipped at the first non-empty neighbour, at merged-range boundaries, and at the canvas clip rect. Numeric and boolean values never spill (they widen their slot instead → `###` clipping, §8.7).

### 7.4 Auto-fit measurement
`bestFit="1"` columns and rows with `customHeight="0"` are measured with the same metrics engine and rounded up: column = `text_w + 2 + mdw/4` in px per the documented Excel approximation; row = `((units_per_em − win_descent) + 2 × win_descent) / units_per_em × size_px`, rounded up. This is the only place where our values are explicitly approximations and it is reported as `DEGRADED` (§17).

### 7.5 Merged cells
`mergeCells/mergeCell@ref` builds a union-find over the used range. Each merged region is drawn by its **anchor** cell: one fill, one border ring (outer edges use the anchor's border, interior edges are suppressed), text laid out in the merged rect with the anchor's alignment, truncated to the merged area. Non-anchor cells inside a merge contribute their values to `dump` only. Overlapping/duplicate merge ranges (a known corruption) are deduplicated with a warning.

---

## 8. Value Formatting (Number Format Engine)

### 8.1 Scope
`numFmtId` resolves through `styles.xml`: built-in ids 0–49 are hard-coded per ECMA-376 §18.8.30, ids ≥ 164 resolve to `numFmts/numFmt@formatCode`. Custom codes are parsed into an AST and formatted against the cell value. The grammar parser and formatting algorithms in `src/value/numfmt_*.rs` adapt the battle-tested, spec-compliant reference implementation from upstream `packages/xlsx/src/value/numfmt.ts` in `yukiyokotani/office-open-xml-viewer`, ensuring exact parity on edge cases without trial-and-error.

### 8.2 Grammar (fully implemented)
- **Four sections** `positive;negative;zero;text` with Excel's fallbacks (1 section → all; 2 → positive+zero, negative; 3 → text from `@` elsewhere). Empty trailing sections hide the value (`;;;` renders nothing).
- **Conditional sections** `[>=1000]`, `[<0]`, `[=5]`, `[<>0]` — evaluated left to right, independent of sign.
- **Placeholders** `0` (forced digit), `#` (optional digit), `?` (aligning space), decimal point, thousands separator, scaling commas (`0,,` = millions), `%`, `/` fractions with `?/?`, `??/??`, `/16`, scientific `E+00`/`e-0`.
- **Literals and escapes** `"text"`, `\x`, `_x` (width of `x`), `*x` (repeat fill to width), `@` (text substitution).
- **Colors** `[Red]`, `[Blue]`, `[Color10]`, `[Color 3]`.
- **Locale/currency prefixes** `[$-409]`, `[$€-407]`, `[$¥-411]` — the currency token is inserted, the locale identifier only affects decimal/thousands grouping choice.
- **Elapsed time** `[h]:mm:ss`, `[m]:ss`, `[s]`.
- **Fractions** `# ?/?` with reduced denominators, fixed denominators, and mixed numbers.
- **Accounting alignment** (built-in 37–44) — literal `_`/`*` padding reproduced as spacing.

### 8.3 `General`
Excel's `General` is not a format code but a display policy: up to 11 significant characters, switching to scientific when the magnitude overflows, trimming trailing zeros, rendering integers without a decimal point, and honouring column width (§8.7). Implemented as its own formatter, not by synthesising a code.

### 8.4 Dates and times
- Serial `1.0` = 1900-01-01 with the 1900 leap-year bug reproduced exactly (serials 60 = the phantom 29-Feb-1900).
- `workbookPr@date1904="1"` shifts the epoch by 1462 days.
- Time-only fractions, `AM/PM`, `A/P`, month names, weekday names in English (the only locale table shipped; `[$-xxx]` affects separators, not names — documented in §17).
- Format-less but date-like cells (a numeric cell whose style came from a date category) **are not** guessed: display follows the style's `numFmtId`, and a missing style renders `General`. No heuristic reclassification.

### 8.5 Errors and forced text
- `t="e"` values render verbatim.
- A cell whose `numFmtId` resolves to text (`@`) forces its value to text ("displayed as typed").
- Leading-apostrophe text is already a string in the file; no stripping.

### 8.6 Uncalculated formulas
A cell with `<f>` but no `<v>`/`<is>` is **uncalculated**. It compiles to a placeholder run `#UNCALC` (§17, `PLACEHOLDER`) plus one `WarningEvent { code: UncalculatedCell, sheet, cell, formula }` per sheet, deduplicated and capped (first 20, then a count). No evaluator is attempted — decision **D5**.

`--formulas` renders `<f>` text instead of the value, monospace, with a per-cell `F` badge in the status chrome.

### 8.7 Width-driven re-formatting
When a numeric value overflows its cell width and the neighbour is non-empty, Excel re-formats rather than clips: `General` falls back to fewer significant digits and then to scientific; fixed formats fall back to `#`. When even that fails the cell shows `###` repeated to fill the width, right-aligned. All three behaviours are reproduced; the `###` case is `DEGRADED` with a warning capped at one per sheet.

---

## 9. Styles, Theme & Paint Resolution

### 9.1 Resolution chain
`cellXfs[i]` → (`numFmtId`, `fontId`, `fillId`, `borderId`, `alignment`, `protection`, apply-flags). Apply-flags are honoured where Excel honours them (`applyNumberFormat`, `applyFont`, `applyFill`, `applyBorder`, `applyAlignment`); a `0` flag with a non-zero target still applies when the target differs from the cell style's base, matching Excel's observed behaviour.

### 9.2 Theme and tint
`theme="n"` resolves through `theme1.xml` `a:clrScheme` in schema order (dk1, lt1, dk2, lt2, accent1–6, hlink, folHlink), preferring `a:srgbClr@val`, falling back to `a:sysClr@lastClr`, then to a fixed default. `tint` applies the ECMA HLS transform. `indexed="n"` resolves through the legacy indexed palette (0–63) and `auto="1"` yields the automatic foreground (black on light).

### 9.3 Borders
All 14 line styles (`thin`, `medium`, `dashed`, `dotted`, `double`, `hair`, `mediumDashed`, `dashDot`, `mediumDashDot`, `dashDotDot`, `mediumDashDotDot`, `slantDashDot`, `thick`) map to `VectorStroke` dash patterns and widths in EMU; `double` emits two strokes. Adjacent cells sharing an edge draw **one** stroke at the shared coordinate (right edge wins over left edge, bottom over top, matching Excel's precedence), so no edge is double-painted. Diagonal borders (`diagonalUp`, `diagonalDown`) are drawn corner-to-corner and clipped to the cell. Border colour resolution is per-side.

### 9.4 Fills
`solid` → `VectorPaint::Solid`; `pattern` (18 patterns) → a generated 8×8 tile emitted as a repeated `DrawPath` set (not a bitmap, preserving SVG scaling); `gradientFill` → a linear/radial `VectorPaint` gradient using the existing style engine, or a two-stop average as `DEGRADED` when path/angle is unsupported.

### 9.5 Conditional formatting (`dxfs`)
Applied in rule order, later rules painting over earlier ones:
- `cellIs` / `containsText` / `beginsWith` / `endsWith` / `containsBlanks` / `notContainsBlanks` / `duplicateValues` / `uniqueValues` / `top10` / `aboveAverage` — evaluated statically over the used range (no formula interpreter needed except for the literal comparand).
- `colorScale` (2- and 3-point, `min`/`max`/`percent`/`percentile`/`num`/`formula`) — value interpolation in sRGB.
- `dataBar` — gradient/solid bar with optional value axis, negative fill, `showValue`.
- `iconSet` — 3/4/5 symbol sets drawn as vector glyph marks (arrows, flags, traffic lights, ratings, quarters, stars, boxes) with `>=`/`>` thresholds.
- `expression` / `formula` / `timePeriod` rules — **not evaluated**; the range is left unformatted and one `ConditionalExpressionUnsupported` warning is emitted per sheet (§23).
- `stopIfTrue` is honoured across the evaluable rule kinds.

### 9.6 Protection and ignored attributes
`protection@locked/hidden` are parsed for `dump` but have no visual effect. `quotePrefix`, `phonetic`, `readingOrder` (visual only for RTL; recorded, base direction applied to runs).

---

## 10. Drawings: Charts

### 10.1 Reuse, not reimplementation
`xl/charts/chartN.xml` maps into the **existing** core chart model (`ChartFamily`, series with `name` + `values: Vec<Option<f64>>`, categories, axes, legend, styles) consumed by `crates/office-terminal-core/src/compiler/charts/`. Every renderer already present — bar, column, stacked/percent variants, line, area, pie, doughnut, scatter, bubble, radar, stock, surface — applies unchanged, including SVG export with selectable axis labels.

Mapping rules:
- `c:barChart`/`c:barDir@val` selects bar vs column; `grouping` selects stacked/percent.
- `c:ser` → series: `c:tx` (name), `c:val` → `c:numRef`/`c:numLit` → `c:numCache/c:pt` (sparse `idx` honoured), `c:cat` → `c:strRef/c:strCache` or numeric categories.
- `c:gapWidth`, `c:overlap`, `c:dLbls`, `c:legend`, `c:title`, `c:plotVisOnly`, `c:dispBlanksAs` map onto the model.
- `c:marker`, `c:smooth`, `c:hiLowLines`, `c:upDownBars` map where the model supports them; otherwise DEGRADED with a per-chart warning.
- 3-D families (`c:view3D`, `bar3DChart`, `pie3DChart`, …) render **flat** (2-D projection) with one `ChartFlattened` warning per chart.
- `c:chart@f` (chart-sheet style reference) and `c:externalData` are ignored.

### 10.2 Chart data binding
Primary source is the chart part's own cache (`numCache`/`strCache`) — no sheet resolution, no evaluator (D5). When the cache is absent or its `ptCount` disagrees with the preceding `c:f` range extent, the range is resolved against the workbook grid; if that also fails, `ChartDataMissing` → PLACEHOLDER box with the chart title.

### 10.3 Chart sheets
`xl/chartsheets/sheetN.xml` is a **tab whose entire surface is one chart**, with its own `pageSetup`/`pageMargins`. It contributes one sheet kind `SheetKind::ChartSheet` and exactly one print page. In grid mode it compiles to `compile_canvas` with `chrome = None` and the chart filling the viewport.

### 10.4 `chartEx` and unknown families
`xl/charts/chartExN.xml` (treemap, sunburst, funnel, waterfall, histogram, box-whisker, region map) is **detected and not rendered**: a titled placeholder box at the anchor with one `ChartExUnsupported` warning per chart, and its tab still appears in the sheet tabs / page list. Rationale in §23.

### 10.5 Sparklines
`extLst/x14:sparklineGroups` render as inline marks in the host cell: `line` (polyline), `column` (bars from baseline), `stacked`, plus `winLoss`; `markers`, `high`, `low`, `first`, `last`, `negative` point colours are honoured from the group's `sparkline` colors. Sparkline axis min/max from `x14:sparkline@max`/`min` or the group's manual/group scale. No interaction, no hover.

---

## 11. Drawings: Pictures, Shapes & Objects

### 11.1 Anchor → EMU
Adapts the coordinate resolution formulas from upstream `packages/xlsx/src/drawings/anchor.ts`:
```
x_emu = col_prefix(from.col) + from.colOff
y_emu = row_prefix(from.row) + from.rowOff
w_emu = col_prefix(to.col)   + to.colOff   − x_emu      // twoCellAnchor
h_emu = row_prefix(to.row)   + to.rowOff   − y_emu
```
`oneCellAnchor` takes `xdr:ext` (cx/cy in EMU); `absoluteAnchor` takes `xdr:pos` + `xdr:ext` directly. Because column prefixes come from §6, object placement is font-metric dependent exactly like cell text — a single shared solver prevents drift.

### 11.2 Z-order
Drawing children compile in document order (later on top), between two layers: **below** is the cell layer (fills, gridlines, text), **above** is print chrome (§12.5) and the grid's frozen bands/overlays. `xdr:grpSp` is flattened with inherited transforms; `xdr:cNvPr@hidden="1"` is skipped.

### 11.3 Pictures
- Formats: PNG, JPEG, GIF, BMP, TIFF via `image`; EMF/WMF/EMF+ through `office-terminal-core::metafile` ([PRD 13](./13-metafile-pictures-and-vector-playback.md)); SVG through `a14:svgBlip` (`r:embed` in the extension namespace) via `resvg`/`svg_image_path`.
- `a:srcRect` crop (`l/t/r/b` in 1/1000 %) crops the source rect; crop clamping reuses the shared logic from `crates/patches/0003` rather than a second implementation.
- `a:alphaModFix@amt` maps to `DrawBitmap` opacity; `a:xfrm@rot` rotates about the anchor centre.
- Decompression and decode are budgeted (§18): decoded bytes are charged once per distinct part, cached by part name, and downscaled to display size when the stored resolution exceeds the anchor's device pixels.

### 11.4 Worksheet shapes and pictures-as-background
`xdr:sp` (text boxes, rectangles, callouts, freeforms) compile through the existing shape compiler; `a:prstGeom` presets reuse [PRD 02b](./02b-vector-style-and-geometry-engine.md)/[PRD 11](./11-extended-and-tier3-shape-presets.md). Worksheet `<picture r:id>` (a tiled sheet background) is drawn beneath the cell layer, tiled at the image's native size, and is `DEGRADED` when the source is a metafile.

### 11.5 Print participation
`xdr:clientData@fPrintsWithSheet="0"` excludes the object from **print pages only**; the grid surface still shows it (this is what Excel does). `fLocksWithSheet` is recorded in `dump` and has no behavioural effect. `xdr:twoCellAnchor@editAs` is recorded; since column/row resizing is out of scope (§23), it produces no observable change and is documented as such.

---

## 12. Print Pagination Engine

### 12.1 Page list construction
For each sheet (in workbook order, respecting `sheet@state`): if the sheet is hidden it still contributes pages only when `printOptions` requests it — hidden sheets are **omitted from print pages** but listed in `dump`. Chart sheets contribute one page each. Worksheet pages are produced by §12.2–§12.6, flattened in `pageOrder` order.

### 12.2 Print range
1. `definedNames/definedName[@name="_xlnm.Print_Area"][@localSheetId=sheetIdx]` → one or more `Sheet!$A$1:$H$42` areas (a union prints each area starting on a new page, in listed order; `&P` numbering continues across areas unless `useFirstPageNumber="1"`).
2. Otherwise `dimension/@ref`, else the bounding box of non-empty cells.
3. `_xlnm.Print_Titles` supplies repeat rows (`$1:$2`) and repeat columns (`$A:$B`), applied to every page of that area.

### 12.3 Page setup
`pageSetup` provides `orientation` (default portrait), `paperSize` (the ~60 ECMA codes → physical mm), `scale` (10–400 %, default 100), `fitToWidth`/`fitToHeight`, `pageOrder` (`downThenOver` default / `overThenDown`), `blackAndWhite`, `draft`, `cellComments`, `errors`, `horizontalDpi`/`verticalDpi`, `firstPageNumber`+`useFirstPageNumber`. `sheetPr/pageSetUpPr@fitToPage="1"` enables fit mode; when enabled, `scale` is ignored and **manual breaks are suppressed** (matching Excel: fit-to overrides manual breaks).

`pageMargins` defaults are `left/right 0.7`, `top/bottom 0.75`, `header/footer 0.3` inch.

### 12.4 Fit solver
Let `N_w`, `N_h` be the natural print-range size in points (px × 0.75), `A_w`, `A_h` the printable area (paper − margins). With `fitToPage` on and `fitToWidth=w`, `fitToHeight=h` (0 meaning "as many as needed"):

$$\text{scale} = \min\!\left(\frac{A_w \cdot \max(w,1)}{N_w},\ \frac{A_h \cdot \max(h,1)}{N_h}\right)$$

clamped to `[0.10, 4.00]`. The resulting page counts are `ceil(N_w / (A_w / scale))` and `ceil(N_h / (A_h / scale))`, then re-derived from actual break positions so that the emitted page count and the solver never disagree. `scale = 1.0` when fit mode is off.

### 12.5 Page chrome
- **Gridlines**: printed iff `printOptions@gridLines="1"` **and** `@gridLinesSet="1"`. Colour is not stored per sheet; the default light grid tint (`#D0D7DE`) is used, configurable via `xlsx.gridline_color`.
- **Headings**: `printOptions@headings="1"` adds row numbers and column letters in the margin band, styled with the workbook's Normal font.
- **Centering**: `horizontalCentered` / `verticalCentered` offset the content block inside the printable area.
- **Clipping**: content is clipped to the page rect; a chart or picture straddling a page boundary is drawn in both pages, clipped to each (Excel behaviour).
- **`blackAndWhite`** maps all fills to white and strokes to black; **`draft`** drops pictures, charts and fills (documented Excel semantics).
- **`cellComments`** and `errors` (`displayed`/`blank`/`dash`/`NA`) are reproduced; comments themselves are `OMITTED` (§23) so `cellComments` yields the blank form.

### 12.6 Breaks
Manual `rowBreaks/brk@id` (0-based, break **after** that row, `@man="1"`) and `colBreaks` are honoured unless fit mode is active. Automatic breaks are inserted at the last row/column whose accumulated extent fits the scaled printable area. Page order and numbering follow `pageOrder` and `useFirstPageNumber`.

### 12.7 Headers and footers
`headerFooter` `oddHeader`/`oddFooter` (+ `evenHeader`/`evenFooter` with `differentOddEven`, `firstHeader`/`firstFooter` with `differentFirst`) are parsed with the full field-code grammar:
`&L` `&C` `&R` sections · `&P` page · `&N` total pages · `&D` date · `&T` time · `&F` file name · `&A` sheet name · `&"Font,Style"` · `&nn` size · `&B` `&I` `&U` `&E` (double underline) `&S` (strike) · `&X` (superscript) `&Y` (subscript) · `&&` literal · `&[Picture]` → PLACEHOLDER marker (§23). Header/footer text is laid out in its own EMU band at the margin offsets, each of the three sections independently aligned.

---

## 13. Display List Compilation

### 13.1 Single compiler
`compile_canvas(spec)` performs, in order:

1. Resolve `window` → visible cell range via `GridModel` (binary search on both axes).
2. Emit worksheet background (`sheetPr@tabColor` is *not* the background; a background picture covers the sheet if present).
3. Emit cell layer (fills, then borders, then gridlines if chrome requires, then text) for visible cells only, including merged regions whose anchor or extent intersects the window.
4. Emit conditional-format overlays.
5. Emit drawings in z-order from `draw/*`.
6. Emit sparklines (cell-local, in the cell layer).
7. Emit chrome: frozen-pane separators, headings, header/footer bands, page border shadow (print surface only).
8. Return `VectorSlide` with `page_size_emu = spec.viewport` size, plus `VectorSlide::warnings` aggregated as `WarningEvent`s.

**Skip-set optimisation.** Cells with no value, no fill, no non-default border and no conditional rule are not emitted; the grid's default line work is stroked once per row/column band rather than per cell, cutting display-list size by an order of magnitude on sparse sheets.

### 13.2 Tiles and caching
Grid surface requests **tiles**: `CanvasSpec` whose `window` equals `viewport`, sized to the visible cell area plus one tile of look-ahead margin. Tile identity is `(sheet, window_cells, scale_class)`; the pipeline's Tier-2 cache holds up to `xlsx.tile_cache` (default 64) tiles. Panning inside a cached tile issues **no compile** — the driver re-places with a new source rect ([PRD 14a](./14a-pdf-reader-ui-designs.md) §3.1). Zoom steps change `scale_class` and recompile the visible tiles only.

### 13.3 Text corpus and links
`SearchCorpus` pages = sheets; `page_text(sheet)` returns one `TextLine` per non-empty cell whose `text` is the **displayed** string (§8) and whose `rect` is the cell's sheet-EMU rect. Cell addresses are recoverable from the rect via `GridModel` and are attached for the find overlay. Hyperlinks (`cell/hyperlink`) map to the existing `SlideLink` seam as external URLs only; internal `#Sheet!A1` links are `OMITTED` (§23).

### 13.4 Determinism
Compilation is a pure function of `(Workbook, CanvasSpec, MetafileMode)`. No clocks, no locale, no randomness, no parallel-order dependence: the same inputs produce byte-identical display lists and identical SVG.

---

## 14. TUI: Sheet Grid Surface

### 14.1 Layout

```
┌ address / formula bar ───────────────────────────────────────────────┐
│ fx  B3   =SUM(B2:B5)                     1,842,300                  │
├─────────┬───────────────────────────────────────────────────────────┤
│  frozen │ scrolling viewport                                        │
│  bands  │ (Kitty tile placements, source-rect panned)               │
├─────────┴───────────────────────────────────────────────────────────┤
│ sheet tab bar                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ status line: stats · zoom · help                                     │
└─────────────────────────────────────────────────────────────────────┘
```

Chrome is ratatui text; the sheet area is one to four Kitty placements (corner / top / left / body) from the resident tile pool. Gridline and cell visuals are **content**, not chrome — no `Borders::*` is ever drawn around the viewport (the [PRD 14a](./14a-pdf-reader-ui-designs.md) §4.1 "no stray `─`" rule applies unchanged).

### 14.2 New application mode
`AppMode::Sheet` (`crates/office-terminal-tui/src/app/mode.rs`) with `SheetState` (`sheet_idx`, `cell`, `anchor`, `scroll`, `zoom`, `fit`, `show_formulas`). Sorter gains `SorterKind::{Pages, Sheets}`; `HelpModal` gains a `SHEET_HELP` table sourced from the same keymap as the handler, per `help.rs`'s existing rule.

### 14.3 Keymap (normative)

| Key | Action |
| :--- | :--- |
| `h` `j` `k` `l`, arrows | move cell cursor (scrolls the window only when the cursor crosses an edge) |
| `Ctrl+→` `Ctrl+←` `Ctrl+↑` `Ctrl+↓` | jump to the data edge in that direction (Excel semantics) |
| `Ctrl+Home` / `g` | `A1` / open ` Jump to cell: B42_ ` prompt (digit-buffer style of `normal.rs:46`) |
| `PageDown` `PageUp` | one viewport down/up, keeping the column |
| `d` `u` | half viewport down/up |
| `Shift+arrows` | extend the selection from the anchor |
| `Ctrl+A` | select the used range |
| `]` `[` | next / previous sheet (chart sheets included) |
| `+` `-` `=` | zoom in / out / reset (ladder 10–400 %) |
| `f` | cycle fit: none → fit-width → fit-sheet |
| `F` | toggle `--formulas` display |
| `/` · `Ctrl+F` | find across sheets (`FindModal` over `SearchCorpus`) |
| `Ctrl+G` | go-to prompt (same as `g`) |
| `?` | help |
| `q` `Esc` | quit (Esc first clears the selection, then quits) |

### 14.4 Status line (normative format)
```
budget.xlsx · Sheet2 "Staffing" 2/4 · B3:D9 · Sum 4,335,450 · Avg 216,772 · Count 20 · 100% · ?:help · q:quit
```
Segment order: file · sheet name + index · selection · statistics (only for numeric selections ≥ 2 cells, in Excel's order sum/avg/count/min/max) · zoom · key hints. Separator and styling reuse `reader/status.rs` (` · `, white on dark grey, no border).

### 14.5 Frozen panes
`sheetView/pane@xSplit|ySplit|topLeftCell|state` produces up to four independently scrolling regions (`frozen`, `frozenSplit`, `split` treated as frozen). Each region is a separate placement with its own source rect, so the frozen band is never re-compiled when the body pans. `topLeftCell` is the initial scroll anchor.

### 14.6 Selection rendering
The cursor is a 2 px inset outline; the selected range is a translucent overlay (`#1A6FE0` at 18 % alpha) with the anchor cell outlined. Both are emitted **into the display list** (§13.1 step 7) so they appear in PNG/SVG exports only when `--selection` is passed; the interactive TUI compiles them as chrome overlay tiles so panning does not dirty the body tile.

---

## 15. TUI: Print-Page Surface

### 15.1 Reader mode
Print pages are ordinary pages: `AppMode::Reader` consumes `Arc<dyn PageSource>` exactly as it does for PDF/DOCX ([PRD 14a](./14a-pdf-reader-ui-designs.md)), inheriting continuous scroll, spread, focus cycling (`BackTab`/`w`/`\`), fit cycling (`f`), zoom ladders, TOC modal and find. Status line is unchanged: `{file} · p.N/M · fit:{mode} {zoom}% · ?:help · q:quit`. Sheet and print-page numbers are additionally shown by `ot info`, not in the status bar, to avoid a second numbering system.

### 15.2 Slide mode and presenter console
`--mode slides` and `present` treat pages as slides: the existing `fullscreen`/`PresenterConsole` layouts, timer, gauge, sorter and notes apply. Notes are empty (Excel has no speaker-notes model); the presenter's notes pane shows the sheet name and print-page address range for the current page instead of an empty box.

### 15.3 Sorter
`SorterKind::Pages` renders print-page thumbnails with titles ` Sheet "Staffing" 3/12 `; `SorterKind::Sheets` (grid surface's `Tab`) renders one card per sheet showing its used range at thumbnail scale — including chart-sheet cards.

---

## 16. CLI, Config & Export

### 16.1 Command matrix

```bash
ot view book.xlsx                          # Sheet Grid (default)
ot view book.xlsx --sheet 2                # open on sheet 2
ot view book.xlsx --mode print             # print pages in Reader mode
ot view book.xlsx --mode slides            # print pages as slides

ot present book.xlsx                       # print pages in the presenter console

ot export book.xlsx -o out/ --format svg   # one SVG per print page
ot export book.xlsx -o out/ --format png --dpi 200 --pages 1-3
ot export book.xlsx -o out/ --sheet 2 --format svg     # whole sheet, one file
ot export book.xlsx -o out/ --sheet 2 --cells A1:H40 --format png --width 2400

ot info book.xlsx
ot dump book.xlsx --format csv --sheet all > data/
ot dump book.xlsx --format json --formulas > cells.json
```

### 16.2 New arguments

| Flag | Commands | Semantics |
| :--- | :--- | :--- |
| `--sheet <NAME\|INDEX>` | view, export, dump | sheet selection (1-based index or tab name) |
| `--cells <REF>` | export | A1 range: `A1:H40`, `A:H`, `1:40`, `A1:H` |
| `--pages <SPEC>` | export, present | reuses `parse_slide_range` ("1-3,5,10-") |
| `--formulas` | view, export, dump | render formula text instead of values |
| `--metrics-mdw <PX>` | view, export, info | override `MetricsProfile::mdw_px` |
| `--gridlines <on\|off>` | view, export | override `printOptions` on the grid surface |
| `--selection` | export | include cursor/selection chrome in output |

Dump gains `--format csv|tsv` (one file per sheet under `-o`, or stdout for a single sheet), and `--sheet` selects. `--json` emits a workbook → sheets → cells tree with `ref`, `value`, `display`, `type`, `formula`, `style_id`, `merge`, `conditional_rules`.

The existing `--mode` value enum (`auto|slides|reader`) gains `print` (print pages in Reader mode) and `sheet` (grid surface). `auto` resolves `.xlsx` to `sheet`; `slides`/`reader`/`print` all use the paginated surface, so `present` requires no new mode.

### 16.3 Output naming
`{stem}-page-{NN}.svg|png` for print pages; `{stem}-sheet-{index}-{slug}.svg|png` for whole-sheet export; `{stem}-sheet-{index}-{slug}.csv` for dumps. Deterministic, zero-padded, matching the existing deck exporter conventions.

### 16.4 Feature gating
`office-terminal-cli` gains its first `[features]` table:

```toml
[features]
default = ["pdf", "xlsx"]
pdf = ["dep:office-terminal-pdf"]
docx = ["dep:office-terminal-docx"]   # entry activates when PRD 15 lands
xlsx = ["dep:office-terminal-xlsx"]
```

With `--no-default-features`, a workbook yields `ParseError::UnsupportedFormat("built without the `xlsx` feature")` rather than a linker error, and `ot info` reports the enabled input formats.

### 16.5 Config keys (`config.toml`)

```toml
[xlsx]
metrics            = { mdw_px = 7 }   # optional override
gridline_color     = "#D0D7DE"
tile_cache         = 64
max_sheets         = 512
max_cells          = 5000000
decoded_image_mb   = 128
formulas           = false            # show formulas by default
freeze_panes       = true
```

---

## 17. Fidelity Policy Matrix

Tiers per [PRD 02d](./02d-element-renderers-and-fidelity-policy.md).

| Category | Tier | Behaviour & diagnostics |
| :--- | :--- | :--- |
| Cell values, text, numbers, booleans, errors | **FULL** | Exact display strings (§8) |
| Shared strings & rich runs | **FULL** | Per-run font/size/colour/underline, super/subscript |
| Solid fills, theme colours, tint | **FULL** | ECMA HLS tint + theme resolution |
| Borders (all 14 styles), diagonals | **FULL** | Shared-edge single stroke |
| Alignment: 9 horizontal, 5 vertical, indent | **FULL** | Includes `centerContinuous`, `distributed` |
| Wrap, clip, overflow spill | **FULL** | §7.2–7.3 |
| `shrinkToFit` | **DEGRADED** | Uniform scale with a 40 % floor; below it, clip + warning |
| `textRotation`, stacked 255 | **FULL** | Rotated glyph runs about the cell centre |
| Merged cells | **FULL** | Anchor-owned paint, deduplicated overlaps |
| Number formats (built-in + custom) | **FULL** (documented subset of locale names) | English month/weekday names only; `[$-xxx]` affects separators |
| `General` display + width re-formatting | **FULL** | 11-significant-char policy, `###` fallback |
| Dates/times (1900 + 1904 epochs) | **FULL** | 1900 leap bug reproduced |
| Uncalculated formulas | **PLACEHOLDER** | `#UNCALC` + `UncalculatedCell` warning |
| Formula evaluation | **OMITTED** | Decision D5 (§23) |
| Conditional formatting: cellIs, top10, duplicate, contains*, colorScale, dataBar, iconSet | **FULL** | §9.5 |
| Conditional formatting: `expression`/`formula`/`timePeriod` | **PLACEHOLDER** | Range left unformatted + warning |
| Pattern & gradient fills | **DEGRADED** | Patterns as vector tiles; unsupported gradients average to two stops |
| Gridlines (screen & print) | **FULL** | Print requires `gridLines` **and** `gridLinesSet` |
| Auto-fit column width / row height | **DEGRADED** | Approximated per §7.4 with warning once per sheet |
| Frozen panes, split panes | **FULL** | Up to 4 regions |
| Sheet tabs, sheet order, hidden sheets | **FULL** | Hidden sheets omitted from print pages, listed in `dump` |
| Print area, print titles, fit-to, breaks, page order | **FULL** | §12 |
| Headers/footers, field codes, per-section fonts | **FULL** | Except `&[Picture]` → placeholder |
| DrawingML charts (2-D families) | **FULL** | Existing core renderers |
| 3-D charts | **DEGRADED** | Rendered flat + warning |
| `chartEx` (treemap, sunburst, funnel, waterfall, histogram, box-whisker, map) | **PLACEHOLDER** | Titled box + warning (§23) |
| Chart sheets | **FULL** | One page, own page setup |
| Sparklines | **DEGRADED** | Line/column/winLoss marks; no axes, no markers interaction |
| Pictures: PNG/JPEG/GIF/BMP/TIFF/SVG | **FULL** | Crops, transparency, rotation |
| Pictures: EMF/WMF/EMF+ | **FULL** | Through `metafile` playback |
| Worksheet shapes & text boxes | **FULL / DEGRADED** | Presets via shape engine; complex effects flatten to 2-D |
| Picture as sheet background | **DEGRADED** | Tiled; metafile sources degrade to a fill |
| `fPrintsWithSheet="0"` objects | **FULL** | Hidden on print pages, visible in grid |
| Pivot tables | **DEGRADED** | Materialised cells render; pivot semantics not recomputed |
| Comments, notes | **OMITTED** | `dump --comments` lists them; no visual |
| Data validation | **OMITTED** | No UI, no dropdowns |
| Slicers, timelines, form controls, OLE objects | **PLACEHOLDER** | Titled box + warning |
| External workbook links | **DEGRADED** | Cached value shown; link target recorded in `dump` |
| Hyperlinks (external) | **FULL** | Existing link seam |
| Hyperlinks (internal `#Sheet!A1`) | **OMITTED** | Rendered as styled text only |
| VBA / macros | **OMITTED** | Never read |
| Sheet/column/row protection | **OMITTED** | Visual only, no enforcement |
| Encrypted / password-protected workbooks | **ERROR** | `ParseError::PasswordProtected` |
| `.xls`, `.xlsb`, `.ods`, `.csv`, `.tsv` | **ERROR** | Typed `LegacySpreadsheet` / `UnsupportedFormat` with conversion hint |

---

## 18. Resource Limits, Security & Determinism

| Bound | Value | On crossing |
| :--- | :--- | :--- |
| ZIP expansion ratio | 50:1 | `ParseError::ResourceLimit` |
| Total decompressed bytes | 500 MB | `ParseError::ResourceLimit` |
| XML element depth | 256 | `CorruptPart` naming the part |
| Sheets per workbook | 512 | `ResourceLimit` |
| Rows per sheet (hard cap) | 1,048,576 | `ResourceLimit` |
| Columns per sheet | 16,384 | schema-implied; columns beyond are ignored with a warning |
| Non-empty cells per workbook | 5,000,000 | `ResourceLimit` |
| Shared-string entries | 2,000,000 | `ResourceLimit` |
| Merged ranges per sheet | 200,000 | truncated + warning |
| Drawing objects per sheet | 10,000 | truncated + warning |
| Charts per workbook | 500 | truncated + warning |
| Decoded image bytes (live) | 128 MB | evict LRU, then skip + warning |
| Tile cache | 64 tiles | LRU eviction |
| Single canvas compile | 250 ms budget | compile returns a `RenderError::Timeout`, pane keeps its previous raster |
| Total print-page construction | 10 s budget | remaining pages become `PLACEHOLDER` |

Security posture:
- No `unsafe` anywhere in the crate (`#![forbid(unsafe_code)]`).
- XML parsers configured without external DTD/entity resolution; no XInclude, no file URL fetch.
- No network access of any kind (external link parts are recorded, never followed).
- All `catch_unwind` boundaries are per-canvas and per-page; a panic degrades one tile, never the session.
- Path traversal in relationship targets is rejected before any part read.

---

## 19. Error Taxonomy & Diagnostics

New `ParseError` variants (`crates/office-terminal-core/src/error.rs`):

```rust
LegacySpreadsheet(&'static str),   // ".xls (BIFF)"; message names the conversion
UnsupportedFormat(String),         // .ods/.xlsb/unknown ZIP; includes detected content types
```

New `WarningCode` values (typed, deduplicated per sheet):

`UncalculatedCell`, `ChartExUnsupported`, `ChartFlattened`, `ChartDataMissing`, `ConditionalExpressionUnsupported`, `AutoFitApproximated`, `ShrinkToFitClipped`, `PatternFillDegraded`, `GradientFillDegraded`, `BackgroundPictureDegraded`, `SparklineDegraded`, `CommentsSkipped`, `PivotValuesStatic`, `ExternalLinkCached`, `ObjectTruncated`, `MergeCollision`, `TextSpillClipped`.

Every warning carries `(sheet_idx, Option<cell_ref>, detail)` and is reachable through the existing `WarningDrawer` (`W`) and `ot info`'s warning count.

---

## 20. Performance Budgets

| Scenario | Budget (p95) |
| :--- | :--- |
| Load + first grid tile, 5 MB workbook, ≤ 200 k cells | ≤ 150 ms |
| Pan inside a cached tile | ≤ 16 ms (re-placement only, no compile) |
| Tile compile, 1,000 visible cells with styles | ≤ 60 ms |
| Zoom step (visible tiles recompiled) | ≤ 120 ms |
| Print-page construction, 30 pages | ≤ 300 ms total |
| Whole-sheet SVG export, 50 k cells | ≤ 2 s |
| Resident memory, 1 M-cell workbook, warm cache | ≤ 300 MB |

Measurement harnesses follow [PRD 07](./07-quality-and-testing.md)'s automated NFR protocol; regressions beyond 20 % fail CI.

---

## 21. Testing & Validation

### 21.1 Levels
- **L1 unit** — MDW/column-width arithmetic (`px = round(w·MDW)+PP`), prefix sums with hidden columns, anchor→EMU conversion, numFmt parse/format round-trips (≥ 200 table-driven codes), epoch handling incl. the 1900 bug, fit solver page counts, break insertion, header/footer field expansion, merge union-find, style resolution with tint.
- **L2 scene goldens** — serialised `VectorSlide` JSON for: styled table, merged header, wrapped + shrunk + rotated cells, pattern fill, diagonal borders, conditional formatting of all five evaluable kinds, bar/line/scatter chart, chart sheet, anchored image with crop, worksheet shape, sparklines, print page with headers/footers.
- **L3 visual** — DSSIM ≤ 0.05 versus LibreOffice `--convert-to pdf` rendered baselines, per print page, on the pinned corpus.
- **L4 grid-parity** — the repo's existing Playwright harness (`crates/office-terminal-core/tests/golden/external/viewer/`) renders the same workbook with `@silurus/ooxml`'s `XlsxViewer`; compare full-canvas rasters at matched zoom for grid-surface parity. This is the closest available oracle for on-screen appearance.
- **L5 robustness** — fuzz targets (`fuzz/`) for `sheet_xml`, `styles`, `shared_strings`, `drawing`, `chart`, asserting no panic, bounded memory, and typed errors.

### 21.2 Corpus
Pin `sc/qa/unit/data/xlsx` from `LibreOffice/core` (313 files at `master` when this PRD was written) using the existing `generate_manifest.py` / `fetch_fixtures.py` pattern that already pins `sd/qa/unit/data/pptx` (419 decks / 545 slides). Add ~25 hand-authored fixtures covering the cases the corpus lacks: `date1904`, `_xlnm.Print_Titles`, `fitToPage` with manual breaks, header/footer field soup, `fPrintsWithSheet="0"`, `chartEx`, chart sheet, stacked rotation, pattern fills, sparklines, uncalculated formulas.

The external comparison test is cache-gated exactly like `external_libreoffice_renderer_comparison`: with `target/external/` populated it runs on every `cargo test`.

---

## 22. Implementation Deliverables (Definition of Done)

The feature is either complete exactly as specified here, or it does not ship. **No partial modes, no surface without the other, no "phase 2" carve-outs inside the in-scope list.**

| # | Deliverable | Done when |
| :--- | :--- | :--- |
| 1 | Detection | `SourceFormat::Xlsx` + content-type sniffing + every §5.1 rejection path has a typed error and a test |
| 2 | Package layer | workbook/sheets/definedNames/SST/styles/theme/sheet XML parsed; `catch_unwind`-free deterministic parse; all limits enforced |
| 3 | Grid model | MDW profile, column/row prefix sums, hidden/outline, defaults, overrides; geometry tests green |
| 4 | Value layer | all `c@t` kinds; numFmt engine with the full §8.2 grammar; `General`; `###`/scientific fallbacks |
| 5 | Paint layer | fills, borders with shared-edge precedence, alignment, conditional formatting (5 evaluable families) |
| 6 | Text layer | wrap, clip, spill, shrink, rotation, stacked, rich runs, merged-cell layout, auto-fit approximation |
| 7 | Drawings | anchors, pictures (7 formats incl. SVG + metafiles), shapes, background picture, z-order, groups |
| 8 | Charts | all 2-D families mapped to core renderers; cache + range binding; 3-D flatten; chart sheets; `chartEx` placeholder; sparklines |
| 9 | Print engine | print area/titles, pageSetup, fit solver, breaks, order, margins, chrome, headers/footers, centering, clipping |
| 10 | Compiler | `compile_canvas` single entry; skip-set optimisation; tile identity; determinism test (byte-identical repeat compiles) |
| 11 | Contracts | `SheetSource`, `CanvasSpec`, `DeckSource::Xlsx`, `SourceFormat::Xlsx`; `PageSource` **unchanged** |
| 12 | TUI grid | `AppMode::Sheet`, full §14.3 keymap, frozen panes, tabs, stats, find, goto, zoom, sorter kind, help table from the real keymap |
| 13 | TUI print | Reader reuse, presenter console, page sorter, status line byte-identical to `reader/status.rs` format |
| 14 | CLI | all §16.1 commands and §16.2 flags; feature gating with a clean message when compiled out; config keys |
| 15 | Exports | SVG with selectable text, PNG at `--dpi`/`--width`, per-page and per-sheet naming, `dump` in json/text/csv/tsv |
| 16 | Robustness | every §18 bound enforced with a test; fuzz targets in place; no panic on the corpus |
| 17 | Quality gates | `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all clean; no source file ≥ 600 lines; `#![forbid(unsafe_code)]` |
| 18 | Docs | this PRD updated in place; `docs/README.md` index; `ot --help`, `ot info`, and the in-app help table reflect the shipped surface |

---

## 23. Out of Scope (Explicit)

Each entry is deliberate. Where a construct is encountered at runtime it degrades to the tier shown; nothing here is silently swallowed.

### 23.1 Editing and persistence
- **Any mutation, save, or round-trip.** Read-only by [AGENTS.md](../AGENTS.md) and by product definition.
- **View-only column/row resizing** (the JS reference viewer offers it). Consequence: `editAs` has no observable effect; documented in §11.5.
- **Format painter, cell editing, formula entry, recalculation.**

### 23.2 Computation
- **Formula evaluation engine.** Decision **D5**. The file's cached values are authoritative; uncalculated cells are visibly `#UNCALC`. Rationale: a correct Excel-compatible evaluator (volatile functions, iterative calculation, array/spill semantics, date systems, locale-sensitive coercion) is a product in itself and would dwarf this feature. Revisit only with a dedicated PRD.
- **Pivot table re-computation.** Materialised cells render; `refreshOnLoad` is ignored.
- **Data tables (what-if), scenario manager.**

### 23.3 Chart families
- **`chartEx` rendering** (treemap, sunburst, funnel, waterfall, histogram, box-and-whisker, region map). Rationale: separate schema, separate layout engines (hierarchy packing, bins, waterfall bridging), and each deserves its own fidelity spec. Placeholder + warning per chart; the tab and page still exist.
- **3-D chart projection.** Rendered flat.
- **Chart animations, `c:externalData`, pivot charts' dynamic behaviour.**

### 23.4 Objects and interactivity
- **Comments/notes rendering**, including `cellComments` printing modes (blank form only). Available via `dump --comments`.
- **Data validation UI**, input messages, error alerts, dropdowns.
- **Slicers, timelines, form controls, ActiveX, OLE objects, embedded packages.**
- **Object selection, hit-testing, hover tooltips, right-click menus.**
- **Internal hyperlinks (`#Sheet!A1`).** External URLs render through the existing link seam.
- **Grouped/outline collapse interaction** (outline levels are displayed, not toggled).
- **Sheet/column/row protection enforcement**, shared-workbook editing (`.xlsm` legacy shared mode).
- **Audio/video media on sheets, `xl/media` non-image payloads.**

### 23.5 Formats and inputs
- **`.xls` (BIFF8), `.xlsb`, `.ods`, `.fods`, `.csv`/`.tsv`, SYLK, DIF, PRN, HTML/`.xls` mislabelled.** Typed errors with conversion hints. Delimited text is a separate future format (the reference viewer supports it; we do not, so the diagnostic says so).
- **Encrypted/IRM-protected workbooks.** Typed error; no decryption.
- **External workbook references** beyond cached values.
- **VBA/`vbaProject.bin`**, macros, XLM macro sheets.
- **Legacy VML comments** and legacy drawing parts (`legacyDrawing`).

### 23.6 Semantics we intentionally do not chase
- **Pixel-exact auto-fit** for arbitrary fonts (§7.4), and **exact MDW** for fonts whose Excel behaviour depends on EBDT bitmap metrics (§6.1). Both are approximations **with an override**, surfaced in `info`.
- **Locale databases** beyond English month/weekday names and `[$-xxx]` separators; the system locale never influences output (determinism, §13.4).
- **Printer device settings** (`devMode` printer-settings parts), printer-specific pagination, and colour management. Pages derive from paper size and margins only.
- **Excel's `CalcChain` ordering, iterative calculation, and precision-as-displayed** settings.
- **Cell-level accessibility metadata** and alt-text beyond `cNvPr@descr` recorded in `dump`.

---

## 24. References & Standards

- **ECMA-376 / ISO/IEC 29500-1** (SpreadsheetML): §18.3.1.13 column widths, §18.3.1.99 `worksheet`, §18.3.1.63 `pageSetup`, §18.3.1.72 `printOptions`, §18.3.1.16 `colBreaks`, §18.3.1.103 `rowBreaks`, §18.3.1.64 `pageMargins`, §18.3.1.46 `headerFooter`, §18.8.30 `numFmt`, §18.8.45 `dxf`, §18.4.6 `ctCell`, §20.1.2 chart parts.
- **MS-OI29500** (Office implementation details for ISO 29500) — where Excel diverges from ECMA.
- **`pageSetup` attribute semantics** (scale 10–400, overridden by fit-to; paper codes): <https://c-rex.net/samples/ooxml/e1/part4/OOXML_P4_DOCX_pageSetup_topic_ID0EHZ54.html>
- **`printOptions` semantics** (gridLines **and** gridLinesSet): <https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.printoptions>
- **`xdr:twoCellAnchor` / `clientData@fPrintsWithSheet`, `@editAs`**: <https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.spreadsheet.twocellanchor>, <https://openpyxl.readthedocs.io/en/stable/api/openpyxl.drawing.spreadsheet_drawing.html>
- **Column-width / MDW / bitmap-metric findings**: <https://github.com/ClosedXML/ClosedXML/wiki/Cell-Dimensions>
- **`chartEx` families** (treemap, sunburst, funnel, waterfall, histogram): <https://juliadata.org/XLSX.jl/dev/charts/creatingChartEx>
- **Reference browser viewer** (grid-surface parity oracle, MIT): <https://github.com/yukiyokotani/office-open-xml-viewer> — `XlsxViewer` is "one continuous grid" with its own canvas, tab bar and zoom slider; its parser handles `numFmt`, `sharedStrings`, `conditionalFormat`, `mergeCell`, `dataValid`, `drawing`, but no `printArea`/`pageSetup`/breaks, i.e. it has no print surface. Our §12 is therefore **not** ported parity work; it is specified from the ECMA/MS semantics above.
- **Corpus** (pinned): `LibreOffice/core` @ `master`, `sc/qa/unit/data/xlsx` — 313 files at the time of writing; harness follows `crates/office-terminal-core/tests/golden/external/{generate_manifest,fetch_fixtures}.py`.
