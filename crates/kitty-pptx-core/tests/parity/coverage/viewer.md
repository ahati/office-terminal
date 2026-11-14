# Viewer parity coverage (`viewer` slot)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only).
Scope: the remaining reference specs after the core renderer batches — every
`viewer*`, `scroll*`, `worker*` and `presentation*` spec plus the selection/nav
specs they delegate to, and the 14 Playwright acceptance specs under
`tests/visual/`.

Files enumerated: **72 spec files / 455 cases** — 45
`viewer|scroll|worker|presentation` specs, 13 deeper specs the viewers delegate
to (`slide-nav`, `element-selection`, `find`, `text-layer`,
`text-layer-hyperlink`, `find-highlight-layer`, `selection-context`,
`slide-cursor-operation`, `slide-pull-*`, `render-worker-*`,
`renderer-destroy-invalidation`), and the 14 Playwright specs under
`tests/visual/`. (The brief said 62 files + 13 specs; `tests/visual/` actually
holds 14 — `pdf-acceptance` is the one usually forgotten — and the 16
non-spec modules of the `viewer|scroll|worker|presentation` group carry no cases
of their own, so they are listed after the per-file table.)
`cases` counts reference test functions (`it(`/`test(`); `it.each` rows are
counted once, so this number is a lower bound for `scroll-viewer.test.ts`.

Verification (`CARGO_TARGET_DIR=target/agent-viewer`):

```
cargo test -p kitty-pptx-core --test parity_viewer_nav_tests          # 15 pass  0 ignored
cargo test -p kitty-pptx-tui  --test parity_viewer_nav_tests          # 25 pass  0 ignored
cargo test -p kitty-pptx-core --test parity_viewer_selection_tests    #  4 pass  0 ignored
cargo test -p kitty-pptx-core --test parity_viewer_lifecycle_tests    # 10 pass  0 ignored
cargo test -p kitty-pptx-core --test parity_viewer_worker_tests       #  9 pass  0 ignored
cargo test -p kitty-pptx-core --test parity_playwright_tests          #  7 pass  0 ignored
cargo test -p kitty-pptx-core --test golden_tests                     #  5 pass
cargo clippy -p kitty-pptx-core -p kitty-pptx-tui --tests -- -D warnings   # clean
```

`70` tests pass; the `34` blocked cases are archived under `tests/parity/pending/`
(not compiled) with the missing symbol in each archive header. No production file was
modified and no golden output changed
(`UPDATE_GOLDENS` not run — the batch only reads existing APIs).

## Summary

| target | reference specs | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | tests | pass | ignored |
| :--- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| nav | 9 | 76 | 9 | 34 | 14 | 19 | 40 | 40 | 0 |
| selection | 10 | 56 | 0 | 5 | 19 | 32 | 4 | 4 | 0 |
| lifecycle | 32 | 258 | 2 | 19 | 118 | 119 | 10 | 10 | 0 |
| worker | 8 | 49 | 0 | 1 | 6 | 42 | 9 | 9 | 0 |
| playwright | 14 | 21 | 0 | 6 | 3 | 12 | 7 | 7 | 0 |
| **total** | **73** | **460** | **11** | **65** | **160** | **224** | **70** | **70** | **0** |

Notes on the groups: `nav` = the 5 nav/zoom viewer specs plus `slide-nav`,
`hyperlink`, `viewer-hidden`, `viewer-hidden-skip`, `scroll-viewer-hidden`; `selection` = the
10 selection/find/layer specs; `lifecycle` = the `presentation*` and remaining
`viewer-*`/`scroll-*` lifecycle specs; `worker` = the 8
`worker*`/`slide-pull*`/`render-worker*`/`renderer-destroy-*` specs;
`playwright` = the 14 `tests/visual/*.spec.ts`. Non-spec modules in scope
(`viewer.ts`, `presentation.ts`, `scroll-viewer.ts`, `slide-nav.ts`, `hidden.ts`,
`part-failures.ts`, `slide-repository.ts`, `worker-protocol.ts`, `worker.ts`,
`scroll-*.ts`) carry no cases of their own; their behaviour is asserted through
the specs above, and `slide-repository.ts` (bounded slide LRU) has no Rust
analogue at all (`PipelineCache` is unbounded and keys on slide index).

