# Parity coverage — batch `misc`

Reference: `/tmp/oooxml/packages/pptx/src/*.test.ts` @ `1f5c6e1d` (read-only).
Rust targets: `crates/kitty-pptx-core/tests/parity_misc_tests.rs` and
`crates/kitty-pptx-core/tests/parity_parse_error_tests.rs` (split at the 600-line limit).
Verification: `CARGO_TARGET_DIR=target/agent-struct-core cargo test -p kitty-pptx-core
--test parity_misc_tests --test parity_parse_error_tests` → **15 tests, 15
passing**; `cargo clippy -p kitty-pptx-core --lib -- -D warnings` clean.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
| :--- | --: | --: | --: | --: | --: | :--- | :--- |
| dim-overlay.test.ts | 1 | 0 | 1 | 0 | 0 | `dim_overlay_paints_one_full_slide_scrim_last` | BEHAVIOURAL: `apply_dim_overlay(&mut slide, {color: #ffffff, opacity: .6})` appends exactly one `DrawPath` covering `(0,0,w_emu,h_emu)` with `Solid([255,255,255,153])` (`0.6×255`) as the last command (ref renderer.ts:8110-8122) |
| export-completeness.test.ts | 1 | 0 | 0 | 1 | 0 | — | N/A: `ts.createProgram` walk of the `index.ts` barrel; Rust has no per-file export surface (module graph is compiler-enforced) |
| renderer.parse-error.test.ts | 7 | 0 | 6 | 1 | 0 | `parity_parse_error_tests` (`broken_slide_paints_a_part_tagged_placeholder`, `healthy_slide_never_paints_the_parse_error_placeholder`, `builtin_text_is_kept_when_the_option_is_absent_or_returns_none`, `custom_title_replaces_the_heading_and_detail_none_hides_the_part`, `error_placeholder_closure_receives_the_slide_failure`, `healthy_slide_never_evaluates_the_error_placeholder`) | BEHAVIOURAL: `Slide::parse_error` short-circuits to the five-group card before background/elements (ref renderer.ts:8188-8274, 8477-8487); `CompileOptions::error_placeholder` overrides the heading and can hide the detail; the resolver sees `PartFailure{kind:"slide", slide_index, slide_number, message}` and is never called for a healthy slide. N/A: `errorPlaceholder: () => ({title: 1})` TypeError is unrepresentable with typed options |
| scene3d-transform-order.test.ts | 1 | 0 | 1 | 0 | 0 | `scene3d_keeps_rotation_outside_the_local_flip` | `splitScene3dShapeTransform(25,true,false)` is the identity split; asserted via `compute_shape_transform(0,0,1000,1000,25,true,false)` = R(25°)·diag(−1,1): sx=−cos25, sy=cos25, kx=ky=−sin25, det=−1 |
| smartart-fallback-contrast.test.ts | 19 | 1 | 3 | 0 | 15 | `smartart_fallback_keeps_the_theme_default_on_a_light_background`, `dark_background_does_not_touch_ordinary_shapes`, `parsed_slide_without_an_authored_colour_paints_hard_black`, `fallback_shape_keeps_an_explicit_run_colour` | EXACT: `ED7D31` run colour → `[0xED,0x7D,0x31,255]`. BLOCKED: `background_luminance(&Fill) -> Option<f32>`, `smart_art_fallback_text_color`, `is_smart_art_fallback_shape`, run identity (`PositionedGlyphRun::{shape_id, element_index, origin}`), slide-theme/caller `defaultTextColor` fallback, hyperlink colouring (`hyperlink_uses_text_fill`, `hlinkColor`) |
| wordart-resource-bounds.test.ts | 1 | 0 | 1 | 0 | 0 | `wordart_empty_manual_lines_do_not_build_per_line_envelopes`, `wordart_painted_lines_share_the_same_envelope` | 100 empty manual lines: one `DrawText`, one glyph run, exactly 1 distinct `Arc<WarpEnvelope>` (reference bound ≤ 2 `buildWarpEnvelope` calls). Second test pins envelope sharing across 3 painted lines |
| model-source-text-colour.test.ts | 2 | 0 | 1 | 1 | 0 | `parsed_text_colour_chain_ignores_the_theme_default` | 'parser' variant ported (theme `FFFFFF` ignored → `000000`). N/A: caller-supplied `ModelSource` plugin does not exist (no worker/plugin loader) |
| resource-policy-wiring.test.ts | 1 | 0 | 0 | 1 | 0 | — | N/A: `Worker` bridge `postMessage` request shape (`kind:'parse'`, `resourcePolicy`) — the crate parses in-process |
| progressive-preflight-gate.test.ts | 2 | 0 | 0 | 2 | 0 | — | N/A: Promise-based host-acknowledgement gate; no progressive worker transport in the crate |
| renderer-destroy-invalidation.test.ts | 1 | 0 | 0 | 1 | 0 | — | N/A: `ImageBitmap` decode + `invalidatePptxRenderTarget(canvas)` mount lifecycle; the rasterizer is synchronous, no caller-owned canvas |
| selection-context.test.ts | 1 | 0 | 0 | 1 | 0 | — | N/A: DOM `Selection`/`Range`/`HTMLElement.dataset` locator extraction |

**Totals:** 37 cases — EXACT 1, BEHAVIOURAL 13, N/A 8, BLOCKED 15; 14 cases
ported as 15 passing Rust tests (9 in `parity_misc_tests`, 6 in
`parity_parse_error_tests`).

Files created: `crates/kitty-pptx-core/tests/parity_misc_tests.rs`,
`tests/parity/coverage/misc.md`. No production code touched.

Missing-symbol summary for follow-up implementation work:
~~`apply_dim_overlay`~~ (landed), ~~`Slide::parse_error` placeholder emission +
`CompileOptions::error_placeholder`~~ (landed), `background_luminance`, `smart_art_fallback_text_color`,
`is_smart_art_fallback_shape`, text-run identity (`shape_id`/`element_index`/`origin`),
slide-theme + caller `defaultTextColor` resolution in `compile_text_body`, hyperlink text colouring
(`hlinkClr="tx"`).
