# TEXT / TEXT-RENDERING gaps — implementation spec (core)

Reference: `/tmp/oooxml` @ `1f5c6e1d` (read-only). Counts: `tests/parity/COVERAGE.md` + `tests/parity/coverage/*.md`,
re-verified against code and `#[ignore]` stubs (Wave-2 landed `text/{bullets,paragraphs,decor,stacked}` and
`StyleDef::letter_spacing_emu`, so several COVERAGE rows are stale). Verify per agent:
`CARGO_TARGET_DIR=/workspace/kitty-pptx/target/agent-<name> cargo test -p kitty-pptx-core --test <target>` and
`cargo clippy -p kitty-pptx-core --lib -- -D warnings`. Every file < 600 lines, no `unwrap()` in production.

## 0. Snapshot — remaining BLOCKED text cases (160; 94 still open, 19 landed uncommitted)

| gap | cases | one-line blocker | prod LOC |
| :--- | --: | :--- | --: |
| **T1** face-metrics catalogue + PowerPoint line model | 54 → **7 open** | catalogue + kernels + ascent-share wiring landed; residual: `text/emit.rs` model dispatch (marks, `compatLnSpc`, `fontAlgn`, any-unresolved-face) | 460 |
| **T2** script-slot dispatch (`a:ea`/`a:cs`/`a:sym`) + app defaults | 14 → **3 open** | slots + `east_asian_default` landed uncommitted; residual: `vert` feature model, FF1A class | 120 |
| **T3** `rPr@spc` letter spacing end-to-end | 22 | `letter_spacing_emu` counts code points, not clusters; no run seam; splits drop it | 90 |
| **T4** bidi order + RTL tab frame | 15 → **7 open** | `text::bidi` + visual-order paint landed uncommitted; residual: RTL tab READING frame (`tab_start_pen`) | 110 |
| **T5** break seams (`kinsokuAcrossRuns`), tab start pen | 6 | `BreakOptions` carries no seam/pen evidence; one bullet-indent stub | 80 |
| **T6** spAutoFit re-derivation (`naturalWidthExceedsBbox`) | 3 | `do_wrap = wrap != "none"`; no preflight | 110 |
| **T7** glyph decorations in the IR (underline/`uLn`, double strike) | 7 | IR has `bool` underline/strike; nothing positional | 250 |
| **T8** stacked WordArt layout | 14 | one grapheme per column; no cell pitch/`lnSpc`/wrap/`algn` | 420 |
| **T9** stacked content (math, hyperlink, frame, prefix cells) | 11 | `compile_stacked` skips objects, prefix cells, rotation | 260 |
| **T10** warp residuals (`lnSpc` stack, single-edge outset, SVG) | 14 | warp stacks lines at 1.2 em, one envelope for all lines, SVG ignores `warp` | 340 |

Out of scope (already owned): arrowheads/`cmpd` 6 → `gaps/paint.md` §7 (`line-end-shaft-trim.test.ts`); multi-column
bodies 2 → `gaps/structure.md` G5. Delete-not-ignore: §12 (6). Two parallel agents landed T2/T4 while this was written
(`text/{slots,east_asian_default}.rs`, `text/bidi/`, `tests/parity_{fonts_slot,text_flow_b_bidi}_tests.rs`) — run those
targets before re-dispatching, and update the ledger's stale rows for both.

## T1 — face-metrics catalogue + the PowerPoint line model (54 cases → **7 open**)

**Landed (wave 3, `agent-w3-faces`).** `assets/fonts/office-face-metrics.json` (the reference
JSON verbatim, 1050 profiles) + `text/faces.rs` (`power_point_ascent_share`,
`power_point_face_metrics`, `FaceMetricsFull{share,glyph,excel,decoration}`, the
`SHARE_CACHE_LIMIT` LRUs, `OBSERVED_SUBSTITUTES`, `is_east_asian_face`,
`is_serif_latin_face`) + `text/power_point.rs` (`compat_off_natural_line`,
`font_algn_reference`, `font_algn_offset`, `FONT_ALGN_UNIT_PT`).
`text/shape.rs::ascent_share_for` now reads the catalogue first (metric-compatible
bundled substitutes map back to the authored Office face), so
`compile_text_body` lays every baseline out with the catalogued split;
`metrics::FaceMetrics::for_family` is the shared constructor. Ports:
`parity_text_box_a_faces_tests.rs` (A1, A2, A3, A10, the glyph/Excel/decoration,
boxes), `parity_text_box_a_compat_tests.rs` (B2-B5) and
`parity_text_box_a_fontalgn_tests.rs` (C1 ×34, C2-C6) through the two-model
`text_box_a_support::Stack`; `parity_text_box_b_highlight_tests::highlight_uses_the_face_glyph_box`
and `parity_text_box_b_eavert_tests::latin_faces_classify_by_panose_serif_style`
reinstated. Coverage: `tests/parity/COVERAGE.md` text-box-a 11 BLOCKED (was 57).

