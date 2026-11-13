# Parity coverage — batch `fonts`

Reference: `/tmp/oooxml/packages/pptx/src/*.test.ts` @ `1f5c6e1d` (read-only).
Rust targets (split to keep both under the 600-line limit):

- `parity_fonts_tests.rs` — resolved family strings, the theme/paragraph/run
  cascade, embedded-font slots, family serialization.
- `parity_fonts_slot_tests.rs` — `a:ea`/`a:cs` per-cluster slot dispatch and
  PowerPoint's application-default East Asian / complex-script faces.

Verification: `CARGO_TARGET_DIR=target/agent-text-b cargo test -p kitty-pptx-core
--test parity_fonts_tests --test parity_fonts_slot_tests` → **22 tests, 22
passing**; `cargo clippy -p kitty-pptx-core --lib --tests -- -D warnings` clean.

## Why most cases are N/A

Every spec in this batch decides families while building **CSS font strings**
(`cssFontStack` / `buildFont`) or while loading DOM `FontFace` objects, both
consumed by a Canvas painter. The port has neither: a `PositionedGlyphRun`
carries exactly one concrete family (`resolve_font_family` in
`src/compiler/text.rs`, then `fontdb` in `src/rasterizer/text.rs`), the
rasteriser is `tiny-skia`, and the SVG writer emits that same string. Cases
whose reference API has no port counterpart are N/A below with the reason key;
the portable remainder is asserted on the compiled display list, the SVG export
of the same run, or the parser's `p:embeddedFontLst` projection.

