# Coverage — parity batch `text-box-b`

Reference: `/tmp/oooxml` @ `1f5c6e1d`, `packages/pptx/src/*.test.ts`.
Rust targets: `crates/kitty-pptx-core/tests/parity_text_box_b*.rs`.

Status rule: a case is EXACT/BEHAVIOURAL only when **every** assertion of the
reference `it()` is ported and passing; if any assertion needs an absent API the
case is BLOCKED (the passing half is still pinned by that test's active body, and
each blocked test carries `#[ignore = "blocked: <symbol/reason>"]`).
Verify: `CARGO_TARGET_DIR=target/agent-text-box-b cargo test -p kitty-pptx-core
--test parity_text_box_b_tests` (+ `_bullet_image`, `_highlight`, `_eavert`,
`_stacked`, `_warp`). 136 test fns, 45 passing, 91 ignored, 0 failing;
`cargo clippy -p kitty-pptx-core --tests -- -D warnings` clean.

| reference file | cases | EXACT | BEHAVIOURAL | N/A | BLOCKED | Rust target | notes |
|---|---|---|---|---|---|---|---|
| empty-bullet.test.ts | 14 | 6 | 0 | 0 | 8 | parity_text_box_b_tests | marker run text = label. Blocked: no `paragraph_has_renderable_content`, no counter (index-based numbering, no skip/reset). |
| bullet-image.test.ts | 14 | 0 | 3 | 4 | 7 | parity_text_box_b_bullet_image_tests | `Bullet::Blip` never compiled → only "draws nothing"/"no image for char" observable; cache/worker/`<img>` are browser-only. |
| buszpts-bullet-size.test.ts | 4 | 1 | 0 | 0 | 3 | parity_text_box_b_tests | marker size ignores `size_pts`/`size_pct`; run-size fallback passes. |
| autonum-bullet-color.test.ts | 3 | 0 | 1 | 0 | 2 | parity_text_box_b_tests | inherited colour passes via shape default; `buClr` + authored face/% blocked. |
| rtl-bullet-indent.test.ts | 3 | 1 | 0 | 0 | 2 | parity_text_box_b_tests | LTR gutter control x=0 exact; RTL right-edge anchor absent (issue #930). |
| text-highlight.test.ts | 6 | 0 | 0 | 0 | 6 | parity_text_box_b_highlight_tests | no `highlight` on `PositionedGlyphRun`, no `paintHighlight`. |
| east-asian-default.test.ts | 8 | 0 | 0 | 0 | 8 | parity_text_box_b_eavert_tests | module absent; `font_family_ea` not consumed by `text::emit`. |
| eavert-glyph-orientation.test.ts | 23 | 14 | 3 | 2 | 4 | parity_text_box_b_eavert_tests | orientation/substitution + `dist` pitch exact; `vert` feature, FE14 semicolon, letter-spacing blocked; reflection N/A. |
| stacked-text.test.ts | 36 | 3 | 1 | 0 | 32 | parity_text_box_b_stacked_tests | one-grapheme-per-column reduction: no cell pitch, strokes, inline math, markers, blips. |
| wordart-follow-path.test.ts | 19 | 5 | 0 | 0 | 14 | parity_text_box_b_warp_tests | follow-path span/centre/ink box/clamp exact; multi-line envelope/lnSpc blocked. |
| line-end-shaft-trim.test.ts | 6 | 0 | 0 | 0 | 6 | parity_text_box_b_highlight_tests | `Stroke::{head,tail}_end`,`cmpd` never compiled. |
| display-equation-wrap.test.ts | 1 | 0 | 1 | 0 | 0 | parity_text_box_b_highlight_tests | display equation owns its line (1.2 em pitch). |
| **total** | **137** | **30** | **9** | **6** | **92** | | |