**Still open (7).** `text/emit.rs` (not owned by this agent) must select the model
per body: `compatLnSpc="0"` → the Excel box, any `power_point_face_metrics` = `None`
→ the ordinary model for the whole body, `fontAlgn` → the reference + per-run
offset, and `a:br` / `endParaRPr` marks into the line's metric runs. That
unblocks the 3 `absent authored faces` cases + `sp-autofit` S7-S9
(`useResolvedFontMetrics`).

**Cases.** `powerpoint-line-metrics.test.ts` 49 of 60 (A2/A3/A10, B2–B5, C1 ×34 rows, C2, C3–C6, plus
`:70 :83 :94 :195 :236-299 :301-467`); `sp-autofit-top-anchor.test.ts` S7–S9 (4: `:134`, `:202`, `:257`);
`text-highlight.test.ts` `:141` (1).

**Current Rust.** `text/metrics.rs` has the kernels (`natural_line_box`, `spaced_line_box`,
`exact_line_points`, `paragraph_spacing`, `block_top`, `align_x`) but the share comes from
`text/shape.rs::ascent_share_for` — a live `hhea`/`line_gap` probe clamped to `[0.5, 0.98]`. Every blocked
case pins numbers from the reference's **static** catalogue (`Arial = 1854/2288`), which no installed-face
probe reproduces. `compat_ln_spc`, `font_algn`, `end_run_properties` are parsed and unused.
Reference: `packages/pptx/src/powerpoint-line-metrics.ts` `:39-49` `OBSERVED_SUBSTITUTES`, `:51-81`
share + `SHARE_CACHE_LIMIT = 256` LRU, `:96-141` `chosenProfiles`/`resolveShare` (prefer
`macos-supplemental`, typo table when `USE_TYPO_METRICS`), `:143-221` glyph/Excel/decoration boxes,
`:223-279` interned `PowerPointFaceMetrics`, `:296-356` natural + compat-off lines, `:358-469`
`powerPointFontAlgn{Reference,Offset}` + `FONT_ALGN_UNIT_PT = 0.72`. Data:
`core/src/fonts/reference-font-metrics-data.json` (1050 profiles, 327 KB) +
`core/src/fonts/office-auto-line.ts::excelDrawingMlLineRatios` (1.3× Far-East box).

**Required changes.**
1. Asset `assets/fonts/office-face-metrics.json` = the reference JSON verbatim (data, not source lines),
   parsed once into a `HashMap`. `src/text/faces.rs` (new):
```rust
pub struct ExcelLineBox { pub ascent: f32, pub descent: f32 }              // em ratios
pub struct DecorationGeometry { pub underline_top_em: f32, pub underline_thickness_em: f32,
                                pub strike_top_em: f32, pub strike_thickness_em: f32 }
pub struct FaceMetricsFull { pub share: f32, pub glyph: Option<ExcelLineBox>,
                             pub excel: Option<ExcelLineBox>, pub decoration: Option<DecorationGeometry> }
pub const SHARE_CACHE_LIMIT: usize = 256;
pub fn power_point_ascent_share(family: &str, bold: bool, italic: bool) -> Option<f32>;
pub fn power_point_face_metrics(family: &str, bold: bool, italic: bool) -> Option<Arc<FaceMetricsFull>>;
pub fn is_east_asian_face(family: &str) -> bool;   // OS/2 Far-East code page (T2)
pub fn is_serif_latin_face(family: &str) -> bool;  // PANOSE 2/2..=10 (T2)
```
   `None` = uncatalogued face → keep today's model (`metricOk = false`, `renderer.ts:225`), which is the
   reference's own fallback; `ascent_share_for` = `face_metrics.share` else the current probe.
2. `src/text/power_point.rs` (new): `compat_off_natural_line(runs) -> (f32,f32)`,
   `font_algn_reference(algn, runs, compat_off, unit) -> Option<(f32,f32)>`,
   `font_algn_offset(algn, run, compat_off, unit) -> Option<f32>` — ports of `:343-469`.
3. `text/emit.rs` `StyleDef` gains `face: Option<Arc<FaceMetricsFull>>` (drawn slot) and `face_latin`
   (`lineMetricLatin`, `renderer.ts:1928-1938`); `natural_box` uses `face.share`; `emit` builds
   `metric_runs` = text items **+ `a:br` mark + `endParaRPr` mark** (`renderer.ts:2136-2160`) and falls
   back to the ordinary model when any is `None`. `compatLnSpc="0"` → compat-off box (+ Excel box for
   `spcPct`); `fontAlgn` → per-run offset on each item's baseline (new `LineLayout::font_algn_offset`).

