# PRD 14a: PDF Reader UI Designs — Mode Detection, Focusable Page Panes, Side-by-Side Reading

> Status: **design proposal** (normative for implementing PRs once accepted).
> Depends on: [PRD 04](./04-interactive-tui-presenter.md) (TUI state machine, widgets, graphics syncer), [PRD 03a](./03a-kitty-graphics-protocol-driver.md) (driver, placements, ID pools), [PRD 06](./06-component-interface-contracts.md) (seam contracts), [PRD 14](./14-pdf-input-support.md) (PDF substrate: `PageSource`, per-page dimensions, phase plan).
> Scope: **mode detection** (slide deck vs document), and the **document-reading UI**: focusable, zoomable, scrollable pages displayed side by side, each row centered when narrower than the viewport. PDF *slide-deck* presenting reuses the existing PRD 04 UX with **zero layout changes** and is specified here only to the extent of detection (§2).

---

## 1. Problem & Scope

The presenter TUI is strictly **page-indexed**: `PresentationState::current_slide: usize`, whole-slide `compute_slide_rect` letterboxing, one staged bitmap per screen role, and `Up/Down/PgUp/PgDn/j/k` all meaning *next/previous slide* (asserted in `events.rs` and `navigation_tests.rs`). Nothing in the app has a spatial scroll offset. Reading a 200-page portrait PDF in this model — one whole page per keypress, no mid-page position, no zoom target, no side-by-side — is unusable.

This PRD specifies the missing interaction layer:

1. **Mode detection (§2)** — a deterministic classification from page geometry alone that decides whether a PDF opens in the existing **slide** UX or the new **reader**.
2. **Reader mode (§4–§7)** — pages as **focusable panes**: side by side, **rows centered when narrower than the viewport** (§6.1), each independently zoomable and scrollable (with optional spread-wide synchronization).
3. **Fit & zoom correctness (§8)** — `fit-width`, `fit-page`, per-pane zoom ladders on live cell-pixel metrics.
4. **Driver support (§3)** — sub-rect placements (Kitty source-rect `x,y,w,h`) so scrolling/panning re-places an already-transmitted page without re-encoding or flicker.
5. **Slot & prefetch generalization (§9)** — windowed page residency with focus-aware priority.

Overlays (help, find, TOC, link picker), blank screen, teardown discipline, and the presenter console are **reused as-is**; reader mode composes with them exactly like `Fullscreen` does today.

### Prior art (UX reference)

termpdf.py (fit cycling, continuous scroll, two-page spread with cover-page-alone), zathura (focusable panes, per-pane zoom), treader/kitpdf (Rust + Kitty), pdfterm (PDFium, low-latency scroll), MeowPDF (continuous). The keyboard language follows the family convention so muscle memory transfers.

---

## 2. Mode Detection: Slide Deck vs Document

**Home of the heuristic.** [PRD 14](./14-pdf-input-support.md) §7.1 defers to this section. The classifier runs at load time from **page metadata only** (no rendering, no content inspection): for every page it reads the effective dimensions $(w_i, h_i)$ in points — `intersected_crop_box()` folded with `page.rotation()`, i.e. exactly what will be displayed.

### 2.1 Definitions

For page $i$ with effective size $(w_i, h_i)$ and $a_i = w_i / h_i$:

| Term | Definition |
| :--- | :--- |
| **Landscape** | $w_i > 1.02 \cdot h_i$ |
| **Portrait** | $h_i > 1.02 \cdot w_i$ |
| **Near-square** | neither (within 2 %) — counts as an outlier for both |
| **Aspect median** $m$ | median of $a_i$ over **landscape** pages (the dominant orientation candidate) |
| **Aspect inlier** | a page with $\lvert a_i - m \rvert \le 0.02 \cdot m$ |
| **Screen-like band** | $m \ge 1.25$ (5:4 floor; covers 4:3, 3:2, 16:10, 16:9, 21:9 and landscape paper like A4) |

### 2.2 Classification rules (first match wins)

| # | Condition | Mode |
| :--- | :--- | :--- |
| D1 | $n = 0$ | error (`ParseError::Empty`) |
| D2 | landscape pages ≥ 90 % of $n$ **and** aspect inliers ≥ 90 % of $n$ **and** $m \ge 1.25$ | **slides** |
| D3 | everything else (portrait majority, mixed orientation, mixed aspect, near-square, $m < 1.25$) | **document (reader)** |

