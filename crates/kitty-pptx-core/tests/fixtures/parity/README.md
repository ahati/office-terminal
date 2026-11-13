# Parity fixtures (`sample-1`)

Committed inputs for `tests/parity_fidelity_tests.rs`. This corpus is a **test
oracle**, not a distributable demo.

## Provenance

| Path | Origin |
| :--- | :--- |
| `sample-1.pptx` | Copied verbatim from the reference project (`/tmp/oooxml` @ `1f5c6e1d`, `packages/pptx/public/demo/sample-1.pptx`). 9 slides, 16:9. |
| `reference/sample-1/slide-1..9.png` | Adopted renderer outputs: the reference TypeScript renderer's own committed VRT references (`packages/pptx/tests/visual/references/demo/sample-1/`), byte-identical copies, 1280×720. |
| `reference/sample-1/scores.powerpoint.json` | Byte-identical copy of the reference project's committed `scores.json` for that deck. Context only; **not** this harness's ratchet baseline. |

Licence: the reference project is MIT-licensed, © Yuki Yokotani
(`https://github.com/yukiyokotani/office-open-xml-viewer`). The `.pptx`
authoring and the reference PNGs are covered by that notice to the extent the
reference project holds rights in them. This notice is carried along with the
copied files.

## Media caveat

`sample-1.pptx` embeds **8 JPEG files** (`ppt/media/image1.jpeg` …
`image8.jpeg`, ~1.9 MB total). They carry **no attribution, caption, EXIF
author, or licence statement**, and the reference project does not document
their origin. Treat them as third-party content that has **not** been cleared
for redistribution: they exist only so the renderer is exercised on real
photographic media. Do not ship this deck, or derived PNGs, as product
content.

## The reference PNGs are adopted renderer outputs, not Office exports

`reference/sample-1/slide-N.png` are **outputs of the reference TypeScript
renderer**, captured from its canvas by Playwright. The reference project's own
`AGENTS.md` states that its demo references are adopted renderer outputs, not
Word/Excel/PowerPoint exports. `scores.powerpoint.json` is that project's
match-% against a PowerPoint export of the same deck (95.54–98.62 %), carried
along for context only.

Consequence: the reference PNGs are a **drift oracle**, not a correctness
oracle. Matching them means "renders like the reference renderer", which is this
project's parity goal; it does not prove Office fidelity. Pixelmatch runs with
`threshold = 0.20, includeAA: true`, and both images are padded on white to a
common `max(w) × max(h)` size before comparison.

## Fidelity scores and modes

The ratchet baseline lives at
`crates/kitty-pptx-core/tests/parity/scores/sample-1.json`: one object keyed by
`slide-1` … `slide-9`, values rounded to 2 decimal places, keys sorted,
pretty-printed. A slide fails when:

- its pixel diff exceeds 20 % (`FAIL_ABOVE_DIFF_PCT`),
- no score is recorded for it, or
- its match-% drops more than 0.5 pt below the recorded score
  (`RATCHET_DROP_PCT`).

```sh
# Compare the current render against the adopted reference PNGs.
cargo test -p kitty-pptx-core --test parity_fidelity_tests -- --nocapture

# Adopt the current render as the reference PNG (requires explicit approval).
UPDATE_REFS=1 cargo test -p kitty-pptx-core --test parity_fidelity_tests

# Re-record match-% against the existing reference PNGs; never rewrites them.
UPDATE_SCORES=1 cargo test -p kitty-pptx-core --test parity_fidelity_tests
```

`UPDATE_REFS` intentionally does **not** update scores in the same run (it
short-circuits the comparison, mirroring the reference harness); follow it with
`UPDATE_SCORES=1` after a deliberate reference change, and review the resulting
score deltas. The two modes are mutually exclusive.

## Fonts

Text metrics depend on the installed font stack. The reference deck uses fonts
that are substituted when absent, which moves glyph edges and therefore the
score. Scores are recorded on the machine/font stack that produced them; a large
unexpected drop usually means a font substitution, not a renderer regression.
