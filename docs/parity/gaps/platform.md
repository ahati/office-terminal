# Platform parity — viewer / worker / DOM reference cases vs. kitty-pptx

Scope: the 58 `*.test.ts` specs under `/tmp/oooxml/packages/pptx/src` matching
`scroll-*`, `viewer-*`, `presentation-*`, `slide-*`, `worker-*`, `render-worker-*`,
`text-layer*`, `find*`, `element-selection`, `hyperlink`, plus the 14 Playwright specs under
`/tmp/oooxml/packages/pptx/tests/visual/`. Reference commit `1f5c6e1d`.

Method: for each file, read the spec plus the helper module it pins (`hidden.ts`, `slide-nav.ts`,
`hyperlink.ts`, `element-selection.ts`, `find.ts`, `text-layer.ts`, `part-failures.ts`,
`worker-task-scheduler.ts`, `scroll-viewer-options.ts`), then locate the Rust seam that either
already exists or would carry the same contract.

Verdict vocabulary:

- **EXACT** — a Rust analogue exists today; the port is a direct translation of the assertion.
- **B** (behavioural) — same contract, different shape; asserted through our state/API surface.
- **N/A** — DOM, canvas-realm, worker-wire or scripting-only; no Rust test should exist.
  `(n)` after the case count = how many cases are portable; the rest are N/A for that file.

Host shorthand: `core` = `crates/office-terminal-core`, `tui` = `crates/office-terminal-tui`,
`display` = `crates/office-terminal-display`, `cli` = `crates/office-terminal-cli`.

Totals: 72 files — **EXACT 14**, **BEHAVIOURAL 37**, **N/A 21**. 461 vitest cases + 25 Playwright
test blocks; roughly 150 vitest cases and 12 Playwright blocks are portable.

---

## A. Find / selection / hyperlink / text layer (6 files)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `element-selection.test.ts` | 5 (3) | B | `core/tests/selection_tests.rs` (new) + new `core/src/selection.rs` | reverse paint order returns the last matching element; a point inside the 30°-rotated box but outside the unrotated one hits; `line`/`straightConnector1` use perpendicular distance ≤ tolerance; text is truncated on a grapheme boundary at `max_text_characters` | yes M: new `selection.rs` (~180 lines: inverse frame, flip, line tolerance, bounded text) |
| `find.test.ts` | 3 (3) | B | `core/tests/find_tests.rs` (new) + `core/src/text/find.rs` | a query spanning two runs of one text body yields one match; the same query across a table-cell boundary yields none; matches carry the absolute slide index and `Slide.hidden` slides are excluded under Skip | yes M: `text/find.rs` (~160) + `tui/src/app/find_state.rs` (~120) |
| `find-highlight-layer.test.ts` | 7 (0) | N/A | — | — | no; highlight boxes are HTML `<div>`s with CSS `rotate()` — the geometry half is covered by the find highlight rects in the row above |
| `hyperlink.test.ts` | 5 (5) | B | `core/tests/hyperlink_tests.rs` | no target/action → `None`; `https://` → `ExternalUri`; `ppaction://hlinksldjump` + `../slides/slide3.xml` → `SlideIndex(2)` (part name preferred over verb); bare `ppt/slides/slide2.xml` → `SlideIndex(1)`; `tel:+1…` is external, not dropped | **landed (V8)**: `tel` allowed in `compiler::hyperlinks::classify_external_target`; a non-navigable scheme is classified internal (no link, no `hyperlink_url_rejected`) |
| `text-layer.test.ts` | 10 (0) | N/A | — | — | no; percentage-positioned overlay spans and CSS transform composition have no terminal analogue |
| `text-layer-hyperlink.test.ts` | 3 (2) | EXACT | `display/tests/driver_tests.rs` + `core/tests/hyperlink_tests.rs` | `map_slide_links_to_cells` emits only `ExternalUri` links (an internal `SlideIndex` jump never becomes an OSC-8 cell); a deck with no handler still carries the links in `VectorSlide` but maps none | no |

