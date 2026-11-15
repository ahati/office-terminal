# Parity status after the wave-4/5 pass

Measured at the commit that lands V2–V7, S4, S5, P3/P4 and the partial L0
ledger guard. `GAP_PLAN.md` remains the plan of record; this file records what
the current tree actually contains.

## Project tests

| crate | tests |
| :--- | --: |
| `kitty-pptx-core` | 957 |
| `kitty-pptx-tui` | 107 |
| `kitty-pptx-display` | 12 |
| `kitty-pptx-cli` | 28 |
| **total** | **1104** |

`cargo test --workspace` is green; `cargo clippy --workspace --all-targets --
-D warnings` is clean; every project source file is < 600 lines.

## Landed in this pass

- **V2 find over slide text** — `core/src/text/find.rs` (run-level
  `FindController`, case-insensitive non-overlapping matches, table-cell and
  hidden-slide boundaries) + `tui/src/app/find_state.rs` and the `Ctrl+F` find
  modal. `/` remains the fuzzy TOC search.
- **V3 element selection** — `core/src/selection.rs`: reverse paint-order hit
  test, inverse rotation/flip frame, line tolerance, bounded UTF-16 text,
  chart-context summary and id-based bounds lookup.
- **V4 part-failure delivery** — `collect_part_failures` sorted/deduplicated.
- **V5 pipeline hardening** — cancellation is checked again before
  `put_vector`; `DeckReload` clears the per-slide failure latch; regression
  tests.
- **V6 container guard** — CFB `EncryptedPackage` → `PasswordProtected`,
  legacy `PowerPoint Document` → typed legacy error, unknown CFB/plain bytes →
  not-an-OOXML error.
- **V7 zoom/fit** — shared 0.25…4 ladder, `fit_width`/`fit_page`, latched
  scale applied to the render target, `+`/`-` keys.
- **S4 SmartArt contrast** — background/gradient luminance, synthetic-shape
  fingerprint and the white-on-dark fallback.
- **S5 multi-column text** — `text/columns.rs` balanced overflow columns and
  `rtlCol` mirroring, wired into the emitter.
- **P3 table image fill** — verified the cell blip reaches the display list
  with its `srcRect`; regression test.
- **P4 media poster** — media placeholder emits a `DrawImage` for `posterPath`
  before the vector chrome.
- **S8 helper** — `rasterizer/reflection_blur.rs` band planner (pure API);
  shape reflection keeps its uniform authored-sigma blur.
- **T6 (partial)** — `spAutoFit` natural-width preflight suppresses wrapping
  when every paragraph fits the box.
- **T8 stacked layout** — `text/vertical/stacked_layout.rs`: multiple glyphs
  per column, cell-pitch advance, balanced column wrapping, RTL mirroring,
  per-face usWin cell metrics, grapheme clusters spanning authored runs,
  ASCII-bracket rotation, character prefixes, picture prefixes and inline
  equation cells. Face-feature coverage is handled for the known `vrt2`/`vert`
  cases (`∥`, JhengHei `ー`).
- **T9 stacked content** — equation objects paint their math paths, complete
  authored `mathFallbacks` branches replace their run range, per-glyph
  hyperlinks are projected into `VectorSlide.hyperlinks`, the shape
  text-rotation/flip helper is landed, and stacked strikes/underlines plus the
  paragraph-mark strike gate are emitted as vector bars (one underline frame
  per column). Character and auto-number prefixes occupy cells.
- **T4 RTL tabs** — LTR/RTL multi-cell tab lines are verified end-to-end
  (mirrored stops, reversed visual order, start-tab gap, first-line indent).
- **T5 kinsoku seams** — `TextMeasure::kinsoku_across_runs` carries host evidence;
  the real measurer enables retraction when both runs are East Asian by
  `rPr@lang` or face, while the default stays C08-safe.
- **T10 warp residuals** — `warp_envelope_for_line` expands
  single-edge arch/circle envelopes radially, clockwise presets anchor the
  last line and `textArchDown` anchors the first; warp layout now applies
  `a:lnSpc` (pts and pct), `spcBef`/`spcAft`, mixed-size line boxes and the
  0.8 baseline fraction. Standalone SVG emits per-glyph warped outlines (both
  vector and selectable modes force outlines for a warped run), and a long
  compound segment stays one run with no wrap fragments. 16 warp tests green.
- **S7a (partial)** — all 12 Office bevel profile curves plus the `circle`
  fallback (`scene3d/bevel_profiles.rs`), and all 15 `ST_PresetMaterialType`
  surface classes in the scalar Blinn-Phong table.
- **S7b (partial)** — `legacyWireframe` paints front/extrusion rims with no
  filled face; extrusion/contour code split into `shape_effect/extrusion.rs`.
- **L0 (partial)** — `parity_ledger_tests.rs`: zero `#[ignore]`, pending-archive
  hygiene and ledger arithmetic. Full per-case re-derivation and the external
  corpus still require `target/external` and a fresh reference run.

## Still open

T7 residual general `Decoration` IR (stacked glyph decorations are landed),
T8/T9 residual cases (full face-feature tables), the full S7a/S7b multi-light
rig/shading and rear-plane bevel compositor, P5 (bubble3D/ChartEx/marker
fills), and the full L0 corpus/ledger re-derivation. The external corpus cannot
be fetched in this environment (no network; `target/external` is empty), so the
419-deck comparison remains unverified. The ledger in `COVERAGE.md` is still the
wave-3 snapshot; do not treat its BLOCKED counts as current.