Rationale:

- **Slide decks** (PowerPoint/Keynote/LibreOffice exports, beamer) are uniform landscape at a screen or paper-landscape ratio; a 10 % outlier allowance tolerates stray appendix/transition pages without demoting the deck.
- **Documents** are portrait-majority by nature (papers, reports, manuals). **If most pages are portrait → reader**, unconditionally (D3) — even when perfectly uniform (a portrait "deck" like a beamer handout reads correctly with fit-width scrolling).
- **Mixed landscape aspects** (uniform-orientation but heterogeneous sizes/ratios) fail the inlier test → reader; the reader's per-page slot boxes handle them natively.
- **Near-square** pages are outliers in both directions; a majority-near-square file (scanned receipts, social-media exports) classifies as a document.

### 2.3 Rotated pages (`/Rotate`), including mid-document

Detection always uses **effective (post-rotation) dimensions** — the orientation the viewer displays, never the raw `MediaBox`. This makes `/Rotate` transparent to classification and yields the correct behavior in both modes:

| Scenario | Outcome |
| :--- | :--- |
| Portrait document, one landscape page in the middle (rotated wide figure/table) | Stays **reader** (D3: portrait majority). The wide page renders **contained** in its slot box (§6.4): preserved aspect, top-aligned, shorter than its neighbors — never stretched, never cropped. Page flow is unaffected. |
| Landscape deck, one portrait page mid-deck (rotated slide) | Stays **slides** (within the 10 % outlier allowance). The existing per-slide aspect fit already letterboxes it inside the unchanged slide chrome — **no special case exists or is needed** ([PRD 04](./04-interactive-tui-presenter.md) design as-is). |
| Whole document rotated to landscape by `/Rotate 90` (e.g. scanned landscape) | Classifies by what is displayed: landscape-uniform → **slides** if it meets D2, else reader. |

A rotated page is never a mode flip by itself; only the effective-orientation **majority** decides.

### 2.4 Page-size reporting (documents)

The reader recognizes common paper formats for status display and `info` (ISO A0–A5, B5, US Letter/Legal/Tabloid; portrait and landscape variants; 0.5 mm tolerance on both dimensions of the *majority* size). Display-only: it never gates classification (a uniform A4-landscape file is a slide deck per D2, and the status line notes "A4 landscape"). 100 % zoom is anchored to physical size: $1\,\text{pt} = 96/72\,\text{px}$ at standard-DPI terminals.

### 2.5 Transparency & override

The decision and its evidence print once at startup (e.g. `mode: slides (97 % landscape, aspect 1.778 ±1 %)` or `mode: document (92 % portrait, A4)`), and `--mode slides|reader` always overrides the heuristic ([PRD 14](./14-pdf-input-support.md) §7). The classifier is a pure function of the page-geometry vector — unit-testable without any engine or file I/O beyond metadata.

### 2.6 Slides follow the current design

For a D2 deck, **nothing in this PRD applies**: fullscreen, presenter console, sorter, TOC, blank, export, all keymaps — exactly [PRD 04](./04-interactive-tui-presenter.md). The only new behavior is the detection line at startup. Reader-specific machinery (§3–§12) is unreachable in slides mode.

---

## 3. Driver Contract Delta (PRD 03a / PRD 06)

### 3.1 Placement gains a source rectangle

```rust
pub struct PixelRect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

pub enum Placement {
    Cells(CellRect),                                   // unchanged (whole image)
    CellsSubrect { rect: CellRect, src: PixelRect },   // NEW: Kitty a=p x,y,w,h + c,r
    ScrollbackBlock,                                   // unchanged
}
```

Kitty mapping: `a=p,i=<id>,p=<pid>,x=<src.x>,y=<src.y>,w=<src.width>,h=<src.height>,c=<rect.cols>,r=<rect.rows>,z=-1,C=1,q=2`. The protocol defines `x,y,w,h` as a **source rectangle in pixels** of the transmitted image; the display area is `c × r` cells. Re-issuing a placement with the same `(image id, placement id)` moves/resizes it **without flicker or retransmission** — this is the scroll **and pan** primitive (vertical scroll shifts `src.y`; horizontal pan shifts `src.x`).

Non-Kitty future drivers emulate `CellsSubrect` with client-side cropping + re-stage (correctness fallback; performance secondary).