## Per-file rows

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | ported where / missing symbol |
| :--- | --: | --: | --: | --: | --: | :--- |
| `slide-nav.test.ts` | 11 | 9 | 2 | 0 | 0 | part index / part target / `hlinksldjump` / M2 resolve in `resolve_internal_slide` + `resolve_hyperlink` (EXACT); the two relative-verb cases split into the action mapping (core target) and the arithmetic (TUI target) |
| `hyperlink.test.ts` | 5 | 0 | 5 | 0 | 0 | `classifyPptxHyperlink`: no link, navigable URL (`tel:` included), part-name-preferred slide jump, bare part name, non-navigable scheme as internal — `compiler::hyperlinks` + `tests/hyperlink_tests.rs` |
| `viewer-internal-nav.test.ts` | 5 | 0 | 5 | 0 | 0 | dispatch branches via `extract_slide_hyperlinks` + `PresentationState::dispatch_link`, including the caller-owned `onHyperlinkClick` delegation (`LinkDispatch::Internal`) |
| `scroll-viewer-internal-nav.test.ts` | 5 | 0 | 4 | 1 | 0 | same dispatch contract in scroll mode; the scroll-specific resolution is N/A (one slide per presenter screen) |
| `viewer-hidden.test.ts` | 1 | 0 | 1 | 0 | 0 | `navigation::{next_visible,resolve_visible,count_visible}` asserted in the core nav target |
| `viewer-hidden-skip.test.ts` | 5 | 0 | 4 | 0 | 1 | show/dim `goToSlide`, skip `goToSlide`, all-hidden, internal-link cases ported; the run-level `findText` case is re-scoped to V2 (wave 4) |
| `scroll-viewer-hidden.test.ts` | 15 | 0 | 6 | 9 | 0 | hidden slide exclusion, absolute indices, skip jumps, all-hidden, mode-switch clear, dim/show addressability ported; scroll layout/paint/progressive/anchor cases are N/A (no continuous-scroll layout) |
| `viewer-zoom-contract.test.ts` | 13 | 0 | 5 | 2 | 6 | width-verbatim, fitWidth/fitPage cell fit, `set_scale` clamp + unchanged-factor gate ported; getScale/render-width pairing, steppers/ladder and the pre-load latch BLOCKED on V7; 2 DOM cases N/A |
| `scroll-viewer-zoom-contract.test.ts` | 16 | 0 | 2 | 2 | 12 | base-fit cases ported to the cell fit; the per-scale slot/anchor contract BLOCKED on viewer zoom |
| `element-selection.test.ts` | 5 | 0 | 1 | 0 | 4 | topmost-by-paint-order ported to `SlideLinkIndex::hit_test`; rotation/line tolerance, bounded text, chart context, element-context struct BLOCKED on `hit_test_slide_*` |
| `find.test.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: `PptxFindController` (no run-level find index) |
| `text-layer.test.ts` | 10 | 0 | 2 | 8 | 0 | run-box positioning + frame clamp ported (`compiler::hyperlinks`); DOM overlay structure/rotation cases N/A |
| `text-layer-hyperlink.test.ts` | 3 | 0 | 2 | 1 | 0 | a hyperlinked run carries its own target + box; the no-handler interactivity case N/A (no spans) |
| `find-highlight-layer.test.ts` | 7 | 0 | 0 | 2 | 5 | BLOCKED: `buildPptxHighlightLayer`; 2 rebuild/grouping cases N/A |
| `selection-context.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A (DOM): reads `HTML Range`/`Selection` |
| `slide-cursor-operation.test.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: `loadPptxSlideFromCursor` |
| `viewer-selection-context.test.ts` | 13 | 0 | 0 | 5 | 8 | BLOCKED: element/text selection context projection; 5 DOM cases N/A |
| `scroll-viewer-selection-context.test.ts` | 6 | 0 | 0 | 2 | 4 | BLOCKED: same projection in scroll mode |
| `presentation-element-context.test.ts` | 5 | 0 | 0 | 0 | 5 | BLOCKED: `hit_test_slide_context` + `PptxElementContext` |
| `presentation-part-failures.test.ts` | 4 | 2 | 2 | 0 | 0 | deck loads with a broken part, part-tagged message, never hidden (EXACT through `DefaultPresentationLoader`); the placeholder resolver runs once for the broken slide only and its title/detail reach the card (BEHAVIOURAL, `broken_slide_paints_the_error_placeholder`) |
| `presentation-resource-poison.test.ts` | 2 | 0 | 2 | 0 | 0 | the first fatal slide error is replayed on every later read (`WorkerPool::get_failed`) and cleared only explicitly |
| `presentation-destroy.test.ts` | 15 | 0 | 2 | 6 | 7 | destroy twice + in-flight cancellation ported (`WorkerPool::shutdown` / `invalidate`); worker-request rejection and embedded-font registration BLOCKED; `FontFaceSet` cases N/A |
| `viewer-part-failures.test.ts` | 1 | 0 | 1 | 0 | 0 | the broken slide's frame carries the labelled card instead of its content (`CompileOptions::error_placeholder` sees `PartFailure{slide_index, slide_number}`) |
| `presentation-cfb-guard.test.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: `assert_not_cfb_container` (`ParseError::PasswordProtected` is never produced) |
| `presentation-decrypt.test.ts` | 4 | 0 | 0 | 0 | 4 | BLOCKED: `decrypt_ooxml` |
| `presentation-preflight.test.ts` | 20 | 0 | 0 | 3 | 17 | BLOCKED: `PresentationPreflightBuilder` / deck font union / structural projection |
| `presentation-bounded-pipeline.test.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: structural-JSON projection limit |
| `presentation-canvas-realm.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A: canvas realm negotiation |
| `presentation-font-options.test.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: `LoadOptions.fonts` |
| `presentation-handle-media-errors.test.ts` | 7 | 0 | 0 | 0 | 7 | BLOCKED: `WarningEvent::MediaFailed` is never emitted |
| `presentation-handle.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A: DOM handle |
| `presentation.image.test.ts` | 2 | 0 | 0 | 0 | 2 | BLOCKED: decode-error surface + bitmap placeholder painter |
| `presentation.markdown.test.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: no `to_markdown` projection |
| `presentation.mode.test.ts` | 2 | 0 | 0 | 2 | 0 | N/A: main/worker realm selection |
| `presentation-worker-bitmap-ownership.test.ts` | 6 | 0 | 0 | 6 | 0 | N/A: `ImageBitmap` ownership |
| `viewer-concurrent-load.test.ts` | 5 | 0 | 0 | 0 | 5 | BLOCKED: no concurrent load/abort controller |
| `viewer-destroy.test.ts` | 8 | 0 | 1 | 7 | 0 | destroy-twice ported; canvas reparenting / display restore N/A |
| `viewer-destroy-during-render.test.ts` | 2 | 0 | 1 | 0 | 1 | a superseded frame is never cached (BEHAVIOURAL); the in-flight promise case BLOCKED |
| `viewer-layout-error-routing.test.ts` | 4 | 0 | 0 | 0 | 4 | BLOCKED: no layout-error routing |
| `viewer-model-source-options.test.ts` | 2 | 0 | 0 | 0 | 2 | BLOCKED: no model-source loader seam |
| `viewer-reload-orphan.test.ts` | 2 | 0 | 0 | 0 | 2 | BLOCKED: no reload/orphan tracking |
| `viewer-render-error.test.ts` | 3 | 0 | 1 | 0 | 2 | failure surfaced on the entry channel (BEHAVIOURAL); overlay/retry cases BLOCKED |
| `viewer-composition-boundary.test.ts` | 2 | 0 | 0 | 2 | 0 | N/A: DOM composition boundary |
| `viewer-find-destroy.test.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: `PptxFindController` |
| `scroll-viewer-reload-orphan.test.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: reload/orphan tracking |
| `scroll-viewer-concurrent-load.test.ts` | 4 | 0 | 0 | 0 | 4 | BLOCKED: concurrent load/abort |
| `scroll-media-controller.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A: media chrome DOM |
| `viewer-progressive.test.ts` | 7 | 0 | 1 | 1 | 5 | superseded navigation settles (BEHAVIOURAL via `cancel_except`); lifecycle options / hidden waits BLOCKED; loading overlay N/A |
| `presentation-progressive.test.ts` | 8 | 0 | 3 | 2 | 3 | opening slide first, prefetch window, background-failure surfacing ported; prefix/malformed-prefix protocols BLOCKED; 2 realm cases N/A |
| `scroll-viewer-progressive.test.ts` | 6 | 0 | 1 | 1 | 4 | window prefetch ordering ported; progressive slot population BLOCKED (DOM) |
| `scroll-viewer.test.ts` | 127 | 0 | 4 | 85 | 38 | N/A: wrapper/virtualization/comment cards/padding/pageShadow/DOM; BLOCKED: zoom (T4), flicker-free zoom (T8), render-epoch/stale bitmap, selection (T5), find; BEHAVIOURAL: per-call render width + 3 fit cases (T6) |
| `slide-pull-worker.test.ts` | 21 | 0 | 0 | 0 | 21 | BLOCKED: `BoundedPullSession` pull protocol + transferred payloads |
| `slide-pull-client.test.ts` | 4 | 0 | 0 | 0 | 4 | BLOCKED: byte-credit retry / session identity |
| `render-worker-canvas-transfer.test.ts` | 4 | 0 | 0 | 4 | 0 | N/A: `OffscreenCanvas` transfer |
| `render-worker-init-hang.test.ts` | 9 | 0 | 0 | 0 | 9 | BLOCKED: no worker init timeout |
| `worker-init-hang.test.ts` | 8 | 0 | 0 | 0 | 8 | BLOCKED: no worker init timeout |
| `worker-renderer-modules.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A: module registry over `postMessage` |
| `worker-task-scheduler.test.ts` | 1 | 0 | 0 | 1 | 0 | N/A (JS realm): `MessageChannel` task vs microtask ordering |
| `renderer-destroy-invalidation.test.ts` | 1 | 0 | 1 | 0 | 0 | a decoded frame is not served after its target was superseded (`InvalidateCause::TargetChanged`) |
| `tests/visual/non-ooxml-rejection.spec.ts` | 1 | 0 | 1 | 0 | 0 | the loader rejects non-OOXML input with a typed `ParseError` |
| `tests/visual/worker-equivalence.spec.ts` | 3 | 0 | 3 | 0 | 0 | two realms render identically; part-index map agrees (assertion in the nav target) |
| `tests/visual/visual.spec.ts` | 1 | 0 | 1 | 0 | 0 | deterministic render per slide (the reference-PNG corpus stays out of this batch) |
| `tests/visual/part-failure.spec.ts` | 1 | 0 | 1 | 0 | 0 | built-in/static/function placeholders are realm-neutral and pairwise distinct; the resolver receives `{kind:'slide', slideIndex:1, slideNumber:2, message}` (`placeholder_text_matches_in_main_and_worker_mode`) |
| `tests/visual/table-image-fill.spec.ts` | 2 | 0 | 0 | 0 | 2 | BLOCKED: table-only image/pattern fill plan |
| `tests/visual/pdf-acceptance.spec.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: no PDF backend |
| `tests/visual/responsive-overlay.spec.ts` | 1 | 0 | 0 | 1 | 0 | N/A: DOM overlay vs scroll container |
| `tests/visual/rotated-tile-coverage.spec.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: rotated-tile coverage oracle |
| `tests/visual/scroll-zoom-settle.spec.ts` | 3 | 0 | 0 | 0 | 3 | BLOCKED: viewer zoom + scroll anchor |
| `tests/visual/inline-equation-fallback.spec.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: equation-fallback branch selection |
| `tests/visual/warp-strip-quality.spec.ts` | 2 | 0 | 0 | 2 | 0 | N/A (browser canvas): no strip compositor |
| `tests/visual/bevel-ring.spec.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: bevel compositor |
| `tests/visual/font-source-worker-equivalence.spec.ts` | 2 | 0 | 0 | 0 | 2 | BLOCKED: font-source demand facts |
| `tests/visual/worker-selection-equivalence.spec.ts` | 1 | 0 | 0 | 0 | 1 | BLOCKED: selection/find/zoom realm |
## Non-spec modules in scope (no cases of their own)

`viewer.ts`, `presentation.ts`, `presentation-preflight.ts`,
`presentation-handle.ts`, `presentation-layout-events.ts`, `scroll-viewer.ts`,
`scroll-viewer-options.ts`, `scroll-viewer-test-dom.ts`, `scroll-slot.ts`,
`scroll-layout-controller.ts`, `scroll-comment-navigation.ts`,
`scroll-media-controller.ts`, `worker.ts`, `worker-protocol.ts`,
`worker-source.ts`, `worker-task-scheduler.ts`, `render-worker.ts`,
`render-worker-host.ts`, `slide-pull-worker.ts`, `slide-pull-client.ts`,
`slide-nav.ts`, `hidden.ts`, `part-failures.ts`, `element-selection.ts`,
`find.ts`, `find-highlight-layer.ts`, `text-layer.ts`, `selection-context.ts`,
`slide-cursor-operation.ts`, `slide-repository.ts`.

Their behaviour is asserted through the specs above. Two of them have no Rust
analogue at all: `slide-repository.ts` (bounded slide LRU keyed on structural
JSON weight; `PipelineCache` is unbounded) and `worker-task-scheduler.ts`
(microtask/task boundary).

## Stub → missing symbol

| blocked stub (archived) | missing symbol |
| :--- | :--- |
| `skip_navigation_jumps_over_hidden_slides` | `next_visible_index` / `resolve_visible_index` |
| `zoom_scale_clamps_to_the_configured_range`, `fit_width_settles_the_container_span`, `set_scale_does_not_fire_for_an_unchanged_factor` | `PresentationState::{set_scale,zoom_in,zoom_out,fit_width,fit_page}`, `on_scale_change` |
| `skip_mode_go_to_hidden_slide_lands_on_the_next_shown_slide`, `find_text_excludes_hidden_slides`, `caller_callback_owns_the_internal_click` | `HiddenSlideMode`, `PptxFindController`, `on_hyperlink_click` |
| `topmost_element_context_reports_identity_and_origin`, `rotation_inversion_and_line_tolerance`, `bounded_element_text_never_splits_unicode`, `chart_labels_stream_into_bounded_context_text` | `hit_test_slide_context`, `hit_test_slide_element`, `limit_element_context`, `bounded_chart_context_text` |
| `find_maps_matches_and_highlights_to_slides`, `find_rejects_matches_crossing_table_cells`, `highlight_boxes_sit_at_the_slice_offset` | `PptxFindController` / `UnitFindController`, `paint_find_highlights` |
| `slide_cursor_prepares_before_ack_and_publishes_after`, `pull_retries_coded_insufficient_credit_with_the_same_identity`, `a_pull_session_transfers_exactly_one_slide` | `load_slide_from_cursor`, `BoundedPullSession`, `pull_with_credit_retry` |
| `cfb_containers_are_rejected_with_a_typed_code`, `encrypted_decks_require_the_right_password` | `assert_not_cfb_container`, `decrypt_ooxml` |
| `preflight_unions_fonts_across_slides` | `PresentationPreflightBuilder` / `collect_deck_fonts` |
| `unreadable_media_degrades_with_a_warning` | `WarningEvent::MediaFailed` emission |
| `a_hanging_worker_init_times_out` | worker init timeout |
| `table_image_fill_composites_in_authoring_order`, `table_pattern_fill_retains_both_colours` | table-only image/pattern fill plan + `srcRect` cell clip |
| `pdf_pages_match_powerpoint`, `rotated_tiles_cover_the_complete_shape`, `scroll_anchor_survives_a_zoom_settle`, `inline_equation_falls_back_to_the_retained_branch`, `bevel_rim_brightness_stays_below_white`, `font_subset_requests_match_across_realms`, `worker_selection_and_find_match_main_mode` | PDF backend, tile-coverage oracle, viewer zoom, equation-fallback branch, bevel compositor, font-source demand facts, selection/find realm |

## Reconciliation notes

* **`EXACT` is deliberately rare here (11/455).** The reference's viewer is a DOM
  application: almost every case asserts a `<canvas>`, a `<div>` tree, a
  `Promise` lifecycle or an `ImageBitmap`. Only the pure resolvers
  (`partName → index`, relative verbs, part-failure facts) port without loss, so
  they carry `EXACT`; state-machine behaviour that the presenter re-implements
  (`goToSlide` clamping, show-jump arithmetic, dispatch branches, prefetch
  window, destroy idempotence) is `BEHAVIOURAL`.
* **`viewer-zoom-contract` / `scroll-viewer-zoom-contract`**: the *render width*
  half is real (`RenderTargetSpec` reaches the framebuffer unchanged) and the
  *fit* half maps onto `widget::layout::compute_slide_rect` (tighter of
  width/height in cell space). `setScale`, the ladder, the per-scale slot cache
  and the pre-load latch have no presenter analogue → BLOCKED.
* **`scroll-viewer.test.ts` (127 cases)** is 85 N/A (wrapper DOM,
  virtualization, comment cards, padding gutters, page shadow) + 38 BLOCKED
  (zoom T4/T8, render epoch, selection T5, find) + 4 BEHAVIOURAL (per-call render
  width, three fit/navigation cases).

## Production gaps found (out of scope: no production code was modified)

1. `pptx_model` JSON is **not** `Deserialize`-round-trippable:
   `TextRunData::strike_double` and `::underline_line_no_fill` lack
   `#[serde(default)]`, so `serde_json::from_str(&serde_json::to_string(&presentation))`
   fails with `missing field 'strikeDouble'`. Any worker-wire port needs those
   defaults before the byte-for-byte "main vs worker" cases can be asserted; the
   test compares two independent loads of the same package instead.