**Test mapping.** `parity_text_box_a_tests.rs` 49 new fns. `shares_follow_uswin_per_face`:
`power_point_ascent_share("Arial",false,false) == 1854.0/2288.0`, `("Gabriola",..) == 0.814` (typo) ≠ `0.647`.
`share_cache_is_bounded`: 300 distinct faces → cache len ≤ `SHARE_CACHE_LIMIT`.
`compat_off_uses_the_excel_box`: Arial 100 pt → split at `1.15·100` snapped to 1/100 in.
`font_algn_t_puts_the_run_box_top_on_the_reference`:
C1 row (Arial,`t`,100 %) → `round(ascent/0.72)*0.72`. `parity_text_box_a_autofit_tests.rs` S7–S9:
`use_resolved_font_metrics = resolvedFontHeight > 1.2·maxSize` + the top-anchored-pct correction.
`parity_text_box_b_highlight_tests.rs::highlight_uses_the_face_glyph_box`: Arial 48 pt box top `baseline − 42.75`, height `53.64`.

**Deps** none (foundation for T2/T7/T8/T10) · **risk** medium: static shares shift existing baselines
≈0.5 pt; no other agent may edit `metrics.rs` concurrently · **owner** `text/{faces,power_point}.rs`*,
`text/metrics.rs`, `text/emit.rs` (`StyleDef`/`Measurer` region), asset.

## T2 — script-slot dispatch + PowerPoint application defaults (14 cases)

**Cases.** `east-asian-default.test.ts` 8 of 8 (`:15` PANOSE, `:26` tier coverage, `:37` EA Latin stack,
`:43` cs per script, `:86` empty ea slot, `:93` ea+cs split, `:100` ea symbols/quotes, `:123` per-cluster
symbol stack); `firstline-indent-autofit.test.ts` F4 (`:131` `a:ea` metric slot); `font-requests.test.ts`
2 of 11 (`:21` authored cs face retained for shape + table); `eavert-glyph-orientation.test.ts` 3
(`orientation` class of U+FF1A `：` must rotate, and the two `vert`-feature probes
`helper_uses_the_vert_feature_for_long_marks_only` / `helper_keeps_uncovered_glyphs_on_the_plain_fallback`).
Four of the eight are stubbed as
`parity_text_box_b_eavert_tests.rs::{empty_east_asian_slot_uses_the_application_default,
resolved_east_asian_face_is_kept_and_complex_scripts_split,east_asian_face_claims_symbols_and_curly_quotes,
east_asian_slot_symbol_stack_resolves_per_cluster}`.

**Current Rust.** `paragraphs.rs` now calls `slots::push_run_items` with `font_family_ea/cs/lang` (landed
uncommitted, `tests/parity_fonts_slot_tests.rs`); the residual is the `vert` font-feature model behind
`eavert-glyph-orientation.test.ts` (2 cases) and the FF1A `：` class (1). Reference: `packages/pptx/src/east-asian-default.ts:1-120` (`SERIF_TIERS`/`SANS_TIERS`,
`complexScriptDefaultFace`, `isEastAsianSlotSymbol`, `EA_SLOT_SYMBOLS`, `QUOTE_SEAM_LANG_RE`) +
`east-asian-default-coverage.ts` (8.7 KB bitmap/ranges); per-cluster slot choice `renderer.ts:1907-1988`.

**Required changes.** `src/text/east_asian_default.rs` (new) + `assets/text/east-asian-defaults.json`
(bitmap as hex): `east_asian_default_faces(latin, text) -> Vec<&'static str>`,
`complex_script_default_face(ch) -> &'static str`, `is_east_asian_slot_symbol(cp, lang) -> bool`.
`paragraphs.rs` splits each run per grapheme cluster into slot-keyed `BreakItem::Text`
(`split_slots(text, style) -> Vec<(String, StyleId)>`, reusing `vertical::graphemes`); `StyleDef` gains
`lang`; `a:sym` maps U+F020–U+F0FF through the `map_bullet_char` substitution.

**Test mapping.** `parity_text_box_b_eavert_tests.rs` un-ignore 8: `east_asian_latin_face_keeps_its_own_stack` →
`east_asian_default_faces("Yu Gothic","日本語かな") == ["Yu Gothic","Microsoft JhengHei","Malgun Gothic"]`;
`empty_east_asian_slot_uses_the_application_default` → compiled run face `"MS Gothic"`, **not** `"Corbel"`;
`complex_script_default_face_follows_the_script` → `ע`/`ع`→`Arial`, `ภ`→`Angsana New`, `ह`→`Mangal`, `த`→`Latha`.
`parity_fonts_tests.rs` 2: `שלום` with `font_family_cs="Calibri"` compiles to `font_family == "Calibri"` (shape + table).

**Deps** T1 (`is_serif_latin_face`) · **risk** low-medium (slot splitting multiplies items; `StyleId` coalescing must
keep measure==paint) · **owner** `text/{slots,east_asian_default}.rs`*, `text/paragraphs.rs`, asset,
`parity_text_box_b_eavert_tests.rs`; the `vert` model lives in `text/vertical/eavert.rs` (post-split).

## T3 — `rPr@spc` letter spacing (22 cases)

