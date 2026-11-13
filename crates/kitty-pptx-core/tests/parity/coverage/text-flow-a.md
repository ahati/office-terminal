# Parity coverage — batch `text-flow-a`

Reference: `/tmp/oooxml/packages/pptx/src` @ `1f5c6e1d` (read-only).
Rust targets (all in `crates/kitty-pptx-core/tests/`):

- `parity_text_flow_a_tests.rs` — justification kernel (specs' own `splitBefore`/`perGap` numbers)
- `parity_text_flow_a_tests_tabs.rs` — tab-stop grid kernel
- `parity_text_flow_a_tests_wrap.rs` — tab-led / multi-cell line wrapping
- `parity_text_flow_a_draw_tests.rs` — draw pass through `compile_text_body`
- `parity_text_flow_a_support/mod.rs` — shared harness (reference stubs + pen reconstruction)

Units: the specs build canvas px from EMU through `SCALE = 1/12700` (1 pt → 1 px);
the kernels here are linear, so the probes use the specs' px numbers verbatim.
Stubs: `advance = chars*20`/`space = 20` (`pptx-tab-wrap`, `pptx-multi-tab`,
`rtl-tab-stops`); explicit widths (`text-justify`, `tab-layout`); real shaping for
the draw pass (measurement is not stubbable there — `Measurer` is `pub(super)`).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| `text-justify.test.ts` | 20 | 19 | 1 | 0 | 0 | `parity_text_flow_a_tests.rs` | `justify_line` returns a boundary mask over the flat unit stream, so `splitBefore=[1,2]` reads `gap_positions()==[1,2]`; `perGap==per_gap`. `:190-193` (style preservation) is behavioural: no annotated copies exist, so the contract is "items/styles never rewritten". |
| `justify-line-break.test.ts` | 2 | 0 | 2 | 0 | 0 | `parity_text_flow_a_draw_tests.rs` | `<a:br/>`-terminated `just` line extent == natural advance, `dist` line extent == box width with uniform pitch `slack/(cps-1)`. Reference's "one contiguous fillText" carrier cannot exist in a positional IR (noted, see bracket file). |
| `justify-bracket-overlap.test.ts` | 4 | 0 | 4 | 0 | 0 | `parity_text_flow_a_draw_tests.rs` | Fully distributed line: one run per code point (IR has no `letterSpacing`), so the fix's *geometry* is asserted: uniform `advance+perGap`, no adjacent overlap, last glyph on the box edge; partial-gap case asserts the exact piece split `[あ][［][AB][］][い]`. Case 4(a) left-aligned control ported; 4(b) `@spc` half blocked. |
| `tab-layout.test.ts` | 13 | 13 | 0 | 0 | 0 | `parity_text_flow_a_tests_tabs.rs` | `resolve_tab_widths` with the spec's items/stops/pen/limit; every width vector matches exactly (incl. `defTabSz` grid and the #835 clamp). |
| `tab-stop-spc.test.ts` | 4 | 0 | 1 | 0 | 3 | `parity_text_flow_a_draw_tests.rs` | Case 4 (`ls==0`) behavioural: one piece, `x+width == 400 px` stop. Cases 1–3 need `rPr@spc` → BLOCKED on `letterSpacing` (see below). |
| `pptx-tab-wrap.test.ts` | 9 | 9 | 0 | 0 | 0 | `parity_text_flow_a_tests_wrap.rs` | `break_paragraph` with `advance=chars*20`; line texts, stop-seated pens (100/200/300) and text-left continuations all match. |
| `pptx-multi-tab.test.ts` | 4 | 3 | 0 | 0 | 1 | `parity_text_flow_a_tests_wrap.rs` | LTR three-cell, start-tab gap and indent-stop cases exact (pens reconstructed from resolved item widths). RTL mirroring BLOCKED. |
| `rtl-tab-stops.test.ts` | 7 | 0 | 1 | 0 | 6 | `parity_text_flow_a_tests_wrap.rs` | Case 5's reading-frame half ported (`[50,20,0,40]`); its mirrored canvas x (440/480) and cases 1–4, 6–7 BLOCKED. |
| `spc-wrap.test.ts` | 2 | 0 | 0 | 0 | 2 | — | BLOCKED on `letterSpacing`: no wrap-budget path consumes `rPr@spc`. |
| `spc-cluster-advance.test.ts` | 12 | 0 | 0 | 0 | 12 | — | BLOCKED on `letterSpacing` (cluster advances, EA seam, tab seam, RTL seam, grapheme clusters, inert/absent-canvas modes). |
| **total** | **77** | **44** | **9** | **0** | **24** | | **53 cases ported, 53 tests passing** |

## Blocked cases — symbols needed

- `rPr@spc` (ST_TextPoint, §21.1.2.3.9): `pptx-model::TextRunData.letter_spacing` is
  parsed but nothing in `kitty-pptx-core` reads it. Needed: a letter-spacing field on
  `text::emit::StyleDef` + `TextMeasure::advance`/`linebreak::measure_items` folding
  `(clusters-1)*ls` into the advance, and a per-piece spacing on
  `contracts::PositionedGlyphRun`. Blocks `tab-stop-spc` 1–3, `spc-wrap` 1–2,
  `spc-cluster-advance` 1–12.
- RTL/bidi tab-cell mirroring: the paint pass has no visual-order pass, so a stop's
  mirrored x (`(bx+bw-rIns)-pos`) and the cell's trailing-edge anchor do not exist.
  Blocks `pptx-multi-tab` 2 and `rtl-tab-stops` 1–4, 6–7.
- Canvas-only structure: `fillText` call granularity and `ctx.letterSpacing` strings
  have no analogue in a vector IR; every such assertion is restated as compiled
  geometry (extents/pitch/piece split) rather than dropped.

## Files created

- `crates/kitty-pptx-core/tests/parity_text_flow_a_tests.rs` (20 tests)
- `crates/kitty-pptx-core/tests/parity_text_flow_a_tests_tabs.rs` (13 tests)
- `crates/kitty-pptx-core/tests/parity_text_flow_a_tests_wrap.rs` (13 tests)
- `crates/kitty-pptx-core/tests/parity_text_flow_a_draw_tests.rs` (7 tests)
- `crates/kitty-pptx-core/tests/parity_text_flow_a_support/mod.rs` (harness)
- `tests/parity/coverage/text-flow-a.md` (this report)

Verification:

```
CARGO_TARGET_DIR=target/agent-text-flow-a cargo test -p kitty-pptx-core \
  --test parity_text_flow_a_tests --test parity_text_flow_a_tests_tabs \
  --test parity_text_flow_a_tests_wrap --test parity_text_flow_a_draw_tests
# 20 + 13 + 13 + 7 = 53 passed, 0 failed
CARGO_TARGET_DIR=target/agent-text-flow-a cargo clippy -p kitty-pptx-core \
  --test parity_text_flow_a_tests --test parity_text_flow_a_tests_tabs \
  --test parity_text_flow_a_tests_wrap --test parity_text_flow_a_draw_tests -- -D warnings
# clean
```
