# Coverage — batch `text-flow-b` (text flow: CJK, SEA, bidi, indent)

Reference: `/tmp/oooxml` @ `1f5c6e1d`, `packages/pptx/src/*.test.ts`.
Rust targets (all `crates/kitty-pptx-core/tests/`):

- `T1` = `parity_text_flow_b_tests.rs` (CJK / EA / LB glue / Latin / whitespace)
- `T2` = `parity_text_flow_b_sea_tests.rs` (SEA wrapping + justification gaps)
- `T3` = `parity_text_flow_b_indent_tests.rs` (indent budget, superscript sizing, astral)

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cjk-wrap.test.ts` | 10 | 9 | 0 | 0 | 1 | T1, T3 | `fitCjkLine` has no Rust symbol; ported through `break_paragraph`'s first-line grapheme count (same 1/2 numbers). BLOCKED `:41-44` = `fitCjkLine(mc('、あ'), 15, 20) = 0` (needs a `start_width` argument; `BreakOptions` has none). `:47-51` "kinsoku off" ported via `eaLnBrk="0"`, which the crate documents as lifting kinsoku. Astral case passes: `𠮷𠮷、` retracts to `["𠮷","𠮷、"]`. |
| `cjk-latin-word-wrap.test.ts` | 4 | 4 | 0 | 0 | 0 | T1 | All four line vectors match, incl. the E06 per-grapheme emergency split at a font seam. |
| `cjk-spc-justify.test.ts` | 6 | 0 | 2 | 0 | 4 | T2 | `:113-258` (4a/4b) ported as the gap mask the contiguous draw relies on: `あ［AB］い` → gaps `[1,2,4,5]` (none between A and B, per_gap 35); `あいうえお` → gaps `[1,2,3,4]`, per_gap 37.5. BLOCKED `:113-211` (1–4): `rPr@spc` letter-spacing is not modelled at all — needed symbol `text::emit::StyleDef::letter_spacing` (absent) plus a canvas-style `letterSpacing` state on the paint pass. |
| `ea-line-break.test.ts` | 13 | 4 | 5 | 0 | 4 | T1 | E00/E01/E08-E09 and the "unobserved punctuation / mixed seams" vectors are exact. BEHAVIOURAL: table-cell case (same kernel, cell insets 0); ko/zh one-face segment (font string + `sizePx` not carried by `LineItem`); L00/L02/L07 (region splits exact; the `sizePx` facet is not expressible — `BrokenLine` carries no run sizes). BLOCKED: ja-JP two-run seam (`'漢漢' + '”漢'` → port gives `["漢漢","”漢"]`, ref `["漢","漢”","漢"]`; needs `BreakOptions::kinsoku_across_runs`), en-US seam, "decides by language", and the measured-language slot seam (needs `run.lang` + `paragraphInputRuns`' EA/Latin slot split). The one-run twins and the two-run `“` twin are asserted and pass. |
| `bidi-line.test.ts` | 8 | 0 | 0 | 0 | 8 | — | No bidi kernel exists: needed `text::bidi::{segments_have_rtl, compute_line_visual_order}` (UAX #9 segment reorder, tab = Bidi_Class S, mirrored cells under an RTL base). `para.rtl` only feeds `align_x`; nothing reorders segments. |
| `sea-line-break.test.ts` | 6 | 1 | 4 | 0 | 1 | T2 | BLOCKED `:71-83`: "breaks only at dictionary word boundaries" needs `sea_word_break_offsets` (ICU Thai dictionary); the port's documented fallback keeps Thai interior unbreakable. BEHAVIOURAL: one segment per line, narrow-box progress, one-line fit, Myanmar/Tibetan grapheme fill (asserted as "no line begins with a stacked mark" + text preserved + maximal fill). |
| `sea-transition-line-break.test.ts` | 2 | 0 | 2 | 0 | 0 | T2 | Thai↔digit transitions wrap, no number torn, a transition seam is used; mixed CJK+Thai keeps `ภาษาไทย` intact on one line. Note: the reference's `legal()` set is `seaMixedBreakOffsets`, whose dictionary spans are ICU-only, so the probe accepts Thai-interior breaks on both-SEA seams only (asserted explicitly). |
| `sea-justified-fit.test.ts` | 8 | 4 | 0 | 0 | 4 | T2 | EXACT for the dictionary-free rules: chunk fitting the remainder (1 line), zero shrink at 179 px (`มากมากมาก`/`ดังนี้`), the same under left alignment, and X=−3 at 183 px (1 line). BLOCKED Rules 1/2 and the over-full control: they assert the dictionary split points (120 px prefix), needing `sea_word_break_offsets`. |
| `boundary-glue-lb28-wrap.test.ts` | 4 | 4 | 0 | 0 | 0 | T1 | `<شيء`, `$100`, `(2026` all wrap down as units; `wwww ภาษา` is not retracted onto the `<a` line. |
| `latin-hyphen-wrap.test.ts` | 5 | 5 | 0 | 0 | 0 | T1 | HY/HH soft break at 140 px, U+2010, `--list-`, across a run seam; numeric range / word-initial / Hebrew never take it. |
| `latin-nonstarter-wrap.test.ts` | 2 | 2 | 0 | 0 | 0 | T1 | 165 px: `["aaaa bbbb ", "system, cc"]`; no line starts with the comma, also across a font seam. |
| `paragraph-trailing-whitespace.test.ts` | 5 | 5 | 0 | 0 | 0 | T1 | Terminal space makes no continuation line; suffix trimmed across runs; NBSP/U+2007/U+202F preserved (UTF-16 stub). |
| `firstline-indent-wrap.test.ts` | 3 | 3 | 0 | 0 | 0 | T3 | 4 tokens at no indent (200 px with the kept space), 2 tokens at an 80 px indent (100 px ≤ 120), continuations use the full width; explicit 0 == omitted. |
| `superscript-citation-wrap.test.ts` | 3 | 0 | 2 | 0 | 1 | T3 | 65% citation (386 px) fits 390 on one line; full-size control (400 px) orphans `"1"`. Font-string (`13px`/`20px`) and `sizePx` facets BLOCKED (`StyleDef`/`LineItem` carry no font text). BLOCKED `:129-150`: `naturalWidthExceedsBbox` autofit preflight has no public symbol. |
| **totals** | **79** | **41** | **15** | **0** | **23** | | 56 cases ported in 53 test functions; 53/53 pass. |

## Missing symbols (aggregate)

1. `pptx::cjk_wrap::fit_cjk_line(chars, start_width, max_width, rules)` — 1 case.
2. `text::emit::StyleDef::letter_spacing` (`rPr@spc`) + paint-side spacing state — 4 cases.
3. `BreakOptions::kinsoku_across_runs` cross-run kinsoku evidence — 2 cases (ja-JP seams, one-face slot seam).
4. `run.lang` / `paragraphInputRuns` language slot seam (`fontFamilyEa`) — 2 cases.
5. `text::bidi::{segments_have_rtl, compute_line_visual_order}` — 8 cases.
6. `sea_word_break_offsets` (ICU Thai/Lao/Khmer) — 5 cases.
7. `natural_width_exceeds_bbox` autofit preflight — 1 case.
8. Per-unit authored size / font string on `LineItem`/`BrokenLine` — facets of 4 ported cases (noted in place).