**Cases.** `tab-stop-spc.test.ts` 1–3, `spc-wrap.test.ts` 1–2, `spc-cluster-advance.test.ts` 1–12 (17 in `text-flow-a`,
**no stubs** — that target has 0 `#[ignore]`); `cjk-spc-justify.test.ts` 1–4; eavert `helper_advances_by_measure_plus_spacing`.

**Current Rust.** `StyleDef::spacing_advance` adds `ls·(chars−1)` (`text/emit.rs:38-46`). Divergences:
(a) the reference counts **shaping clusters** (`measureTextAdvance`, `renderer.ts:1672-1691`), so
`e + U+0301` adds 0; (b) one spacing unit belongs at a **same-source-run seam** (`boundaryAdvance`,
`:2089`; `leadingLetterSpacingPx`, `:2158-2161`) — `StyleDef` has no run identity, and an intra-run seam
currently coalesces, losing one unit; (c) justified splits (`paint.rs::place_text`) drop `ls`.
Reference: `renderer.ts:1611-1614` (preflight), `1672-1691`, `1885` (`spc·PT_TO_EMU`), `2000-2004`
(`sameStyle` requires one `sourceRunId` when `ls != 0`), `2085-2096` (measure + boundary advance),
`5709/5751` (tab widths), `5942-5950` (`visualBoundarySpacing`), `5983-6030` (`letterSpacing` paint,
`pitch = ls + segPerGap`).

**Required changes.** `StyleDef` += `source_run_id: usize`; `spacing_advance` counts
`vertical::graphemes(text).len()-1`. `linebreak.rs::measure_items` adds `ls` again when the next item
shares `source_run_id`; `make_line` must not coalesce non-zero-`ls` items across runs. `paint.rs::place_text`
advances each split piece by `advance(piece) + ls`. `Measurer` gets
`pub(super) fn seam_advance(&self, left: StyleId, right: StyleId) -> f32` (concrete type; `TextMeasure`
stays 2-method).

**Test mapping.** New `parity_text_flow_a_spc_tests.rs` (17). `positive_and_negative_spacing_only_at_internal_cluster_boundaries`
→ `"AB"` ls=4 measures `24` and the next run starts at `A.x+24`; `combining_sequence_is_one_cluster` → `"e\u{301}"` ls=4
advance `10`, not `14`; `ea_font_seam_keeps_the_spacing` → `"Aあ"` Meiryo: `あ.x − A.x == 10+ls`, `あ.x+あ.w − A.x == 20+ls`;
`distinct_authored_runs_have_no_seam` → `A`(4)→`あ`(4) starts at `A.x+10`. `parity_text_flow_b_sea_tests.rs` 4 (gaps
`[1,2,4,5]`/`per_gap 35`, `[1,2,3,4]`/`37.5`); `parity_text_box_b_eavert_tests.rs` 1: advance `= measure + ls` per code point.

**Deps** T4 for the RTL visual-seam case · **risk** medium (`source_run_id` changes coalescing → re-run
every wrap vector) · **owner** `text/emit.rs` (`StyleDef`/`Measurer`, after T1), `text/linebreak.rs`,
`text/paint.rs`, `text/tabs.rs`.

## T4 — bidi visual order + RTL tab frame (15 cases)

**Cases.** `bidi-line.test.ts` 8 of 8 (`segmentsHaveRtl`; LTR keep; pure-RTL reverse; embedded LTR; neutral object;
tab=S mirroring; LTR tab order; mixed cell); `pptx-multi-tab.test.ts` 2 of 4; `rtl-tab-stops.test.ts` 1–4, 6–7.
Only stub: `parity_text_flow_a_tests_wrap.rs:230` (case 5 ported reading-frame only, canvas x blocked).

**Current Rust.** `text/bidi/` (class/levels/sequence/explicit + `compute_line_visual_order`) and the
visual-order paint pass landed uncommitted, so only the tab frame is open: tabs still resolve in the wrap
pass with `start_pen = first_line_indent`, `limit = INFINITY` (`linebreak.rs:250-266`) and no mirroring.
Reference: `packages/core/src/text/bidi/{segment-line.ts,line-order.ts}` (UAX #9, tab = Bidi_Class S,
U+FFFC per object), adapter `packages/pptx/src/bidi-line.ts`. Draw side `renderer.ts:5680-5690`
(`paraNeedsBidi`, `hasTab`), `5700-5730` (`leadingIndentPx = baseRtl ? marR : marL + textXOffset`,
`limitPx = textMaxW + marL + marR`), `5875-5900` (`penX = textX + textMaxW − reserve − lineWidth`),
`5925-5950` (visual order + `visualBoundarySpacing`), `5655-5676` (column shift).