**Re-transmission is destructive.** Transmitting image data for an id deletes that image *and every placement of it* until a new placement arrives (kitty §*Transmitting images*), so a pane never re-stages the id it is displaying. Reader ids repeat every `READER_IMAGE_ID_SLOTS` (12) pages, so a page turn onto a colliding id — and any re-render of a page already on screen — stages into a **spare id of the reader pool** (one no other pane owns, stages or displays), presents the new bitmap, and only then releases the outgoing id. The next frame reuses the resident bitmap without re-sending it. A pane whose next page is still rendering **keeps whatever it displays** until the new raster is `Ready` — the same keep-until-Ready rule the slide views follow — because releasing into a blank is the transition flicker when a held key outruns the rasterizer; the Ready path then swaps atomically (present first, release after).

Non-Kitty future drivers emulate the swap by keeping the outgoing image alive until the incoming one is placed (same ordering, client-side).

### 3.2 Placement IDs become explicit

The syncer must update *specific* placements (left pane pans while the right pane's band shifts). The driver emits `p=<placement-id>` for reader slots (kitty placement ids `1..=2` per image, one per pane). `reposition(id, placement)` keeps its current signature; the syncer tracks the pairing.

### 3.3 ID pool partition (disjoint ranges)

| Range | Role |
| :--- | :--- |
| 1–8 | current slide (rotating) |
| 9–16 | presenter preview |
| 17–80 | sorter thumbnails (64) |
| 81–92 | reader pages (`READER_IMAGE_ID_BASE=81`, 12 slots for two spread rows of up to six panes) |

Reader ids moved to their own disjoint range (`81..=92`) so reader releases can never delete slide/preview/thumbnail images; `RESIDENT_IMAGE_IDS` is 92. Reader residency is bounded by a **window**, not document length, and panes are prefetched off-thread (worker-side pre-encoding; the UI thread writes prebuilt bytes).

---

## 4. Reader State Model — Focusable Page Panes

New `AppMode::Reader` (peer of `Fullscreen`/`PresenterConsole`/`Sorter`):

```rust
pub enum ReaderLayout { Single, Spread }
pub enum Slot { Left, Right }            // Single layout: always Left
pub struct PageView { fit: FitMode, zoom: f32, scroll: (u32, u32) }   // per-page overrides

pub struct ReaderState {
    pub anchor: usize,               // left page of the current spread row (or the single page)
    pub layout: ReaderLayout,        // Spread default when viewport ≥ 2·min_pane_cols, else Single
    pub default_fit: FitMode,        // FitWidth (default) | FitPage — for pages w/o override
    pub default_zoom: f32,           // 1.0 — multiplier over fit; ladder 0.1..=4.0 (NavState reuse)
    pub focus: Slot,                 // the focused pane; focus gates scroll/pan/zoom/fit
    pub sync: bool,                  // default true: j/k and zoom apply to BOTH panes
    pub cover_alone: bool,           // Spread: page 1 shown solo
    views: LruCache<usize, PageView>,// per-page view state (bounded, §4.3)
}
```

### 4.1 Focus

- Exactly one visible pane is **focused**. The status bar shows it (`focus:L` / `focus:R`), and nothing is drawn in or around the pages for focus: any line on, under or beside page content read as a rendering artifact (`Borders::BOTTOM` on the pane, then a gutter marker, both removed). No `─` glyph may appear in a reader frame.
- `Tab` / `Shift-Tab` cycles focus between visible panes. **Focusing a pane turns `sync` off** — subsequent `j/k`, `h/l`, zoom and fit operations act on the focused pane only. `y` re-enables spread-wide synchronization.
- On page turn, focus **stays on the same slot side** (reading a book, you keep your eye on the same side); the focused page's `PageView` is restored from the LRU (fresh pages start at top, default fit).

### 4.2 Scroll, pan, zoom — all pane-local, sync-optional

| Operation | Acts on |
| :--- | :--- |
| Vertical scroll (`j/k/d/u`, wheel) | focused pane; both panes when `sync` (each clamped independently) |
| Horizontal pan (`h/l`) | focused pane — only meaningful when it overflows (§6.3); clamped to content |
| Zoom (`+`/`−`) / fit cycle (`f`) / zoom reset (`=`) | focused pane; both when `sync` |
| Zoom-all (`Z`) | apply the focused pane's zoom to every page (bulk override) |

Zoom and fit changes invalidate that page's raster at the new scale class and re-stage (existing `ScaleChanged` path); scroll/pan **never** re-render — they re-place only (§3.1).

### 4.3 Per-page view state

`PageView` overrides live in a bounded LRU (64 pages, oldest-first eviction — the `PipelineCache` discipline). Eviction loses only zoom/position memory of distant pages, never correctness. `Z` writes the current view as the new `default_*` for unvisited pages.

---

## 5. Design Pillars

1. **Flicker-free movement is a protocol feature, not a re-render.** A page bitmap is transmitted once per (page, scale class); scrolling and panning re-issue placements with shifted source rects. Zero PNG encoding on the movement hot path.

**Page rasters are sized to the terminal.** The raster target snaps the pane's zoomed device-pixel width × 1.5 up a coarse class ladder (1280 / 1920 / 2560 / 3840 px; `ReaderState::READER_TARGET_CLASSES`), detected from the terminal's pixel geometry via `TIOCGWINSZ` (the same probe the layout uses). A 600-px pane renders at 1280 px — ~4× faster rasterization and ~4× less wire payload than the old fixed 2560-px target — which is what let boundary transmits land mid-glide and blank the entering pane. Coarse classes keep resize jitter from re-rendering, zoom steps up the ladder, and the 1280-px floor keeps text above the rasterizer's 6-px font floor.

**The anchor flip is lag-compensated — in both directions, with per-row strides.** The anchor wraps when the scroll *target* crosses the row stride, and the wrap shifts the smoothed display by the same stride — so under continuous scrolling the display is still negative under the new anchor (it lags by the remaining glide distance). The renderer walks back, adding the stride of each row it steps into, while that holds: the frame is geometrically identical to the pre-wrap frame, so the flip is invisible. Retreats are the mirror image: a scroll-up wrap leaves the display *above* the retreated row's range, and the renderer walks forward, consuming the stride of each row it leaves. Because rows of a mixed-orientation document differ in height, every re-base — the advance wrap, the retreat wrap, and both render walks (including the deep-gutter stride) — consumes the stride of the row it steps into or out of, never a blanket stride. Without these the layout clamped the new row to the viewport top and the whole page jumped by the lag at every boundary — the transition flicker under a held key.
2. **No shrinking, and no one-sided margins.** The reader **never shrinks pages below fit to force both spread pages on screen** — when a row overflows, the left page anchors at column 0 (always fully visible at fit) and the right page is clipped at the viewport edge, with panning to reveal it. When a row *fits*, it is centered (§6.1) so the slack splits evenly instead of piling up on the right.
3. **Pixel-exact fit.** Pages render at exactly the terminal's pixel metrics for the chosen fit; a 1:1 source-to-screen mapping means scrolling never resamples.
4. **Focus over modes.** One pane model, one keymap; focus + the `sync` flag compose into book behavior (synced spread) or diff-viewer behavior (independent panes) without separate modes.
5. **Progressive disclosure under slow renders.** Panes paint placeholders (aspect-correct silhouette + page number) the instant geometry is known; bitmaps arrive when the pipeline delivers (`NotLoaded → Rendering → Ready`).
6. **Nothing new below `RgbaSlide`.** All changes sit in `kitty-pptx-display` (placement), `kitty-pptx-tui` (state/layout/keys), and pipeline *prefetch policy* — render backends untouched.

---

## 6. Layout Model — Side-by-Side, Centered Rows

### 6.1 Pane geometry (Spread)

```
viewport_w_px, viewport_h_px   from the live cell probe (§8)
gutter_cols  = 2
box_w_px     = fit width of the DOMINANT page at the current zoom   // the column box
grid_w_px    = N · box_w_px + (N − 1) · gutter                  // the column grid
grid.x_px    = max(0, (viewport_w_px − grid_w_px) / 2)            // CENTERED anchor
pane_i.x_px  = grid.x_px + i·(box_w_px + gutter) + (box_w_px − pane_i.w) / 2
```

- The column grid is **stable per document**: boxes are sized by the
  dominant (majority) page, so every row — and the row peeking in from
  below during a continuous scroll — sits at identical x positions. Pages
  glide straight up; nothing shuffles sideways at an anchor flip.
- Each pane **centers in its box**; a pane never exceeds its box at fit
  (FitPage containment, §6.4). Uniform documents reduce to the classic
  centered row: box = the page's own fit width, so panes sit edge to edge
  with one gutter between them.
- Rows whose panes overflow their boxes (FitWidth zoomed past the slot,
  per-page zoom with `sync` off) keep the flush-left content walk and its
  horizontal pan window (`h/l`); a walk narrower than the viewport still
  centers as a unit.
- The **right page may or may not fit fully** — by policy (§5, pillar 2). When `right.x + right.w > viewport_w`, the right pane is clipped at the viewport edge; `h/l` on the focused right pane pans within its content (the left pane never moves).
- Row height = `max(left.h, right.h)`; a shorter pane is **top-aligned** within the row (reading flow), leaving blank space below — never stretched.

### 6.2 Pane geometry (Single)

Same machinery, right slot absent. A page narrower than the viewport (fit-page, or fit-width after zoom-out) is **centered** horizontally — the row-centering rule of §6.1 with a single pane; a zoomed page wider than the viewport is **left-anchored** at pan origin.

### 6.3 Overflow, zoom, and pan

At fit-width, panes don't overflow horizontally. Zoom multiplies the render size from the fit basis:

```
render_w_px(page) = slot_w_px · zoom(page)        // Spread; Single: viewport basis
```

**The column plan re-evaluates on every zoom step.** The renderer recomputes how many panes fit per row from the document's *dominant* page at the current zoom each frame (same formula as §6.1, `page_w = viewport_h · dom_ar · zoom`, clamped `1..=6`): zooming in reflows to fewer columns — a 2-up row drops to 1-up as soon as the zoomed pair would overflow — and zooming out packs more pages per row. The dominant basis keeps the plan stable across mixed-orientation documents (§6.4): a landscape page never re-plans the row just because it happens to lead. When a reflow would push the focused page out of the narrower row, the focused page becomes the new row lead (page-turn snap), so the zoom always follows the page under the cursor. Zooming out only adds columns, so it never moves the anchor.

**The zoom-out floor is the page sorter.** The reader's flow model renders two rows against a 12-image pool, so a deep zoom-out cannot add rows — it only spreads the same pages thinner, detaching the capped column grid from the edges. Instead of degrading, `−` stops zooming the moment the next step would put the pane within **twice sorter-card scale** (or leave the two-row window short of the viewport — the backstop on tall terminals; both computed from the dominant page and the sorter's own live card height) and **hands the screen to the sorter grid**, selected on the page being read. An **applied** sorter selection (`Enter`) lands the reader back at reading size: a zoomed-out overview resets to fit, a zoomed-in reading view is preserved; the grid's cards are torn down only once the reader's panes actually present (present-then-release, like the slide path), so a slow re-render after a jump never blanks the screen. `Esc`/`Tab` return unapplied; `Tab`/`t`/`s` remain manual overrides at any zoom.