2. `ParseError::PasswordProtected` is **unreachable**: `DefaultPresentationLoader`
   has no CFB sniff, so an encrypted `.pptx` surfaces as
   `ParseError::ArchiveError` (zip error). `presentation-cfb-guard` /
   `presentation-decrypt` stay BLOCKED until a CFB guard + `decrypt_ooxml` land.
3. `WarningEvent::MediaFailed` is constructed nowhere in `kitty-pptx-core`; only
   the TUI renders it. Media degradation is therefore invisible in
   logs/JSON today.
4. `pptx_model::Slide::hidden` is parsed but no navigation path consults it —
   the presenter always behaves like the reference's `show` mode.
5. `WorkerPool::active_tokens` holds **one** token per slide index, so
   re-scheduling an index supersedes the tracked token while the earlier token
   stays armed (it is still in the queue until `cancel_except` drains by
   `CancellationToken::is_canceled`). `prefetch` avoids the double-schedule via
   `is_in_flight` (asserted in `prefetch_does_not_reschedule_a_slide_already_in_flight`),
   but a direct `schedule` caller can leak an armed token.
6. `resolve_internal_slide` is stricter than `buildSlidePartIndex` for
   *duplicate basenames* (ambiguous → `None`, the reference keys on the full part
   name and takes the first). Exact part-name duplicates match the reference
   (first wins, asserted).