**Required changes.** `src/text/bidi.rs` (new): `segments_have_rtl(items) -> bool` (`RTL_GATE` ranges) and
`order(items: &mut Vec<LineItem>, maths: &mut Vec<usize>, base_rtl: bool) -> Vec<bool>` — one logical
string (`U+FFFC` per object/tab) → `unicode_bidi::BidiInfo::new(text, Some(level))` → each item's
first-unit level → L2 reorder. `unicode-bidi 0.3.18` is **already in `Cargo.lock`** (via `usvg`): one added
`Cargo.toml` line (lead approval). `BreakOptions` += `tab_start_pen: f32` (`mar_l`/`mar_r`,
`tabStartPen`, `renderer.ts:2090`). Paint: clone items, apply `order`, re-resolve tab widths with the finite
limit, then `line_x = l_ins + avail_w − reserve − width` for `hasTab && rtl`; carry the math piece index
(T9) so reordering cannot desync `object_cursor`.

**Test mapping.** New `parity_text_flow_b_bidi_tests.rs` (8): `tab_separates_cells_under_an_rtl_base` → visual order
`["C","TAB","B","TAB","A"]`, `rtl == [true,·,true,·,true]`; `mixed_cell_reorders_inside_the_cell` → `["CD","אב","TAB"]`,
`rtl[hebrew]==true`, `rtl[latin]==false`. `parity_text_flow_a_tests_wrap.rs` 7: `rtl_right_tab` → cell `x == 200`
(=600−400) and `!= 360`; `ctr` → `180`; overflow stop → `0`; case (5) → `x == 440`, `A.x == 480`; `pptx-multi-tab` 2
→ mirrored three-cell order.

**Deps** T3 (visual seam), T5 (start pen) · **risk** high (reorder changes command order for every RTL
line) · **owner** `text/bidi.rs`*, `text/tabs.rs`, `text/paint.rs`, `text/emit.rs` (line loop), `Cargo.toml`.

## T5 — break seams and the tab start pen (6 cases)

**Cases.** `ea-line-break.test.ts` 4 (`:107` ja-JP two-run seam, `:116` en-US seam, `:121` decides by
language, `:134` measured-language slot seam); `firstline-indent-autofit.test.ts` F8 (`:177`).

**Current Rust.** `linebreak.rs::BreakOptions` documents the absence: "a run seam keeps its greedy break
(Office C08)"; the kinsoku adjustment is gated on `prev.run == next.run` (`:419`). `measure_items` omits
`marL`/`marR` from the start pen (`:250-256`) — that is F8.
Reference: `renderer.ts:2030-2058` (`powerPointKinsokuAcrossRuns`: both langs `^ja`, curly quote ↔ Han;
plus the one-face slot seam under `QUOTE_SEAM_LANG_RE = /^(?:en-US|ja-JP)$/i`), `2077-2096`
(`kinsokuAcrossRuns` + `tabStartPen` into `breakDrawingMlText`).

**Required changes.** `BreakOptions` += `tab_start_pen: f32` and
`kinsoku_across_runs: Option<&'a dyn Fn(StyleId, &str, StyleId, &str) -> bool>`; `measure_items` uses
`tab_start_pen + (line_index == 0 ? first_line_indent : 0)`; `:419` becomes
`prev.run == next.run || seam_cb(..)`.

**Test mapping.** `parity_text_flow_b_tests.rs` 4 (`kinsoku crosses a ja-JP run seam`: `'漢漢'+'”漢'` →
`["漢","漢”","漢"]`, not `["漢漢","”漢"]`). `parity_text_flow_a_tests_wrap.rs` F8 →
`tab_stop_stays_absolute_when_the_first_line_hangs`: `marL 50`, indent `−50` → cell right edge on stop `300`.
Already expressible, un-ignore now: `parity_text_tests.rs:557` `bulleted_first_line_ignores_the_positive_indent`
→ bulleted para, `indent +50` wraps at the full width.

**Deps** T4 (shares `tab_start_pen`) · **risk** low · **owner** `text/linebreak.rs` (after T3), `text/emit.rs` (one `BreakOptions` literal).

## T6 — spAutoFit re-derivation (3 cases)

**Cases.** `firstline-indent-autofit.test.ts` F1 (`:94` bulleted → FALSE), F3 (`:122` `@spc` counted); `superscript-citation-wrap.test.ts` `:113-150`.

**Current Rust.** `emit.rs:224` `let do_wrap = text_body.wrap != "none";` — no preflight, so an
`spAutoFit` box always wraps and the "grow horizontally" branch never runs. Reference:
`renderer.ts:1575-1618` `naturalWidthExceedsBbox` (sums `measureTextAdvance`, uses
`firstLineIndentPxFor(paragraphHasBullet(para), indent)` so a bullet gutter contributes 0, returns true on
the first paragraph with `lineW > textMaxW`), `:5141-5147` when `autoFit === 'sp'`.

**Required changes.** `src/text/preflight.rs` (new):
```rust
pub fn natural_width_exceeds_bbox(paras: &[ParagraphLayout], box_w: f32, l_ins: f32, r_ins: f32,
                                  measurer: &Measurer) -> bool;
```
`emit` gates `do_wrap` on it for `auto_fit == "sp"`; the bullet gate reuses the already-correct
`marker.is_some()` first-line indent.