- `zoom > 1`: the focused page re-renders at the higher scale class (per [PRD 14](./14-pdf-input-support.md) §3.2 hi-res recompile), the row reflows to fewer columns as above, and the **viewport shows a window into the larger bitmap** — vertical scroll and horizontal pan move the window (`src.x`, `src.y` in §3.1).
- `zoom < 1`: the page shrinks inside its slot (centered vertically-top, horizontally-center); the spread may then fit both pages with room — fine.
- Pan/scroll clamp to content bounds; clamped moves are no-ops (no bounce).

### 6.4 Per-page orientation and size (mixed geometry)

Rows pair **consecutive physical page indices** on the §6.1 column grid — the grid is derived from the document's *dominant* (majority) page, never re-planned per row:

- Portrait page in portrait flow: fills its column box — the standard case.
- **Landscape page in a portrait flow** (the mid-document rotated figure, §2.3): **contained in its column box** (aspect preserved, width-limited, hence shorter), top-aligned, beside the following page — it never claims the whole row just because it leads, and it lands at exactly the columns the portrait pages use. Never cropped, never rotated again — `/Rotate` is already folded in.
- Mixed sizes (A4 + A3 appendix): each pane sizes independently within its box; a shorter pane leaves blank space below (top-aligned, never stretched).
- The row peeking in from below during a continuous scroll is laid out with the same grid, so it is exactly the row the anchor flips to — pages never vanish or jump at an orientation change.
- Spread pairing: `(1)` alone if `cover_alone`, then `(2,3)`, `(4,5)`, … — book order, left = even display position, right = odd.