Reason keys: **R1** CSS stack (`cssFontStack`) absent · **R2** script/regional
fallback chains (`cjkFallback` sc/tc/jp/kr, Arabic/Hebrew/Thai/Devanagari Noto
tails, per-script substitute scope) absent · **R3** numeric OpenType weight /
`font` shorthand absent (port has `bold: bool`) · **R4** pinned faces, aliases,
`FontFace`/`FontFaceSet` routes absent · **R5** preload/request/demand collector
(`PptxFontPreloadAccumulator`, `pptxSlideOfficeFontRequests`,
`collectSlideFontDemand`, `PresentationPreflightBuilder`) absent · **R6**
`loadEmbeddedFonts` FontFace session / alias+tuple registry absent · **R7**
`PptxPresentation.load` options (`useGoogleFonts`, `fonts`) absent · **R8** chart
face/substitution model unused · **R9** native error path aborts (see below) ·
**R10** installed-face catalogue (`findReferenceFontMetrics`: PANOSE serif class,
OS/2 Far-East code page) absent off-host — see divergence 7.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| font-stack.test.ts | 36 | 0 | 5 | 31 | 0 | `cambria_degrades_to_the_serif_substitute`, `calibri_stays_sans_and_times_new_roman_stays_serif`, `measurement_paint_and_export_share_one_resolved_family`, `unnamed_families_stay_regular`, `explicit_bold_run_overrides_a_named_medium_face` | N/A R1/R2/R3/R4: Arabic chain (1), CJK Noto ordering (8), non-CJK script tails (2), `buildFont` CSS/pinned/alias/embedded cases (11), weight-name table (9), per-presentation neutral fallback (1), Arabic-before-CJK (1). Divergences recorded below: cases `does not substitute distinct light and mathematical faces`, `builds the aliased family stack from a trimmed native name`, and the `700` half of `keeps unnamed families regular and recognizes hyphenated bold names` cannot hold in the port |
| font-requests.test.ts | 11 | 0 | 4 | 7 | 0 | `resolves_theme_paragraph_and_run_faces`, `keeps_slide_local_paragraph_families`, `authored_cs_face_reaches_the_painted_run` | BEHAVIOURAL: theme `+mn-lt` → `Carlito`, `a:defRPr` → paragraph face, run `a:latin` override → `Liberation Sans`; slide-local paragraph face survives. `retains authored CS faces in shape/table preload names` (×2) is now asserted where the name is actually used — the painted `a:cs` family (`Carlito`) on both the shape and the table path (`compile_table_element`); the reference asserts a preload *name set*, a surface the port does not have (R5). N/A R5: accumulator + request-list cases (5), `eq` math-preview cases (2) |
| font-family-serialization.test.ts | 3 | 3 | 0 | 0 | 0 | `preserves_quotes_in_the_authored_family`, `preserves_a_literal_backslash_in_the_authored_family`, `keeps_replacement_tokens_literal` | Inputs and expected outputs are the spec's own strings (`Face "Quoted"`, `Face \31 "Quoted"`, `Default $&`); the port stores the family verbatim, so equality on `PositionedGlyphRun::font_family` is the same assertion the spec makes after `JSON.parse`. The application-default *tail* (`'Default "One", Two'`, `Default \2`) is R1 |
| absent-face-routes.test.ts | 10 | 0 | 0 | 10 | 0 | — | N/A R4/R5: `acquireOfficeRoutes`, `FontFace` probing, `officeFaceWeight` (`Calibri Light` → 300), route/presence caches, `__proto__`-named families, application-FontFace shadowing, exact `local()` alias. The measure/paint-single-family half of `uses a non-Calibri exact private alias…` is covered by font-stack's `measurement_paint_and_export_share_one_resolved_family` |
| embedded-fonts.test.ts | 9 | 0 | 2 | 6 | 1 | `maps_all_four_embedded_font_slots`, `keeps_raw_font_bytes_and_skips_an_unreadable_part` | BEHAVIOURAL: `p:embeddedFontLst` projects all four slots (regular/bold/italic/boldItalic, one family, exact part paths, `application/x-font-ttf`); a ref whose Font part is missing is dropped while its sibling survives and the retained bytes are byte-exact (`extract_font`). N/A R6: fetch-skip, 2-way concurrency, per-presentation alias isolation, substitute ordering, failed-face fallback, tuple registry. BLOCKED (`preserves other typed resource-limit errors`): R9 |
| embedded-fonts-load.test.ts | 3 | 0 | 0 | 0 | 3 | — | BLOCKED R9: the case needs `PptxPresentation.load` to *reject* with a typed `OoxmlResourceLimitError` (`limit 4096`, `observed 4097`) and to release registered faces. Native `pptx_parser::extract_font` / `PptxArchive::new` report failures as `JsValue::from_str`, which panics with `function not implemented on non-wasm32 targets`, so a rejected load aborts the test process instead of returning `Err` |
| font-demand-coverage.test.ts | 2 | 0 | 0 | 2 | 0 | — | N/A R5: `collectSlideFontDemand` + `renderSlideWithEmbeddedFonts`; the superset invariant over measured/painted scalars has no port analogue (and `cap="all"`/`cap="small"` are not applied by `text::emit`) |
| font-preload-demand.test.ts | 10 | 0 | 0 | 10 | 0 | — | N/A R5 (+R2 for the cjk `'all'` declines) |
| font-source-load.test.ts | 6 | 0 | 0 | 6 | 0 | — | N/A R4/R5/R6: `pptxFontDemand`, `FontSourceSession`, `PptxInstalledFamilies`, 33-family probe cap |
| cjk-fallback-main-mode.test.ts | 1 | 0 | 0 | 1 | 0 | — | N/A R2: `cjkFallback: 'sc'` → `['Noto Sans SC', 'Noto Serif SC']` preload names; no regional Han fallback in the port |
| chart-font-substitution.test.ts | 2 | 0 | 0 | 2 | 0 | — | N/A R8: `src/compiler/charts/{bands,layout}.rs` paint every chart string in `CHART_FONT = "Carlito"` with `bold: false`; `ChartModel.titleFontFace`/`legendFontFace`/`catAxisFontFace` are never read and there is no substitution map |
| presentation-font-options.test.ts | 3 | 0 | 0 | 3 | 0 | — | N/A R7: `useGoogleFonts` normalisation, deprecation warning, application `fonts` sources — the port has no font-source option |
| east-asian-default.test.ts ¹ | 8 | 6 | 2 | 0 | 0 | `parity_fonts_slot_tests`: `east_asian_default_faces`, `is_serif_latin_face`, and the `a:ea`/`a:cs` slot dispatch | Moved here from the `text-box-b` ledger when `text::east_asian_default` + `slots.rs` landed. The default-face table (`+mn-ea` → PowerPoint's application default per script), serif detection for the Latin tail and the empty-slot fallback are asserted; the two BEHAVIOURAL rows observe the face through the painted run because R10 has no installed-face catalogue. |

¹ Not a `packages/pptx/src` row of its own in the batch's original 12-file scan: it
was enumerated under `text-box-b` and reassigned here, so the table sums to the
**Totals** line below (96 + 8 = 104). No case is counted in two batches.

**Totals:** 104 cases — EXACT 9, BEHAVIOURAL 13, N/A 78, BLOCKED 4; 24 cases
ported as 22 passing Rust tests (12 in `parity_fonts_tests.rs`, 10 in
`parity_fonts_slot_tests.rs`).

## Divergences found while porting (production behaviour, not fixed here)

1. `resolve_font_family` (`compiler/text.rs:504`) substring-matches, so
   `Calibri Light` → `Carlito` and `Cambria Math` → `Caladea`; the reference
   deliberately leaves both unsubstituted (font-stack `does not substitute
   distinct light and mathematical faces`).
2. The same function does not trim the authored name, while the reference
   builds the stack from a trimmed native name (font-stack `builds the aliased
   family stack from a trimmed native name`).
3. *Fixed in this pass*: `a:ea` / `a:cs` are now consumed per grapheme cluster
   (`src/text/slots.rs`, ref `renderer.ts:1930-1985`): a CJK or East-Asian-slot
   symbol cluster takes `a:ea` (or PowerPoint's application default when the slot
   is empty), an RTL/Indic cluster takes `a:cs` (or the per-script default), the
   rest stays on `a:latin`. `a:sym` is still unread.
4. Bullet and auto-number labels hardcode `"Carlito"` (`text/emit.rs:345`)
   instead of the resolved marker face (font-requests `requests the face
   actually used by character and auto-number markers`).
5. Charts ignore `titleFontFace`/axis faces (see R8).
6. `cap="all"` / `cap="small"` are not applied, so capital-expanded scalars are
   never painted (font-demand-coverage, font-preload-demand).
7. `is_serif_latin_face` / `is_east_asian_face` (`src/text/east_asian_default.rs`)
   are answered from an authored subset of the reference's bundled `office-mac`
   face catalogue (PANOSE serif class + OS/2 Far-East code page) rather than from
   the installed face: `ttf-parser` exposes neither `OS/2.panose` nor the code
   page ranges, and the container installs no Office face. An unlisted face is
   sans, exactly the reference's "unknown faces are sans" (R10).

## Missing-symbol summary for follow-up work

~~`TextRunData::font_family_cs`/`font_family_ea` consumed by `text::emit`~~ (landed:
`text::slots` + `text::east_asian_default`) · `a:sym` symbol-font mapping ·
marker-face resolution for `bullet_label` ·
`ChartModel` face plumbing + `fontSubstitutions` · native (`String`-error)
return from `pptx_parser::{extract_font, PptxArchive}` so rejected loads are
assertable off-wasm · a demand/preflight collector and a regional CJK fallback
(`cjkFallback`) if the preload specs are ever in scope.

Files created: `crates/kitty-pptx-core/tests/parity_fonts_tests.rs`,
`crates/kitty-pptx-core/tests/parity_fonts_slot_tests.rs`, `tests/parity/coverage/fonts.md`.
Production added in the slot pass: `src/text/{slots,east_asian_default,east_asian_coverage}.rs`
and the EA/CS dispatch in `src/text/paragraphs.rs` (`SlotFaces::resolve` +
`slots::push_run_items`).