## B. `presentation-*.test.ts` (17 files)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `presentation-bounded-pipeline.test.ts` | 1 | B | `core/tests/prefetch_pipeline_tests.rs` | `prefetch` assigns center first then N+1, N−1, N+2…; `entry(i)` walks NotLoaded → Rendering → Ready; a repeat `prefetch` does not re-schedule an in-flight slide | no; the "retain compact facts, reload a slide on demand" half is not portable — we parse the deck eagerly |
| `presentation-canvas-realm.test.ts` | 1 | N/A | — | — | no; `OffscreenCanvas` / foreign-realm font ownership is browser-only |
| `presentation-cfb-guard.test.ts` | 3 | B | `core/tests/loader_tests.rs` | CFB magic `D0CF11E0A1B11AE1` + `EncryptedPackage` → `ParseError::PasswordProtected`; `PowerPoint Document` stream → legacy-binary error; any other CFB → not-ooxml | yes S: `ParseError::PasswordProtected` is declared but never produced; add a ~50-line magic sniff |
| `presentation-decrypt.test.ts` | 4 | N/A | — | — | ECMA-376 Agile/Standard decryption (AES + SHA + XML key derivation) is a subsystem we deliberately do not have |
| `presentation-destroy.test.ts` | 15 (5) | B | `core/tests/prefetch_pipeline_tests.rs` + `display/tests/driver_tests.rs` | `shutdown()` twice is a no-op; after it `queue_len()==0`, `in_flight_count()==0`, `Drop` joins without panic; `invalidate(DeckReload)` leaves `entry(i)` NotLoaded | no; the `FontFaceSet` release cases are N/A |
| `presentation-element-context.test.ts` | 5 (3) | B | `core/tests/selection_tests.rs` | `bounds_by_ids` returns hits in request order, skips unknown ids, and never hit-tests when the id is authored | shares the `selection.rs` change; the "master reuses a slide id" case is N/A (master shapes are not composed into `Slide.elements`) |
| `presentation-font-options.test.ts` | 3 | N/A | — | — | no GoogleFonts/font-source subsystem; faces come from the host database (`text/fonts.rs`) |
| `presentation-handle-media-errors.test.ts` | 7 (2) | B | `tui/tests/media_tests.rs` | a missing player or failed spawn yields a toast and leaves the already-placed slide untouched; a media failure never aborts the render loop | no |
| `presentation-handle.test.ts` | 1 | N/A | — | — | API-naming parity (`dispose`→`destroy`); there is no `PresentationHandle` type on the Rust side |
| `presentation.image.test.ts` | 2 (1) | B | `core/tests/loader_tests.rs` | a second `get_raw_bytes(rel_id)` for the same part does not re-open the archive | yes S: `DefaultAssetStore::get_raw_bytes` re-reads the zip every call (~30 lines for a bounded raw-part cache) |
| `presentation.markdown.test.ts` | 1 | EXACT | `cli/tests/e2e_tests.rs` (`run_dump --markdown`) | markdown output has one heading per slide plus note bodies; a deck with no notes still emits every slide heading | no |
| `presentation.mode.test.ts` | 2 | N/A | — | — | no main-vs-worker duality: one native parser, threads only rasterize |
| `presentation-part-failures.test.ts` | 4 (3) | B | `core/tests/part_failure_tests.rs` (new) | a deck whose `slide3.xml` is malformed still loads with N slides; slide 3 compiles to a placeholder and emits `FidelityDegraded{Placeholder}`; the failure list is sorted, deduped and delivered once | yes M: per-slide part-failure capture in the parser adapter + placeholder compile path (~150) |
| `presentation-preflight.test.ts` | 20 (1) | B | `core/tests/parity_fonts_tests.rs` | the union of requested families across per-slide `compile_in_deck` equals the union of a single whole-deck pass, including Han/Hangul/Kana on separate slides | no; the bounded-preflight/structural-JSON ABI has no analogue |
| `presentation-progressive.test.ts` | 8 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | the first `entry()` after `prefetch` is Rendering while `queue_len()>0`, then Ready; `slides.len()` is stable across the sequence | no; worker prefix framing is N/A |
| `presentation-resource-poison.test.ts` | 2 (1) | B | `core/tests/prefetch_pipeline_tests.rs` | a compile failure is latched for that slide (`entry` → Failed) until `clear_failed()`; a successfully cached sibling stays Ready | no; only the per-slide latch ports — we have no document-level poison |
| `presentation-worker-bitmap-ownership.test.ts` | 6 | N/A | — | — | bitmaps are `RgbaSlide` values/`Arc`s; nothing is transferable, closable or double-free-able |