### 6.5 Continuous vertical flow

The spread *row sequence* forms the virtual column (rows, not pages, are the scroll substrate):

```
row_h(r)      = max over panes of pane height at current zoom
column_h      = Σ row_h(r) + gaps
scroll_px     = absolute offset into the column; j/k move it; page turns jump to row tops
```

`j`/`k` at a pane boundary flow into the next/previous row naturally (the window slides across the column); `n`/`p`/`Space`/`PgDn`/`PgUp` snap to row starts (page turns); per-pane `scroll.y` offsets (§4.3) apply within a row when a pane is taller than the viewport.

### 6.6 Horizontal flow rest insets

The Horizontal slide flow tiles pages edge to edge, so a mid-stream anchor
always fills the viewport — no inset. Only the two document-rest anchors get
one, so a page never stares at a wall of empty cells on one side:

```
center_px   = max(0, (viewport_w_px − anchor_page_w) / 2)
start (anchor 0):    inset = center_px · (1 − scroll/page_w)     // unwinds to 0 at the first flip
end   (anchor last): inset = (center_px + page_w) · (scroll/page_w)   // winds up to center at the end clamp
```

- A **negative anchor scroll is a right shift, not a crop**: while an inset
  is active the pane moves right instead of its left edge being cut off
  (`src.x` stays 0).
