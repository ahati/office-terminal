# Coverage — batch `text-flow-b` (text flow: CJK, SEA, bidi, RTL tabs, indent)

Reference: `/tmp/oooxml` @ `1f5c6e1d`, `packages/pptx/src/*.test.ts`.
Rust targets (all `crates/kitty-pptx-core/tests/`):

- `T1` = `parity_text_flow_b_tests.rs` (CJK / EA / LB glue / Latin / whitespace)
- `T2` = `parity_text_flow_b_sea_tests.rs` (SEA wrapping + justification gaps)
- `T3` = `parity_text_flow_b_indent_tests.rs` (indent budget, superscript sizing, astral)
- `T4` = `parity_text_flow_b_bidi_tests.rs` (UAX#9 segment order, reading-frame tab stops)

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cjk-wrap.test.ts` | 10 | 9 | 0 | 0 | 1 | T1, T3 | `fitCjkLine` has no Rust symbol; ported through `break_paragraph`'s first-line grapheme count (same 1/2 numbers). BLOCKED `:41-44` = `fitCjkLine(mc('、あ'), 15, 20) = 0` (needs a `start_width` argument; `BreakOptions` has none). `:47-51` "kinsoku off" ported via `eaLnBrk="0"`, which the crate documents as lifting kinsoku. Astral case passes: `𠮷𠮷、` retracts to `["𠮷","𠮷、"]`. |
| `cjk-latin-word-wrap.test.ts` | 4 | 4 | 0 | 0 | 0 | T1 | All four line vectors match, incl. the E06 per-grapheme emergency split at a font seam. |
| `cjk-spc-justify.test.ts` | 6 | 0 | 2 | 0 | 4 | T2 | `:113-258` (4a/4b) ported as the gap mask the contiguous draw relies on: `あ［AB］い` → gaps `[1,2,4,5]` (none between A and B, per_gap 35); `あいうえお` → gaps `[1,2,3,4]`, per_gap 37.5. BLOCKED `:113-211` (1–4): `rPr@spc` tracking now exists end-to-end (`StyleDef::letter_spacing_emu`, folded into `Measurer::advance` as `spc × (cps − 1)`, ref `renderer.ts:1672-1692`), but these four cases assert canvas letter-spacing state the positional IR does not expose — ported by `text-flow-a` (`tab-stop-spc`/`spc-*`), tracked there. |
| `ea-line-break.test.ts` | 13 | 4 | 5 | 0 | 4 | T1 | E00/E01/E08-E09 and the "unobserved punctuation / mixed seams" vectors are exact. BEHAVIOURAL: table-cell case (same kernel, cell insets 0); ko/zh one-face segment; L00/L02/L07 (region splits exact; the `sizePx` facet is not expressible — `BrokenLine` carries no run sizes). BLOCKED: ja-JP two-run seam, en-US seam, "decides by language", measured-language slot seam (need `BreakOptions::kinsoku_across_runs` + a `run.lang` seam in `break_paragraph`; the `a:ea` slot itself now exists, see `fonts.md`). |
| `bidi-line.test.ts` | 8 | 8 | 0 | 0 | 0 | T4 | Every `it()` ported on the new `text::bidi` kernel: `has_strong_rtl` gate (`RTL_GATE`, `line-order.ts:16-24`), then `compute_line_visual_order` = UAX#9 P2-L2 over `packages/core/src/text/bidi/{segment-line,line-order,uax9/rules}.ts` with the DrawingML `isTab` hook (tab = Bidi_Class `S`). Order **and** per-segment `rtl` vectors match, incl. mixed-direction content inside one tab cell. |
| `rtl-tab-stops.test.ts` | 7 | 5 | 2 | 0 | 0 | T4 | Cases 1, 2, 4, 6, 7 EXACT (the cell's trailing/centre edge lands on the mirrored stop `BOX_W − pos`, the overflow clamp pins it at x = 0, and the LTR frame is the regression guard). `[width]`: the reference's mock measures every glyph at the font size, so case 2/7 assert the stop relation from the port's measured advance. BEHAVIOURAL: case 3 (per-segment direction — the port carries `rtl` on the L2 result, not on `PositionedGlyphRun`; the kernel assertion plus the physical position is the observable) and case 5 (the reading-frame start gap is materialised: `A`'s right edge sits one 50 px start-tab gap from the leading edge and the 140 end stop stays reachable). Moved here from `text-flow-a.md`: `parity_text_flow_a_tests_wrap.rs::c25_*` still asserts case 5's kernel half. |
| `sea-line-break.test.ts` | 6 | 1 | 4 | 0 | 1 | T2 | BLOCKED `:71-83`: "breaks only at dictionary word boundaries" needs `sea_word_break_offsets` (ICU Thai dictionary); the port's documented fallback keeps Thai interior unbreakable. BEHAVIOURAL: one segment per line, narrow-box progress, one-line fit, Myanmar/Tibetan grapheme fill. |
| `sea-transition-line-break.test.ts` | 2 | 0 | 2 | 0 | 0 | T2 | Thai↔digit transitions wrap, no number torn, a transition seam is used; mixed CJK+Thai keeps `ภาษาไทย` intact on one line. |
| `sea-justified-fit.test.ts` | 8 | 4 | 0 | 0 | 4 | T2 | EXACT for the dictionary-free rules: chunk fitting the remainder (1 line), zero shrink at 179 px, the same under left alignment, and X=−3 at 183 px. BLOCKED Rules 1/2 + the over-full control: they assert the dictionary split points (120 px prefix), needing `sea_word_break_offsets`. |
| `boundary-glue-lb28-wrap.test.ts` | 4 | 4 | 0 | 0 | 0 | T1 | `<شيء`, `$100`, `(2026` all wrap down as units; `wwww ภาษา` is not retracted onto the `<a` line. |
| `latin-hyphen-wrap.test.ts` | 5 | 5 | 0 | 0 | 0 | T1 | HY/HH soft break at 140 px, U+2010, `--list-`, across a run seam; numeric range / word-initial / Hebrew never take it. |
| `latin-nonstarter-wrap.test.ts` | 2 | 2 | 0 | 0 | 0 | T1 | 165 px: `["aaaa bbbb ", "system, cc"]`; no line starts with the comma, also across a font seam. |
| `paragraph-trailing-whitespace.test.ts` | 5 | 5 | 0 | 0 | 0 | T1 | Terminal space makes no continuation line; suffix trimmed across runs; NBSP/U+2007/U+202F preserved (UTF-16 stub). |
| `firstline-indent-wrap.test.ts` | 3 | 3 | 0 | 0 | 0 | T3 | 4 tokens at no indent (200 px with the kept space), 2 tokens at an 80 px indent (100 px ≤ 120), continuations use the full width; explicit 0 == omitted. |
| `superscript-citation-wrap.test.ts` | 3 | 0 | 2 | 0 | 1 | T3 | 65% citation (386 px) fits 390 on one line; full-size control (400 px) orphans `"1"`. Font-string (`13px`/`20px`) and `sizePx` facets BLOCKED (`StyleDef`/`LineItem` carry no font text). BLOCKED `:129-150`: `naturalWidthExceedsBbox` autofit preflight has no public symbol. |
| **totals** | **86** | **54** | **17** | **0** | **15** | | 71 cases ported in 68 test functions (35 T1 + 12 T2 + 6 T3 + 15 T4); 68/68 pass. |

## Missing symbols (aggregate)

1. `pptx::cjk_wrap::fit_cjk_line(chars, start_width, max_width, rules)` — 1 case.
2. `BreakOptions::kinsoku_across_runs` cross-run kinsoku evidence — 2 cases (ja-JP seams).
3. `run.lang` / `paragraphInputRuns` measured-language slot seam — 2 cases (the
   `a:ea` slot dispatch itself now exists, `src/text/slots.rs`).
4. `sea_word_break_offsets` (ICU Thai/Lao/Khmer) — 5 cases.
5. `natural_width_exceeds_bbox` autofit preflight — 1 case.
6. Per-unit authored size / font string on `LineItem`/`BrokenLine` — facets of 4 ported cases (noted in place).

Landed in this pass: `text::bidi` (`src/text/bidi/{mod,class,explicit,levels,sequence}.rs`,
1:1 port of `packages/core/src/text/bidi/**` + the pptx `bidi-line.ts` wrapper) for
`bidi-line`; draw-time reading-frame tab resolution
(`paint::resolve_line_tabs`, ref `renderer.ts:5683-5722`) plus the UAX#9 L2 paint
order (`paint::place_line`, ref `renderer.ts:5930-5940`) for `rtl-tab-stops`,
with `emit` engaging the pass only when the base direction is RTL or the line
carries strong-RTL text (`paraNeedsBidi`, ref `renderer.ts:5689-5692`). No golden
output changed (`git status` clean for `tests/golden/`).