## C. `scroll-*.test.ts` (9 files)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `scroll-media-controller.test.ts` | 1 | N/A | — | — | playback is a detached external process; no mounted-slide media handle exists to retire |
| `scroll-viewer-concurrent-load.test.ts` | 4 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | a task running when `invalidate(DeckReload)` fires must not publish into the Tier-1 cache afterwards; the post-reload render wins | yes S: `execute_task` calls `put_vector` without a cancellation check |
| `scroll-viewer-hidden.test.ts` | 15 (9) | B | `core/tests/navigation_tests.rs` (new) + `tui/tests/navigation_tests.rs` | Skip: hidden 3 → next shown 4; a hidden last slide → previous shown; an all-hidden deck navigates as in Show; `find` never matches hidden; switching to Skip clears the active match and keeps a shown current slide | yes M: `core/src/navigation.rs` (~90) + rewire `PresentationState` nav/search/sorter (~90) |
| `scroll-viewer-internal-nav.test.ts` | 5 (4) | B | `tui/tests/link_tests.rs` | `ppaction://hlinkshowjump?jump=nextslide` from slide 4 → 5; at the last slide the dispatch reports `changed:false`; an unresolvable ref → `LinkDispatch::None`; an external link → `OpenExternal` and no navigation | no |
| `scroll-viewer-progressive.test.ts` | 10 | N/A | — | — | continuous-scroll progressive layout with a comment rail; we render one slide per screen with a synchronous layout |
| `scroll-viewer-reload-orphan.test.ts` | 3 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | `invalidate(DeckReload)` clears both tiers **and** the failure latch; `invalidate(TargetChanged)` clears only rasters | yes S: `invalidate` never calls `workers.clear_failed()`, so a stale `Failed` survives a reload |
| `scroll-viewer-selection-context.test.ts` | 6 | N/A | — | — | browser `contextmenu` plumbing; the hit-test half is covered by `element-selection` |
| `scroll-viewer.test.ts` | 132 (2) | EXACT | `tui/src/app/graphics_tests.rs` | two `sync()` calls with the same slide and rect emit exactly one `stage`+`present`; moving the slot re-`present`s the same image id without re-transmitting | no; DOM skeleton, virtualization, worker-bitmap and comment-card cases are N/A |
| `scroll-viewer-zoom-contract.test.ts` | 16 (11) | B | `tui/tests/zoom_tests.rs` (new) | `set_scale` clamps to [0.1, 4]; the change callback fires only on an actual change; `fit_page` = min(width fit, height fit) via `fit_cell_aspect(Contain)` and `fit_width` = `FillWidth`; a pre-layout `set_scale` is latched and applied once | yes M: `zoom` on `PresentationState`, folded into `RenderTargetSpec` + placement (~80) |