**Test mapping.** `parity_text_box_a_autofit_tests.rs` 3 new: F1 → bulleted, `indent +50`, natural `135.6`
in a `200`-wide box → `false` (non-bullet control `true`); F3 → `"AB"` `@spc 4` natural `24` → `true` at
width `20`; citation → 65 % run fits `390` → one line.

**Deps** T1/T2/T3 (advance parity) · **risk** low · **owner** `text/preflight.rs`*, `text/emit.rs` (one line).

## T7 — glyph decorations in the IR (7 cases)

**Cases.** `stacked-text.test.ts` 7: `:380` (×2 glyph-sized double strike), `:415` (strike follows the
glyph frame), `:446` (unstruck `endParaRPr` mark), `:463` (×2 per-column underline frame), `:508`
(underline from the face descent).

**Current Rust.** `PositionedGlyphRun { underline: bool, strikethrough: bool }`; the rasterizer fabricates
bars (`rasterizer/text.rs:152-190`: `0.06 em` at `baseline+2`, `0.05 em` at `−0.35 em`), SVG emits
`text-decoration` (`svg/text.rs:173-176`), stacked paints nothing. `a:u`, `uFill`/`uLn`,
`strike="dblStrike"` and the face's own `post`/`OS/2` rule are unrepresentable.
Reference: `renderer.ts:801-860` `paintRunUnderline`/`paintRunStrike` (double strike = two bands,
offset `lineW` with face geometry else `0.9·lineW`); `core/src/text/underline.ts:13-90`
(`DECORATION_LINE_WIDTH_EM 0.05`, `STRIKE_RISE_EM 0.32`, heavy `1.8`, `dbl` gap `1.4 lw`, `a:uLn @w`
override); face rule from `powerPointFaceMetrics(..).decoration` (T1).

**Required changes.** `contracts/text.rs`:
```rust
pub struct Decoration { pub style: String /* ST_TextUnderlineType */, pub color: [u8;4],
                        pub width_emu: Option<f32>, pub dbl: bool }
pub struct PositionedGlyphRun { /* .. */ pub underline: Option<Decoration>,
                                pub strike: Option<Decoration> }   // strike.dbl = dblStrike
```
`text/paint.rs` fills them from `StyleDef`; `rasterizer/text.rs` traces the reference geometry (dash
dispatch, crisp snap, two bands); `svg/text.rs` emits `<path>` strokes in the run transform; stacked
columns emit one decoration per column frame.

**Test mapping.** `parity_text_box_b_stacked_tests.rs` 7: `stacked_paints_glyph_sized_strikes` → exactly 3
strike paths; glyph 0 horizontal span `24·0.6`; two bands `lineW` apart.
`stacked_underline_uses_the_face_descent` → `y == 7.2 + (619/2048)·24 − 2.5` (Yu Gothic).

**Deps** T1 (`decoration`), T8 (frames) · **risk** medium (three backends) · **owner**
`contracts/text.rs`, `text/paint.rs`, `rasterizer/text.rs`, `svg/text.rs`.

## T8 — stacked WordArt layout (14 cases)

**Cases.** `stacked-text.test.ts` 14: `:172` cell pitch, `:182` face cell, `:198` ASCII bracket rotated,
`:204`/`:210` `vert` feature (T2), `:293`/`:312` horizontal-cluster segmentation, `:416`/`:425` prefix
cells, plus the `lnSpc`/wrap/`algn` cases the current model cannot express.

**Current Rust.** `text/stacked.rs` (131 lines) + `vertical::place_stacked`: **one grapheme per column**, no
breaking along the column, no `lnSpc`/`spcBef`/`spcAft`, no `algn`, `thickness = advance`.
`text/vertical.rs` is 598 lines — split before extending.
Reference: `core/src/drawingml-text/stacked.ts:137-288` (`layoutStackedText`: columns as rotated line
boxes, `thickness = max cell`, `drawingMlSpacedLineBox` spacing, `algn` along the column, `just`/`dist`
stretching, trailing-space exclusion, `firstLinePrefix`), `stacked-faces.ts:25-125`
(`STACKED_CELL_FACTOR = 7/6`, `stackedFaceBox`, `stackedSidewaysCharacter`, `stackedVerticalGlyph`),
`stacked-glyphs.ts:88-151` (`stackedVerticalForm`, `stackedUprightBaseline`, `paintStackedRotated`);
glue `renderer.ts:4849-5028`.

**Required changes.** Split `text/vertical.rs` → `vertical/{mod,eavert,stacked}.rs` (no behaviour change).
New `text/stacked_layout.rs`:
```rust
pub struct StackedCell { pub text: String, pub style: StyleId, pub kind: CellKind,
                         pub advance: f32, pub thickness: f32, pub space: bool }
pub enum CellKind { Upright, VerticalGlyph, Rotated }
pub struct StackedColumn { pub axis_x: f32, pub thickness: f32, pub para: usize, pub prefix_top: Option<f32> }
pub fn layout_stacked(paras: &[StackedPara<'_>], o: &StackedOptions<'_>) -> Vec<StackedColumn>;
```
`thickness = 7/6 · max cell` (face box from T1); the column length feeds `break_paragraph` as `max_width`;
`spaced_line_box` applies `lnSpc`.