- Both insets move **linearly with the slide**, so the frame in which the
  anchor flips is geometrically identical to the flush-left stream — no
  boundary jump, and sliding back re-arms the rest symmetrically.
- `G`/`End` land on the **end clamp** (`scroll = page_w − 1`), not the last
  page's left edge, so the final page rests centered exactly like the first.

---

## 7. Cell Metrics & Fit Correctness (prerequisite fixes)

1. **Wire the probe.** `kitty-pptx-display` already knows how to measure cell pixels (`CSI 16 t` / `TIOCGWINSZ`); the TUI currently hardcodes `DEFAULT_CELL_ASPECT_RATIO = 0.5`. Reader mode requires real `cell_w_px × cell_h_px`, refreshed on resize. (Slide modes benefit equally; wiring it is a strict improvement.)
2. **Fix the `fit_width` unit bug.** Today `fit_width`/`fit_page` compare EMU-derived pixels against *cell counts*. With live cell metrics, fit-width becomes exact (§6.1) and the currently-unbound `fit_*` methods get the `f` key.
3. **Render scale classes.** Pages render at `ceil(fit_w_px)` wide snapped to {1×, 1.5×, 2×} classes of the measured grid so resize reuses cached bitmaps across ±1-column jitter; zoom beyond a class boundary triggers the hi-res recompile ([PRD 14](./14-pdf-input-support.md) §3.2), capped at 8,192 px/axis.

---

## 8. Graphics Syncer & Pipeline Windowing

### 8.1 Slot generalization

`GraphicsSyncer`'s hardwired `current_staged/current_placed` + preview pair becomes a **role-keyed slot array** (`BTreeMap<ReaderSlot, SlotState>` — the `thumbs: BTreeMap<usize, ThumbSlot>` pattern promoted to the general mechanism). Slots: `ReaderPage(anchor·2 + side)` × ≤4 visible/adjacent, `Prefetch(usize)` × ≤2 ahead, plus the existing presenter/sorter roles untouched. Diffing semantics unchanged: stage on bitmap identity change, re-place on rect/`src` change, skip on epoch match.

### 8.2 Focus-aware prefetch window

`PrefetchPlan` = rows visible ± 1 row, priority: **focused pane first**, then the visible partner, then ahead-of-anchor rows (reading direction), then behind (backtrack tolerance). Eviction of out-of-window ids follows the existing thumbnail discipline (`release` on leave; new stages per frame capped — `MAX_THUMBNAIL_STAGES_PER_FRAME` generalized to `MAX_STAGES_PER_FRAME`). Pipeline `entry_at(page, target)` already accepts per-page targets; no trait change beyond the window shape.

### 8.3 Placeholder rendering

Panes in `Rendering`/`NotLoaded` draw the existing placeholder vocabulary (PRD 02d): aspect-correct bordered rect + page number — identical to the loading/`Failed` card language, so the UI stays coherent.

---

## 9. Wireframes

### 9.1 Slides mode — current design, unchanged (D2 decks)

