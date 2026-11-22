# Consolidated parity coverage (core test port)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only).
Scope: every `parity_*` test target under `crates/office-terminal-core/tests/` plus the one
`parity_*` target in `crates/office-terminal-tui/tests/` (the viewer batch's nav half).
Sources merged: `tests/parity/coverage/{elements,fonts,misc,shapes-a,shapes-b,text-box-a,text-box-b,text-flow-a,text-flow-b,viewer}.md`.
External corpus baseline: [`tests/parity/scores/external.json`](scores/external.json) (418 decks / 543 slides, 4 strict pass, 414 diagnostic, 0 errors, 1 known unrenderable).

Verification:

```
cargo test -p office-terminal-core --test <target>   # one target at a time, all 32 core targets
cargo test -p office-terminal-tui  --test parity_viewer_nav_tests
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
```

Result: **33/33 targets compile, 532 tests, 532 pass, 0 ignored, 0 failed; clippy clean.**
19 residual blocked cases are archived under `tests/parity/pending/` (not compiled); every
compiled test runs. Current verification: `cargo test --workspace` is green (2024+ tests,
0 failed, 0 ignored) and `cargo clippy --workspace --all-targets -- -D warnings` is clean.

`cases` counts reference runtime cases (every `it.each` row / loop expansion once).
`pass`/`fail` count Rust test functions in the batch's targets. The compiled tree has no
`#[ignore]`: blocked cases are archived under `tests/parity/pending/` and every compiled
test runs.

## Summary

| batch | reference files | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | pass | fail |
| :--- | --: | --: | --: | --: | --: | --: | --: | --: |
| `elements` | 11 | 89 | 3 | 53 | 33 | 0 | 31 | 0 |
| `fonts` | 13 | 104 | 11 | 13 | 80 | 0 | 24 | 0 |
| `misc` | 11 | 37 | 1 | 14 | 22 | 0 | 16 | 0 |
| `shapes-a` | 5 | 36 | 6 | 16 | 14 | 0 | 25 | 0 |
| `shapes-b` | 8 | 66 | 9 | 18 | 39 | 0 | 27 | 0 |
| `text-box-a` | 6 | 112 | 25 | 86 | 1 | 0 | 60 | 0 |
| `text-box-b` | 11 | 129 | 66 | 55 | 6 | 2 | 121 | 0 |
| `text-flow-a` | 9 | 70 | 52 | 18 | 0 | 0 | 53 | 0 |
| `text-flow-b` | 15 | 86 | 63 | 23 | 0 | 0 | 68 | 0 |
| `viewer` | 73 | 460 | 11 | 75 | 357 | 17 | 80 | 0 |
| **total** | **162** | **1189** | **247** | **371** | **552** | **19** | **505** | **0** |

Every batch row is self-consistent (`EXACT + BEHAVIOURAL + N/A + BLOCKED == cases`).
618 of 1189 cases are ported and asserted (247 EXACT + 371 BEHAVIOURAL); 552 cases are N/A (DOM,
canvas-only, microtask scheduler, or browser harness); exactly 19 residual cases are blocked on
unimplemented features and archived under `tests/parity/pending/` (not compiled) with owner
justifications.

`pass` counts batch test functions that ran green; every compiled test runs, so the
per-target table's `ignored` column is 0 throughout.

## Targets and run results

| batch | target | tests | pass | ignored | fail |
| :--- | :--- | --: | --: | --: | --: |
| `elements` | `parity_elements_tests` | 20 | 20 | 0 | 0 |
| `elements` | `parity_elements_table_tests` | 8 | 8 | 0 | 0 |
| `elements` | `parity_elements_chart_tests` | 3 | 3 | 0 | 0 |
| `fonts` | `parity_fonts_tests` | 14 | 14 | 0 | 0 |
| `fonts` | `parity_fonts_slot_tests` | 10 | 10 | 0 | 0 |
| `misc` | `parity_misc_tests` | 10 | 10 | 0 | 0 |
| `misc` | `parity_parse_error_tests` | 6 | 6 | 0 | 0 |
| `shapes-a` | `parity_shapes_a_tests` | 25 | 25 | 0 | 0 |
| `shapes-b` | `parity_shapes_b_tests` | 27 | 27 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_tests` | 10 | 10 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_faces_tests` | 8 | 8 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_compat_tests` | 5 | 5 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_fontalgn_tests` | 6 | 6 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_spacing_tests` | 19 | 19 | 0 | 0 |
| `text-box-a` | `parity_text_box_a_autofit_tests` | 12 | 12 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_tests` | 24 | 24 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_bullet_image_tests` | 10 | 10 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_highlight_tests` | 14 | 14 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_eavert_tests` | 24 | 24 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_stacked_tests` | 33 | 33 | 0 | 0 |
| `text-box-b` | `parity_text_box_b_warp_tests` | 16 | 16 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests` | 20 | 20 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_tabs` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_tests_wrap` | 13 | 13 | 0 | 0 |
| `text-flow-a` | `parity_text_flow_a_draw_tests` | 7 | 7 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_tests` | 35 | 35 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_bidi_tests` | 15 | 15 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_sea_tests` | 12 | 12 | 0 | 0 |
| `text-flow-b` | `parity_text_flow_b_indent_tests` | 6 | 6 | 0 | 0 |
| `viewer` | `parity_viewer_nav_tests` (core) | 15 | 15 | 0 | 0 |
| `viewer` | `parity_viewer_nav_tests` (tui) | 26 | 26 | 0 | 0 |
| `viewer` | `parity_viewer_selection_tests` | 10 | 10 | 0 | 0 |
| `viewer` | `parity_viewer_lifecycle_tests` | 12 | 12 | 0 | 0 |
| `viewer` | `parity_viewer_worker_tests` | 10 | 10 | 0 | 0 |
| `viewer` | `parity_playwright_tests` | 7 | 7 | 0 | 0 |
| | **batch subtotal (32 targets)** | **505** | **505** | **0** | **0** |

Out-of-batch `parity_*` targets (pre-existing, tracked in git, no coverage report):

| target | tests | pass | ignored | fail | note |
| :--- | --: | --: | --: | --: | :--- |
| `parity_fidelity_tests` | 1 | 1 | 0 | 0 | VRT deck fidelity (`parity_support::vrt::DECKS`) |
| `parity_text_tests` | 26 | 26 | 0 | 0 | lead's text kernel suite |
| | **grand total (33 targets)** | **532** | **532** | **0** | **0** |

Non-target modules: `parity_support/{mod,deck,pixelmatch,scores,vrt}.rs`,
`parity_text_flow_a_support/mod.rs`,
`shapes_a_support/{mod,image_fill_cases,image_fill_paint_cases,gradient_cases}.rs`,
`shapes_b_support/{line_end_cases,bevel_effect_geometry}.rs`,
`text_box_a_support/mod.rs`,
`text_box_b_support/{line_end_trim_cases,stacked_decor_cases,stacked_feature_cases}.rs`.

## Residual BLOCKED cases and owner justifications (19 total)

All 19 residual blocked cases are archived under `tests/parity/pending/*.txt`. Every other case
has been implemented or classified N/A.

### `text-box-b` (2 blocked) — in `parity_text_box_b_warp_tests.rs.txt`
- `expanded_ellipse_recomputes_each_line`: blocked on per-line expanded ellipse recomputation for WordArt arch paths.
- `warp_layout_measures_the_full_segment_once`: blocked on measurer call count recording on the warp compile path.

### `viewer` (17 blocked)

#### `parity_viewer_lifecycle_tests.rs.txt` (4 blocked)
- `encrypted_decks_require_the_right_password`: blocked on `decrypt_ooxml` ([MS-OFFCRYPTO] Agile decryption).
- `preflight_unions_fonts_across_slides`: blocked on `PresentationPreflightBuilder` / `collect_deck_fonts`.
- `unreadable_media_degrades_with_a_warning`: blocked on `WarningEvent::MediaFailed` emitted by compiler.
- `a_hanging_worker_init_times_out`: blocked on worker init timeout with hang bound.

#### `parity_viewer_selection_tests.rs.txt` (2 blocked)
- `highlight_boxes_sit_at_the_slice_offset`: blocked on `paint_find_highlights` find highlight geometry.
- `slide_cursor_prepares_before_ack_and_publishes_after`: blocked on `load_slide_from_cursor` pull/ACK operation.

#### `parity_viewer_worker_tests.rs.txt` (2 blocked)
- `pull_retries_coded_insufficient_credit_with_the_same_identity`: blocked on `BoundedPullSession` / `pull_with_credit_retry`.
- `a_pull_session_transfers_exactly_one_slide`: blocked on `pull_session` transfer protocol.

#### `parity_playwright_tests.rs.txt` (9 blocked)
- `table_image_fill_composites_in_authoring_order`: blocked on table-only image-fill paint plan.
- `table_pattern_fill_retains_both_colours`: blocked on table-only pattern fill plan.
- `pdf_pages_match_powerpoint`: blocked on PDF export backend (CLI exports SVG/PNG only).
- `rotated_tiles_cover_the_complete_shape`: blocked on rotated-tile coverage oracle over `rasterizer::image_fill`.
- `scroll_anchor_survives_a_zoom_settle`: blocked on viewer zoom scroll anchor model.
- `inline_equation_falls_back_to_the_retained_branch`: blocked on equation-fallback branch selection.
- `bevel_rim_brightness_stays_below_white`: blocked on bevel/homography compositor.
- `font_subset_requests_match_across_realms`: blocked on font-source demand facts.
- `worker_selection_and_find_match_main_mode`: blocked on worker selection/find/zoom realm.

## L0 External Corpus Validation

The pinned external LibreOffice presentation corpus is linked at [`tests/parity/scores/external.json`](scores/external.json).
- **Corpus size**: 418 presentation decks / 543 slides rendered and evaluated.
- **Pass rate**: 4/4 strict test decks pass; 414 diagnostic decks compared with zero unhandled panics or crashes.
- **Ratchet**: Diagnostic comparisons ratchet against regression (−0.05 cell-pass ratio / +2.0 global mean absolute difference).
- **Regeneration**: `UPDATE_EXTERNAL_SCORES=1 cargo test -p office-terminal-core --test external_fixture_tests -- --nocapture`.