## D. `viewer-*.test.ts` (16 files)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `viewer-composition-boundary.test.ts` | 2 | EXACT | `tui/src/app/graphics_tests.rs` | Fullscreen and PresenterConsole both paint through `GraphicsSyncer::sync`; the driver observes `stage` before `present` for every new image id | no |
| `viewer-concurrent-load.test.ts` | 5 (3) | B | `core/tests/prefetch_pipeline_tests.rs` | as the two scroll rows above, plus: a canceled task leaves no `failed` entry and does not evict another slide's Ready raster | yes (same fix: cancel-before-`put_vector`) |
| `viewer-destroy-during-render.test.ts` | 2 | B | `core/tests/prefetch_pipeline_tests.rs` | after `shutdown()` no error is recorded (`get_failed(i)` is `None`) even for a slide that would fail | no |
| `viewer-destroy.test.ts` | 8 (3) | B | `display/tests/driver_tests.rs` | `release(unknown id)` and a repeated `release_all()` both return Ok; the teardown bytes are exactly `\x1b_Ga=d,d=i,i=N,q=2\x1b\\` and `\x1b_Ga=d,d=a,q=2\x1b\\` | no; canvas reparent / display-style cases are N/A |
| `viewer-find-destroy.test.ts` | 1 | B | `tui/tests/state_tests.rs` | after `set_slides(Vec::new())` no search cursor survives and `selected_slide_index()` is `None` | no (falls out of the find state) |
| `viewer-hidden-skip.test.ts` | 5 (5) | B | `tui/tests/navigation_tests.rs` | `jump_to_slide` to a hidden slide lands on the next shown, else the previous; an all-hidden deck behaves as Show; a link to a hidden slide uses the same rule; find never returns a hidden slide | yes (same as `scroll-viewer-hidden`) |
| `viewer-hidden.test.ts` | 1 | EXACT | `core/tests/navigation_tests.rs` (new) | pure helpers: `next_visible(3)` skips hidden 4; `count_visible` counts shown only; `resolve_visible` maps a hidden target forward then backward and clamps at both ends | yes S (part of the hidden-slide change) |
| `viewer-internal-nav.test.ts` | 5 (4) | B | `tui/tests/link_tests.rs` | as `scroll-viewer-internal-nav`, plus `PreviousSlide` from slide 0 reports `changed:false` and `EndShow` maps to `AppAction::Quit` | no |
| `viewer-layout-error-routing.test.ts` | 4 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | a failing slide is reported through exactly one channel per caller: `render_sync` returns `Err` while `entry()` independently reports Failed, and the CLI prints one line | no |
| `viewer-model-source-options.test.ts` | 2 (1) | B | `core/tests/loader_tests.rs` | loading a `.docx` or legacy `.ppt` byte stream returns a typed `ParseError` (never panics) and the message distinguishes legacy from not-ooxml | yes (same as the CFB guard) |
| `viewer-part-failures.test.ts` | 1 | B | `core/tests/part_failure_tests.rs` | every known failure is delivered exactly once, in slide order, and nothing is delivered after teardown | yes (same as `presentation-part-failures`) |
| `viewer-progressive.test.ts` | 7 (3) | B | `tui/src/app/graphics_tests.rs` | navigating to a slide whose `entry()` is Rendering leaves the previous image placed and returns Ok (no blank flash, no error); it is replaced only once the new bitmap is Ready | no |
| `viewer-reload-orphan.test.ts` | 2 | B | `core/tests/prefetch_pipeline_tests.rs` | as `scroll-viewer-reload-orphan`: a pre-reload compile failure does not survive `invalidate(DeckReload)` | yes (same: clear the failure latch) |
| `viewer-render-error.test.ts` | 3 (2) | B | `cli/tests/e2e_tests.rs` | `export` on a deck whose slide 2 fails exits non-zero and writes exactly one error line naming slide 2 | no |
| `viewer-selection-context.test.ts` | 14 (4) | B | `core/tests/selection_tests.rs` | a NaN/negative tolerance is rejected; a hit resolved for slide A is dropped once the current slide is B; a pending hit is invalidated by a deck reload | yes (same `selection.rs` change) |
| `viewer-zoom-contract.test.ts` | 13 (8) | B | `tui/tests/zoom_tests.rs` | render width = natural × scale (not `width` × scale); scale 1 renders at the natural target; `zoom_in`/`zoom_out` walk one shared ladder; `fit_width` before layout is a no-op | yes (same zoom change) |

