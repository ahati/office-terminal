# text-box-a coverage

Reference specs under `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d`. Cases are
`it()` instances (an `it.each` row counts once). `tests` = test functions in
the port.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
|---|---|---|---|---|---|---|---|
| `powerpoint-line-metrics.test.ts` | 60 | 9 | 1 | 1 | 49 | `parity_text_box_a_tests.rs` | #1610 baseline/space/anchor controls ported against `natural_line_box`/`spaced_line_box`/`paragraph_spacing` + the `renderer.ts:5530-5575` anchor, with the reference catalogue shares passed in (`support::ARIAL`…). BLOCKED: `powerPointAscentShare` catalogue + `OBSERVED_SUBSTITUTES` (A2, A3, D1-D3), `SHARE_CACHE_LIMIT` LRU (A10), the whole `compatLnSpc="0"` Excel box (`excelDrawingMlLineRatios`, B2-B5), `fontAlgn` (`powerPointFontAlgn*`, C1 34 rows, C2), `a:br`/`endParaRPr` face accounting (C3-C6). N/A: C7 (V8 spread-argument limit). |
| `percentage-line-spacing.test.ts` | 6 | 0 | 6 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | pitch = 1.2 × size / spcPts for every face, plain and bulleted, through `compile_text_body`. |
| `percentage-paragraph-spacing.test.ts` | 12 | 0 | 12 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | 24/36/48 pt gaps, first-line `spcBef` / last-line `spcAft`, `spcFirstLastPara` edges, ctr/b anchors. |
| `norm-autofit-omitted-scale.test.ts` | 3 | 0 | 3 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | painted size 24 at 140/76/44 pt boxes; stored `fontScale` 0.5 → 12. |
| `firstline-indent-autofit.test.ts` | 12 | 6 | 2 | 0 | 4 | `parity_text_box_a_autofit_tests.rs` | wrap budgets 135.6/185.6 with the 10-per-char stub, 20/15/5 rows, l/ctr/r pens 0/80/160, ±50 pt draw shift. BLOCKED: `naturalWidthExceedsBbox` (F1, F3), the `a:ea` metric slot (F4), tab start pen = `marL + textXOffset` (F8). |
| `sp-autofit-top-anchor.test.ts` | 19 | 7 | 8 | 0 | 4 | `parity_text_box_a_autofit_tests.rs` | 120 % pitch in both fit modes, usWin split, ctr/b overflow, zero-height b growth. BLOCKED: the resolved canvas font box (`useResolvedFontMetrics`) for S7-S9. |

Totals: 112 cases — 22 EXACT, 32 BEHAVIOURAL, 1 N/A, 57 BLOCKED; 54 ported,
41 test functions, all passing.

Production divergences found (not fixed, not owned): the wrap budget has no
bullet gate on `first_line_indent`; the tab start pen omits `marL`; `b`
anchoring clamps where `renderer.ts:5559` does not; `compat_ln_spc`,
`font_algn` and `end_run_properties` are parsed but unused.

Files: `crates/kitty-pptx-core/tests/parity_text_box_a_tests.rs`,
`parity_text_box_a_spacing_tests.rs`, `parity_text_box_a_autofit_tests.rs`,
`tests/text_box_a_support/mod.rs` (shared fixtures, not a target).
