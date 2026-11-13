# Parity coverage — batch `fonts`

Reference: `/tmp/oooxml/packages/pptx/src/*.test.ts` @ `1f5c6e1d` (read-only).
Rust target: `crates/kitty-pptx-core/tests/parity_fonts_tests.rs` (single file, 519 lines).
Verification: `CARGO_TARGET_DIR=target/agent-fonts cargo test -p kitty-pptx-core --test parity_fonts_tests`
→ **12 tests, 12 passing**; `cargo clippy -p kitty-pptx-core --test parity_fonts_tests -- -D warnings` clean.

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
face/substitution model unused · **R9** native error path aborts (see below).

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| font-stack.test.ts | 36 | 0 | 5 | 31 | 0 | `cambria_degrades_to_the_serif_substitute`, `calibri_stays_sans_and_times_new_roman_stays_serif`, `measurement_paint_and_export_share_one_resolved_family`, `unnamed_families_stay_regular`, `explicit_bold_run_overrides_a_named_medium_face` | N/A R1/R2/R3/R4: Arabic chain (1), CJK Noto ordering (8), non-CJK script tails (2), `buildFont` CSS/pinned/alias/embedded cases (11), weight-name table (9), per-presentation neutral fallback (1), Arabic-before-CJK (1). Divergences recorded below: cases `does not substitute distinct light and mathematical faces`, `builds the aliased family stack from a trimmed native name`, and the `700` half of `keeps unnamed families regular and recognizes hyphenated bold names` cannot hold in the port |
| font-requests.test.ts | 11 | 0 | 2 | 7 | 2 | `resolves_theme_paragraph_and_run_faces`, `keeps_slide_local_paragraph_families` | BEHAVIOURAL: theme `+mn-lt` → `Carlito`, `a:defRPr` → paragraph face, run `a:latin` override → `Liberation Sans`; slide-local paragraph face survives. BLOCKED (`retains authored CS faces in shape/table preload names`, ×2): `a:cs` is parsed into `TextRunData::font_family_cs` but `src/text/emit.rs` never consults it. N/A R5: accumulator + request-list cases (5), `eq` math-preview cases (2) |
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

**Totals:** 96 cases — EXACT 3, BEHAVIOURAL 9, N/A 78, BLOCKED 6; 12 cases
ported as 12 passing Rust tests.

## Divergences found while porting (production behaviour, not fixed here)

1. `resolve_font_family` (`compiler/text.rs:504`) substring-matches, so
   `Calibri Light` → `Carlito` and `Cambria Math` → `Caladea`; the reference
   deliberately leaves both unsubstituted (font-stack `does not substitute
   distinct light and mathematical faces`).
2. The same function does not trim the authored name, while the reference
   builds the stack from a trimmed native name (font-stack `builds the aliased
   family stack from a trimmed native name`).
3. `a:ea` / `a:cs` / `a:sym` are parsed (`pptx_model::TextRunData::font_family_*`)
   but never consulted: `text::emit::build_paragraphs` resolves only
   `font_family`, so CJK/Hebrew runs never reach the script slot.
4. Bullet and auto-number labels hardcode `"Carlito"` (`text/emit.rs:345`)
   instead of the resolved marker face (font-requests `requests the face
   actually used by character and auto-number markers`).
5. Charts ignore `titleFontFace`/axis faces (see R8).
6. `cap="all"` / `cap="small"` are not applied, so capital-expanded scalars are
   never painted (font-demand-coverage, font-preload-demand).

## Missing-symbol summary for follow-up work

`TextRunData::font_family_cs`/`font_family_ea` consumed by `text::emit`
(script-slot dispatch) · marker-face resolution for `bullet_label` ·
`ChartModel` face plumbing + `fontSubstitutions` · native (`String`-error)
return from `pptx_parser::{extract_font, PptxArchive}` so rejected loads are
assertable off-wasm · a demand/preflight collector and a regional CJK fallback
(`cjkFallback`) if the preload specs are ever in scope.

Files created: `crates/kitty-pptx-core/tests/parity_fonts_tests.rs`,
`tests/parity/coverage/fonts.md`. No production code touched.