## E. `slide-*`, `worker-*`, `render-worker-*` (10 files)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `render-worker-canvas-transfer.test.ts` | 4 | N/A | — | — | transferred `OffscreenCanvas` surfaces; we own the framebuffer |
| `render-worker-init-hang.test.ts` | 9 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | a compile error clears `in_flight` and latches Failed, so the slot never stays Rendering; a second open request while one is in flight is refused rather than queued | yes M: `worker_loop` must `catch_unwind` and clear `in_flight`, otherwise a panicking compile hangs that slide forever |
| `slide-cursor-operation.test.ts` | 3 | N/A | — | — | the Rust↔JS prepared-slide ACK/rollback ABI does not exist; parsing is in-process |
| `slide-nav.test.ts` | 11 (9) | EXACT | `core/tests/hyperlink_tests.rs` | `../slides/slide3.xml` and `ppt/slides/slide3.xml` both → 2; duplicate part names resolve to the first; basename-only `slide3.xml` is tolerated; an unknown part → `None`; relative show jumps clamp at both ends | no |
| `slide-pull-client.test.ts` | 4 | N/A | — | — | byte-credit pull/retry protocol between threads that does not exist here |
| `slide-pull-worker.test.ts` | 21 | N/A | — | — | as above: credit envelopes, transfers and package FIFO |
| `slide-repository.test.ts` | 14 (4) | B | `core/tests/prefetch_pipeline_tests.rs` | two `prefetch` calls for one index compile once (`cache.vector_len()==1`); a latched failure retries only after `clear_failed`; a slide over the cache budget is returned without being retained | yes M: `PipelineCache` never evicts (~90 lines for a byte-budgeted LRU on both tiers) |
| `worker-init-hang.test.ts` | 8 (2) | B | `core/tests/prefetch_pipeline_tests.rs` | as `render-worker-init-hang`, plus a failure recorded for slide 2 leaves slide 1's Ready entry intact | yes M (same; also replace the `.expect()` thread spawn with a propagated error) |
| `worker-renderer-modules.test.ts` | 1 | N/A | — | — | JS module registry (ChartEx) reconstruction over structured clone |
| `worker-task-scheduler.test.ts` | 1 | EXACT | `core/tests/prefetch_pipeline_tests.rs` | `PrefetchTask` pops strictly by descending priority, and `prefetch` assigns center > N+1 > N−1 > N+2 > N−2 for `{ahead: 2, behind: 2}` | no |

## F. Playwright specs (14 files, 25 test blocks)