**Test mapping.** `parity_text_box_b_stacked_tests.rs`: `stacked_glyphs_advance_one_cell_along_the_column` → successive
baselines differ by `CELL = 7/6·size`; `stacked_cell_follows_the_drawing_face` → Yu Gothic cell ≠ Arial cell at 24 pt;
a new `lnSpc 150 %` case → column pitch `1.5·CELL`.

**Deps** T1 (face box), T2 (`vert` feature) · **risk** medium · **owner** `text/vertical/*`,
`text/stacked_layout.rs`*, `text/stacked.rs`.

## T9 — stacked content: math, hyperlink, frame, prefix cells (11 cases)

**Cases.** `stacked-text.test.ts` 11: `:227` run hyperlink, `:266` `shapeTextRotation` frame flips,
`:335`/`:344` cached equation, `:349`/`:354` math fallback text, `:360` complete inline branch, `:431`
picture-marker aspect (now paintable — verify), plus the two prefix-cell cases shared with T8.

**Current Rust.** `compile_stacked` iterates `BreakItem::Text` only, has no math/hyperlink/frame-rotation
path, and paints one image per paragraph at `slots[start]`.
Reference: `renderer.ts:4849-4940` (stacked runs: hyperlink, frame, math zone as one object glyph via
`objectGlyph`, fallback text when the renderer is unavailable), `4952-4975` (prefix cells, `IfdImage`),
`5040-5083` (`shapeTextRotation`).

**Required changes.** `StackedOptions::object_glyph: Option<&dyn Fn(&MathPiece) -> StackedCell>`
(equation = one cell, advance = upright height, `renderer.ts:4884-4896`); `compile_stacked` consumes
`BreakItem::Object` in order (reuse `paragraphs::line_math_pieces`), emits `piece.fallback` text on failure,
forwards `PositionedGlyphRun::hyperlink`, and applies
`shape_text_rotation(vert, rotation, flip_h, flip_v) -> AffineTransform` before `emit_run`; the marker label
becomes leading `StackedCell`s of the paragraph's first column.

**Test mapping.** `stacked_cached_equation_keeps_the_horizontal_geometry` → exactly one added cell whose
advance equals the piece height; `stacked_body_rotation_follows_the_frame_flips` → `rotation 90°, flipV`
yields the transposed transform (`a==0 && d==0`); `stacked_picture_markers_keep_their_aspect` → `w/h == 2.0`
at `size_pt 24`.

**Deps** T8, T7 (frames, shared `PositionedGlyphRun` edit) · **risk** medium · **owner** `text/stacked.rs`,
`text/stacked_layout.rs`.

## T10 — warp residuals (14 cases + SVG)

**Cases.** `wordart-follow-path.test.ts` 14 of 19: `:115`/`:126` per-line envelope expansion,
`:136`/`:157`/`:165`/`:173`/`:181` `lnSpc` 100/150/200 % + `spcPts`, `:146` spcBef/spcAft, `:190` mixed-size
accumulation, `:198`/`:207` winding at 135°/225°, `:216`/`:227` circle radial expansion, `:266` one measure.

**Current Rust.** `compiler/text.rs:105-500` stacks lines at `max(run size)·1.2·fontScale` with baseline
`cur_y + 0.75·line_h`, ignores `lnSpc`/`spcBef`/`spcAft`, maps every line through **one** envelope (no
single-edge `outset`), ignores winding, derives `ink_h` from the em box. `svg/text.rs` never reads
`run.warp`/`glyph.warp_u`, so **SVG export drops the warp** (no reference case; internal parity asset).
Reference: `renderer.ts:3260-3300` (flat stacking from `spaceBefore`/`spaceLine`/`spaceAfter`,
`baseline = flatTop + height·0.8`; `clockwise` winding from the envelope's first two points;
`outset → buildWarpEnvelope(boxW+2·outset, boxH+2·outset)`), `3336-3370` (`inkH = maxA+maxD`,
`baselineFrac = maxA/inkH`, `hScale = boxW/totalW`, `warpBoxH = inkH/(v1−v0)`), `3390-3450` (follow offset).

**Required changes.** `compiler/text_warp/lines.rs` (new): `flat_lines(paras, measurer) -> Vec<FlatLine>`
(top/height/`baseline = top + 0.8·height`), `warp_envelope_for_line(env, outset) -> Arc<WarpEnvelope>`,
`anchor_line(lines, clockwise) -> f32`. `text/shape.rs::ink_box(..) -> (f32, f32)` via
`ttf_parser::Face::glyph_bounding_box` replaces the em-box heuristic. SVG: map each glyph through
`warp_glyph_transform` and emit its outline with the per-glyph affine (paired) or rotation (single edge).