```
┌──────────────────────────────────────────────────┐
│                                                  │
│          ┌──────────────────────────┐            │   16:9 slide, letterboxed,
│          │                          │            │   centered — exactly PRD 04
│          │   Slide 12 of 30         │            │   fullscreen; presenter
│          │                          │            │   console / sorter / blank
│          └──────────────────────────┘            │   all available as-is
│                                                  │
│ deck.pdf · slide 12/30 · ?:help · q:quit                  │
```

### 9.2 Reader, Spread, fit-width — both pages fit

```
┌────────────────────────┐  ┌───────────────────────┐
│ █  3. Scrolling        █  │ █  3.1  Source rects  █ │
│ █  The virtual column  █  │ █  A placement may    █ │
│ █  composes pages …    █  │ █  crop its source …  █ │
│ █                      █  │ █                     █ │
│ █                      █  │ █                     █ │
└────────────────────────┘  └───────────────────────┘
 report.pdf · spread 6–7/240 · 12% · fit:width 100% · focus:L · sync:on · ?:help · q:quit
```

### 9.3 Reader, Spread, zoomed — right page clipped (overflow pan policy)

```
┌───────────────────────────────┐  ┌───────────────│
│ █  3. Scrolling               █  │ █  3.1 Source ││ ← right page clipped
│ █  The virtual column         █  │ █  A placement││   at viewport edge;
│ █  composes pages into rows   █  │ █  may crop…  ││   Tab → focus:R,
│ █  …                          █  │ █             ││   h/l pans it
│ █                             █  │ █             ││
└───────────────────────────────┘  └───────────────│
 report.pdf · spread 6–7/240 · fit:width 160% · focus:L · sync:off · ?:help · q:quit
 (left page: fully visible at zoom; right page: partial — pan with h/l)
```

### 9.4 Reader, Spread — landscape figure inside a portrait flow

```
┌───────────────────────┐  ┌────────────────────────┐
│ █  Table 4 spans      █  │ ┌────────────────────┐ █ │
│ █  two pages; the     █  │ │ wide figure,       │ █ │ ← p.35 landscape,
│ █  right page holds   █  │ │ contained & top-   │ █ │   contained in slot,
│ █  the table          █  │ │ aligned (§6.4)     │ █ │   never cropped
│ █                     █  │ └────────────────────┘ █ │
│ █                     █  │                        █ │
└───────────────────────┘  └────────────────────────┘
 report.pdf · spread 34–35/240 · fit:width 100% · focus:L · sync:on · ?:help · q:quit
```

### 9.5 Status bar grammar (one row, existing chrome slot)

```
<file> · [spread <l>–<r>|p.<n>] /<total> · <scroll %> · fit:<mode> <zoom%> · focus:<L|R> · sync:<on|off> [· cover:<on>] · ?:help · q:quit

Everything on the bar is a segment joined by the one `·` separator
(the optional `cover:on` and the key hints join like any other — the
joiner owns the single policy; hints are the keymap's §10 `key:action`
pairs).
```

Find/TOC/help overlays open **over** the reader exactly as over fullscreen (centered modal, dimmed base); find hits highlight on the ratatui layer with page+jump navigation, using the `SearchCorpus` boxes ([PRD 14](./14-pdf-input-support.md) §5.4). The TOC modal numbers outline nodes by their tree position (`1`, `1.1`, `1.1.1`, …) and renders `2.1 Sub Chapter [64]` with the page column right-aligned to the document's widest page number.

---

## 10. Keymap (mode-local; conflicts resolved)