| reference file | cases | verdict | Rust host | assertion | production change? |
|---|---|---|---|---|---|
| `bevel-ring.spec.ts` | 1 (4 scales × dpr) | EXACT | `core/tests/effects_parity_tests.rs` | for width ∈ {40, 120, 480, 1440} px every apex row of a beveled rect has a continuous bevel-coloured band with no background gap | no |
| `font-source-worker-equivalence.spec.ts` | 2 | N/A | — | — | no font-source subsystem (see `presentation-font-options`) |
| `inline-equation-fallback.spec.ts` | 6 (12) | B | `core/tests/math_typesetting_tests.rs` | a malformed `m:choice`/`m:fallback` branch falls back to the whole `Choice` with a `FidelityDegraded` warning and keeps the surrounding runs, instead of dropping the equation | no (verify the existing fallback covers it) |
| `non-ooxml-rejection.spec.ts` | 1 | EXACT | `core/tests/loader_tests.rs` | a PNG and a ZIP without `[Content_Types].xml` are rejected with `ParseError::ArchiveError`, never a panic | no |
| `part-failure.spec.ts` | 1 | B | `core/tests/part_failure_tests.rs` | the placeholder reads `Slide N could not be displayed` and healthy slides are byte-identical to the same deck without the failure | yes (same as `presentation-part-failures`) |
| `pdf-acceptance.spec.ts` | 1 | N/A | — | — | PDF-derived oracle corpus + PDF rasterizer; our oracle is committed PNGs (`visual.spec.ts`) |
| `responsive-overlay.spec.ts` | 1 | N/A | — | — | CSS overflow of an overlay inside a scaled scroll area |
| `rotated-tile-coverage.spec.ts` | 1 | EXACT | `core/tests/picture_parity_tests.rs` | an `a:tile` fill on a 30°-rotated, h-flipped rect leaves zero uncovered interior pixels | no |
| `scroll-zoom-settle.spec.ts` | 3 | N/A | — | — | continuous-scroll zoom preview/settle and anchor retention |
| `table-image-fill.spec.ts` | 2 (6) | EXACT | `core/tests/table_parity_tests.rs` | a `blipFill` cell emits a `DrawImage` clipped to the cell rect; a `pattFill` cell emits two-colour pattern commands retaining both colours | no |
| `visual.spec.ts` | 1 (9 slides) | EXACT | `core/tests/parity_fidelity_tests.rs` + `core/tests/parity_support/vrt.rs` | **port verified**: `sample-1` (9 slides) at 1280 px, pixelmatch threshold 0.20, fail above 20 %, ratchet 0.5 pt, `UPDATE_REFS` / `UPDATE_SCORES` modes, reference PNGs + `scores.json` committed, slide count asserted | no |
| `warp-strip-quality.spec.ts` | 2 (12) | EXACT | `core/tests/warp_parity_tests.rs` | a 50 %-alpha warped run composites at uniform alpha (strip-seam alpha spread ≤ 2/255); no ink outside the envelope map | no |
| `worker-equivalence.spec.ts` | 2 (1) | EXACT | `core/tests/hyperlink_tests.rs` | the part-name → index map is identical whether built from `Presentation.slides` or a re-parsed deck (main/worker meta equivalence) | no; the pixel-equivalence half is N/A (single renderer) |
| `worker-selection-equivalence.spec.ts` | 1 | N/A | — | — | cross-thread equivalence of selection/find/zoom needs a second rendering realm we do not have |

---

## Production changes forced by these tests

| # | change | size | unblocks |
|---|---|---|---|
| 1 | Hidden-slide policy: new `core/src/navigation.rs` (`HiddenSlideMode`, `next_visible`, `resolve_visible`, `count_visible`) wired into `PresentationState::{set_current_slide,next_slide,prev_slide,first_slide,last_slide,jump_to_slide,next_slide_index}`, initial-slide selection, `SearchState`, sorter and `link_at_cell` | M (~180) | **landed (V1)** |
| 2 | Find over slide text: `core/src/text/find.rs` collecting `VectorCommand::DrawText` runs into search containers (text body vs. table cell), plus highlight rects in EMU; `tui/src/app/find_state.rs` for next/prev/active | M (~280) | `find`, `viewer-find-destroy`, and a real text search instead of today's title/notes-only fuzzy TOC |
| 3 | Element selection: `core/src/selection.rs` reverse-order hit test with inverse rotation/flip, line tolerance and bounded text | M (~180) | `element-selection`, `presentation-element-context`, `viewer-selection-context` |
| 4 | Part-failure tolerance: one malformed `ppt/slides/slideN.xml` must not fail the whole deck — capture per-slide, compile a placeholder, emit `FidelityDegraded{Placeholder}` | M (~150) | `presentation-part-failures`, `viewer-part-failures`, `part-failure.spec.ts` |
| 5 | Pipeline correctness: cancel check before the Tier-1 `put_vector`; `clear_failed()` in `invalidate(DeckReload)`; `catch_unwind` around `execute_task` clearing `in_flight` | S (~50) | `scroll-viewer-concurrent-load`, `viewer-concurrent-load`, `*-reload-orphan`, `worker-init-hang` |
| 6 | Container guard: CFB magic sniff producing `PasswordProtected` / legacy-binary / not-ooxml (the `PasswordProtected` variant exists but is never constructed) | S (~60) | `presentation-cfb-guard`, `viewer-model-source-options`, `non-ooxml-rejection.spec.ts` |
| 7 | Zoom/fit scale on `PresentationState` folded into `RenderTargetSpec` + placement | M (~80) | both zoom contracts |
| 8 | Byte-budgeted LRU eviction for `PipelineCache` (both tiers) | M (~90) | `slide-repository` eviction cases |
| 9 | Raw-part cache for `AssetStore::get_raw_bytes` | S (~30) | `presentation.image` |
| 10 | Hyperlink classification: allow `tel:`; treat a non-navigable scheme as internal instead of dropping | S (~15) | **landed (V8)**: `compiler::hyperlinks` + `compiler::slide_nav`; `core/tests/hyperlink_tests.rs` |

