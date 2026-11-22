# text-box-a coverage

Reference specs under `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d`. Cases are
`it()` instances (an `it.each` row counts once). `tests` = test functions in
the port.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
|---|---|---|---|---|---|---|---|
| `powerpoint-line-metrics.test.ts` | 60 | 12 | 44 | 1 | 3 | `parity_text_box_a_tests.rs`, `..._faces_tests.rs`, `..._compat_tests.rs`, `..._fontalgn_tests.rs` | #1610 baseline/space/anchor controls ported against `natural_line_box`/`spaced_line_box`/`paragraph_spacing` + the `renderer.ts:5530-5575` anchor. Landed (T1): the pinned catalogue (`text/faces.rs`, `assets/fonts/office-face-metrics.json`) wired into `shape::ascent_share_for` so `compile_text_body` uses the catalogued split (A1, A2 `Calibri Light` cut, A3 `OBSERVED_SUBSTITUTES`), the bounded share/face LRUs (A10), the `compatLnSpc="0"` Excel box (B2-B5) and `fontAlgn` (C1 34 rows, C2, C3-C6) through the two-model `Stack`. BLOCKED (3): the `absent authored faces` cases need the emitter's painting-face selection. N/A: C7 (V8 spread-argument limit). |
| `percentage-line-spacing.test.ts` | 6 | 0 | 6 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | pitch = 1.2 × size / spcPts for every face, plain and bulleted, through `compile_text_body`. |
| `percentage-paragraph-spacing.test.ts` | 12 | 0 | 12 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | 24/36/48 pt gaps, first-line `spcBef` / last-line `spcAft`, `spcFirstLastPara` edges, ctr/b anchors. |
| `norm-autofit-omitted-scale.test.ts` | 3 | 0 | 3 | 0 | 0 | `parity_text_box_a_spacing_tests.rs` | painted size 24 at 140/76/44 pt boxes; stored `fontScale` 0.5 → 12. |
| `firstline-indent-autofit.test.ts` | 12 | 6 | 2 | 0 | 4 | `parity_text_box_a_autofit_tests.rs` | wrap budgets 135.6/185.6 with the 10-per-char stub, 20/15/5 rows, l/ctr/r pens 0/80/160, ±50 pt draw shift. BLOCKED: `naturalWidthExceedsBbox` (F1, F3), the `a:ea` metric slot (F4), tab start pen = `marL + textXOffset` (F8). |
| `sp-autofit-top-anchor.test.ts` | 19 | 7 | 8 | 0 | 4 | `parity_text_box_a_autofit_tests.rs` | 120 % pitch in both fit modes, usWin split, ctr/b overflow, zero-height b growth. BLOCKED: the resolved canvas font box (`useResolvedFontMetrics`) for S7-S9. |

Totals: 112 cases — 25 EXACT, 75 BEHAVIOURAL, 1 N/A, 11 BLOCKED; 101 ported,
60 test functions, all passing.

Production divergences found (not fixed, not owned): the wrap budget has no
bullet gate on `first_line_indent`; the tab start pen omits `marL`; `b`
anchoring clamps where `renderer.ts:5559` does not.

T1 remaining production step (recorded, not owned by `text/{faces,power_point,
metrics,shape}.rs`): `text/emit.rs` must select the model per body —
`a:bodyPr@compatLnSpc="0"` → `power_point::compat_off_natural_line`, any run
whose `power_point_face_metrics` is `None` → the ordinary model for the whole
body, `a:pPr@fontAlgn` → `font_algn_reference` + per-run `font_algn_offset`,
and the `a:br` / `endParaRPr` marks into that line's metric runs (K4-K6
`followingMark`, `renderer.ts:5341-5346`). `power_point.rs` is not dead code:
the compat and fontAlgn ports drive it directly, and the 3 `absent authored
faces` cases + sp-autofit S7-S9 stay blocked on that dispatch.

Files: `crates/office-terminal-core/tests/parity_text_box_a_tests.rs`,
`parity_text_box_a_spacing_tests.rs`, `parity_text_box_a_autofit_tests.rs`,
`parity_text_box_a_faces_tests.rs`, `parity_text_box_a_compat_tests.rs`,
`parity_text_box_a_fontalgn_tests.rs`,
`tests/text_box_a_support/{mod,stack}.rs` (shared fixtures, not a target).