**Test mapping.** `parity_text_box_b_warp_tests.rs` 14: `warp_uses_50_percent_line_spacing` → baselines differ by
`1.2·size·0.5`; `warp_honours_fixed_line_spacing` → `spcPts 36` → pitch `36 pt`; `multi_line_arch_anchors_the_last_line`
→ the clockwise arch anchors the **last** line (outset 0); `circle_anchors_the_final_line_and_expands_radially` → line 2's
radii differ by `outset`. New `tests/warp_svg_parity_tests.rs`: SVG `<path>` for glyph *i* and the rasterizer's warped
origin agree within `0.5 EMU`.

**Deps** T1 (face metrics/ink), T3 (`spc` in the warp measure) · **risk** high: `0.75 → 0.8` moves every
`warp_parity_tests` number; re-derive from the reference, never tweak · **owner** `compiler/text.rs` (warp
half), `compiler/text_warp/lines.rs`*, `svg/text.rs`, `text/shape.rs` (`ink_box`).

## 12. NOT APPLICABLE — delete the stub (never leave `#[ignore]`d)

| file / case | why |
| :--- | :--- |
| `cjk-wrap.test.ts` `:41-44` `fitCjkLine("、あ",15,20)==0` | `cjk-wrap.ts::fitCjkLine` has **no call site** in the renderer (exported for its spec only); the same 行頭/行末禁則 behaviour is already pinned by the ported `cjk-wrap` vectors through `break_paragraph`. |
| `sea-line-break.test.ts` `:71-83`; `sea-justified-fit.test.ts` Rules 1/2 + over-full control (4) | the reference dictionaries come from `Intl.Segmenter` ICU word data (Thai/Lao/Khmer); the crate has no ICU provider (`icu*` absent from `Cargo.lock`) and a bundled dictionary is a multi-MB, separately-licensed asset. `linebreak/rules.rs::sea_seam` documents the grapheme-fill/transition contract instead. Re-open only if the lead accepts `icu_segmenter` + provider blob. |

`cjk-spc-justify` 1–4 and `tab-stop-spc` 1–3 assert Canvas `ctx.letterSpacing` strings: **reclassified EXACT→BEHAVIOURAL** (not deleted) with the T3 geometry restatement.

## 13. File ownership for parallel work

| gap | exclusive files | shared file (region) |
| :--- | :--- | :--- |
| T1 (*asset* `assets/fonts/office-face-metrics.json`) | `text/faces.rs`*, `text/power_point.rs`*, `text/metrics.rs` | `text/emit.rs` = `StyleDef`/`Measurer` |
| T2 (*asset* `assets/text/east-asian-defaults.json`) | `text/slots.rs`*, `text/east_asian_default.rs`*, `text/paragraphs.rs` | — |
| T3 | `text/linebreak.rs`, `text/tabs.rs`, `text/paint.rs` | `text/emit.rs` = line loop |
| T4 | `text/bidi.rs`* | `text/paint.rs`, `text/emit.rs` (after T3) |
| T5/T6 | `text/preflight.rs`* | `text/linebreak.rs` (after T3), `text/emit.rs` (1 line) |
| T7 | `contracts/text.rs`, `rasterizer/text.rs`, `svg/text.rs` | `text/paint.rs` |
| T8/T9 | `text/vertical/*` (split first), `text/stacked_layout.rs`*, `text/stacked.rs` | — |
| T10 | `compiler/text_warp/lines.rs`*, `svg/text.rs` | `compiler/text.rs` (warp half), `text/shape.rs` |

`*` = new file. Region ownership on `text/emit.rs`/`text/paint.rs`: T1 and T3 merge first, then T4/T6; `text/vertical.rs` (598 lines) must be split before T8.

## 14. First three agents to launch

1. **`agent-text-faces` (T1)** — `src/text/{faces,power_point,metrics}.rs`, `src/text/emit.rs`
   (`StyleDef`/`Measurer` region), `assets/fonts/office-face-metrics.json`, `tests/parity_text_box_a_tests.rs`.
   Largest count (54), zero deps, and it freezes the `StyleDef`/`FaceMetricsFull` contract every other agent reads.
2. **`agent-text-spc` (T3 + T4)** — `src/text/{linebreak,tabs,paint,bidi}.rs`, `src/text/emit.rs` (line
   loop), `Cargo.toml` (one `unicode-bidi` line, lead-approved), new `tests/parity_text_flow_a_spc_tests.rs`
   + `tests/parity_text_flow_b_bidi_tests.rs`, edits `tests/parity_text_flow_a_tests_wrap.rs`. 37 cases, no
   T1 dependency, biggest algorithm block.
3. **`agent-text-slots` (T2)** — `src/text/{slots,east_asian_default}.rs`, `src/text/paragraphs.rs`,
   `assets/text/east-asian-defaults.json`, `tests/parity_text_box_b_eavert_tests.rs`. 11 cases; needs only
   `faces::is_serif_latin_face` (code against the signature, stub until agent 1 merges).

Wave 2: `agent-text-autofit` (T5+T6), `agent-text-stacked` (T7+T8 → T9), `agent-text-warp` (T10).