## Genuinely N/A — no Rust test should exist

| reference file | one-line reason |
|---|---|
| `find-highlight-layer.test.ts` | highlight boxes are positioned HTML `<div>`s; CSS `rotate()` composition on the group div |
| `text-layer.test.ts` | transparent overlay spans, `%` positioning and native copy content are DOM-only |
| `presentation-canvas-realm.test.ts` | `OffscreenCanvas` vs. foreign-realm document font ownership |
| `presentation-decrypt.test.ts` | ECMA-376 Agile/Standard decryption subsystem (AES/SHA/XML key derivation) is out of scope |
| `presentation-font-options.test.ts` | no GoogleFonts/font-source option; faces come from the host font database |
| `presentation-handle.test.ts` | API-naming parity test (`dispose`→`destroy`); no `PresentationHandle` type exists |
| `presentation.mode.test.ts` | no main-vs-worker rendering duality; threads only rasterize |
| `presentation-worker-bitmap-ownership.test.ts` | `ImageBitmap` transfer/close accounting; Rust bitmaps are values/`Arc`s |
| `render-worker-canvas-transfer.test.ts` | contextless 1×1 canvas transfer into the worker |
| `scroll-media-controller.test.ts` | media handles on mounted slides; we spawn a detached external player |
| `scroll-viewer-progressive.test.ts` | continuous-scroll progressive layout with a comment rail |
| `scroll-viewer-selection-context.test.ts` | `contextmenu` event plumbing around a DOM selection surface |
| `slide-cursor-operation.test.ts` | Rust↔JS prepared-slide ACK/rollback ABI |
| `slide-pull-client.test.ts` | byte-credit pull/retry protocol |
| `slide-pull-worker.test.ts` | credit envelopes, buffer transfers, package FIFO |
| `worker-renderer-modules.test.ts` | JS module registry (ChartEx) reconstruction over structured clone |
| `font-source-worker-equivalence.spec.ts` | no font-source subsets to compare across realms |
| `pdf-acceptance.spec.ts` | PDF-derived oracle corpus + PDF rasterizer; our oracle is committed PNGs |
| `responsive-overlay.spec.ts` | CSS overflow of an overlay in a scaled scroll area |
| `scroll-zoom-settle.spec.ts` | scroll-anchor retention across a zoom preview settle |
| `worker-selection-equivalence.spec.ts` | cross-thread equivalence of selection/find/zoom needs a second renderer |

Note: `viewer-composition-boundary.test.ts`, `viewer-hidden.test.ts` and
`worker-task-scheduler.test.ts` are counted EXACT even though part of each file is N/A — the
portable contract is a direct translation, not a re-derivation.