| Key | Fullscreen (today) | Reader (new) |
| :--- | :--- | :--- |
| `j` / `↓` | next slide | **continuous column scroll** (1/8 row stride; next row enters smoothly) |
| `k` / `↑` | prev slide | **scroll −1/8 row stride** |
| `d` / `u` | — | scroll ±½ viewport |
| `←` / `→` | prev/next slide | **slide row horizontally** (when zoomed/overflow) / **prev, next row** (at fit) |
| `h` / `l` | — | **pan horizontally** (Spread/Single) / **slide continuous** (Horizontal mode) |
| `H` | — | **toggle Horizontal slide flow** ↔ Spread |
| `Tab` / `Shift-Tab` / `w` / `\` | — | **cycle pane focus** |
| `y` | — | re-enable spread-wide `sync` (scroll + zoom) |
| `PgDn`/`PgUp`, `Space`, `Enter`, `n`, `p` | slide step | **row/page turn** (focus keeps its side, h-scroll preserved) |
| `g` / `G` | first/last slide | first / last page — in Horizontal mode the **centered rest positions** (§6.6: start rest / end clamp) |
| `N` + `Enter` | jump slide | jump to page `N` (its row, top) |
| `f` | — | cycle fit of focused pane: width → page |
| `+` / `−` | zoom ladder | zoom ladder, **focused pane** (both if `sync`) — every step **reflows the column plan** (§6.3); the last `−` **hands off to the sorter** at the floor |
| `=` | — | reset focused pane to fit + top (resets h-scroll) |
| `Z` | — | apply focused zoom to all pages (new default) |
| `2` | — | toggle Spread ↔ Single |
| `!` | — | toggle cover-alone (Spread) |
| `b`, `t`, `/`, Ctrl-F, `o`, `?`, `q`/`Esc` | unchanged | unchanged |
| wheel | — (only Left-click hit-test today) | scroll focused pane; wheel+Ctrl → zoom |

`handle_key_event` dispatches to `handle_reader_key` while `base_mode() == Reader`; existing per-mode handlers and their tests are untouched. New assertions live in a `reader_events_tests.rs`.

---

## 11. NFRs & Testing

| Metric | Target | Method |
| :--- | :--- | :--- |
| Scroll/pan step latency | ≤ 16 ms p95, zero PNG encode | syncer counter asserts no `stage()` on pure movement; timing harness |
| Row turn (within window) | ≤ 16 ms p95 (placement-only) | as above |
| Page enter (uncached, warm pipeline) | ≤ 120 ms p95 | PRD 00 harness reused on 240-page fixture |
| Focus switch / sync toggle | ≤ 1 frame, zero stages | golden syncer trace |
| Mode detection | deterministic, metadata-only | L1 table tests: §2 matrix incl. portrait-majority, mid-rotated pages both ways, A4-landscape uniform, near-square, single page, 10 % outliers, mixed sizes |
| Resize | ≤ existing resize path | comparative harness |

Test levels (PRD 07): L1 unit — classifier table (above), pane math (slot boxes, clamping, cover-alone pairing, LRU restore, zoom class snapping); L2 layout goldens — placement plans as JSON traces (cell rects + `src` rects per scroll/pan/focus scenario, incl. right-pane clipping); L2a scene goldens — PDF corpus ([PRD 14](./14-pdf-input-support.md) P5); L3 protocol goldens — `CellsSubrect` escapes incl. re-place without re-transmit; L4 interaction — the keymap table, focus/sync semantics, coalescing; L5 live-terminal matrix — kitty/ghostty/wezterm movement smoothness + `d=a` teardown cleanliness.

---

## 12. Milestones

| ID | Scope | Exit criteria |
| :--- | :--- | :--- |
| **R0** | Driver: `Placement::CellsSubrect`, placement-id emission, probe wiring, fit unit fix | L3 goldens for subrect place/re-place; `fit:width` pixel-exact on live kitty |
| **R1** | Mode detection (§2) + Single-pane continuous reader (state, column, windowed slots, scroll, status bar) | classifier L1 suite green; NFR table on 240-page portrait fixture; D2 deck opens in **unchanged** slide UX |
| **R2** | Spread: centered rows (§6.1) + left-anchored overflow, right-clip policy, focus + `sync`, per-page zoom/pan, cover-alone | §9.3/§9.4 goldens; focus/sync L4 suite; uneven-size fixture |
| **R3** | Find/TOC/link-picker composition over reader; page-label display | L4 suite |
| **R4** | Wheel scroll + Ctrl-wheel zoom; scale-class resize reuse | L5 live matrix |

R0 is a prerequisite of [PRD 14](./14-pdf-input-support.md) P2 (the reader ships as P2 = R1+R2; R3/R4 trail P4).

---

## 13. Open Questions

1. Smooth (pixel-wise, timer-driven) scroll vs discrete-step as default? (Discrete ships first; optional `--smooth` later.)
2. When the focused pane hits its bottom and `j` continues, should the view flow into the next row automatically (continuous feel) or stop (explicit page turns only)? Proposal: flow when `sync` is on, stop when off.
3. Should `Z` (zoom-all) also reset per-page `PageView` overrides, or merge with them? Proposal: reset (it declares a new baseline).
4. Mouse drag-to-pan policy over the Kitty placement layer (threshold, pointer capture).
5. Page-label vs physical-index display priority in the status bar (ties to [PRD 14](./14-pdf-input-support.md) §12.4).
