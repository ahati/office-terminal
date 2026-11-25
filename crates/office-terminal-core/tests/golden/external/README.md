# External renderer comparison (opt-in, perceptual)

An independent check that `Office Terminal` still produces the right structure when
another implementation draws the same LibreOffice QA deck. This is **not**
PowerPoint ground truth and **not** a pixel-identity test: fonts come from the
host, so glyphs and anti-aliasing differ between renderers.

Provenance:

- Fixtures: [LibreOffice core](https://github.com/LibreOffice/core), **MPL-2.0**,
  fetched at test time from the commit pinned in `fixtures.json`. They are not
  distributed with this repository.
- References: [`@silurus/ooxml`](https://github.com/yukiyokotani/office-open-xml-viewer),
  **MIT**, driven through Playwright (Apache-2.0) in headless Chromium.

## The corpus: all 419 decks

`fixtures.json` is generated from the whole pinned directory
`sd/qa/unit/data/pptx`, not a hand-picked subset:

| quantity | value |
| --- | --- |
| upstream `.pptx` files | 419 |
| renderable manifest entries | 419 (0 skipped by the generator) |
| slides | 545 |
| fixture bytes | 18,459,826 |
| manifest size | ~138 KB |
| enforced (`strict: true`) | 4 |
| diagnostic (`strict: false`) | 415 |
| rejected by the reference viewer | 1 |

Every entry is `{name, path, sha256, slides, strict, note}`; known-bad files
would additionally appear in a top-level `skipped` array with a reason (0 at
this commit). The four strict fixtures are the original curated decks
(`Bar_List`, `ShapePlusImage`, `ShapeTextInflateTop`, `chart_pt_color_bg1`);
`3columns` stays diagnostic.

### Why most entries are diagnostic

LibreOffice's `sd/qa/unit/data/pptx` directory is a regression corpus, not a
visual test suite. It mixes feature samples with fuzz/bnc/tdf crash samples,
deliberately malformed packages, and decks whose fonts and chart text metrics
depend on the host. A second renderer will differ perceptually even when both
are correct. Hard-gating all 419 would make the tier flaky and meaningless, so:

- **strict** fixtures (stable geometry, text, fills, charts) enforce thresholds
  and fail the test;
- **diagnostic** fixtures still render every slide and print the full metric
  line, but never fail. A human reviews drift when the diagnostic numbers move.

One deck, `tdf148478_transitionMusic`, is present in the manifest but cannot be
rendered by the reference viewer: it contains a non-ASCII ZIP member
(`ppt/media/Cortázar.wav`), which the OOXML/OPC spec forbids. `run.py` records
it in `unrenderable.json` and the test reports it as a missing golden.

### Promoting a fixture to strict

1. Give it references: `python3 .../run.py --skip-test` (or `--limit N` while
   probing), then run the comparison with `--nocapture` and read its metrics.
2. Confirm the deck is stable across rerenders: run `--force-render` once and
   compare the metric line before/after. Font or Chromium changes move it.
3. Add `"strict": true` and, if needed, explicit `thresholds`:

```json
{
  "name": "tdf12345",
  "path": "sd/qa/unit/data/pptx/tdf12345.pptx",
  "sha256": "<64 hex>",
  "slides": 2,
  "strict": true,
  "note": "what the deck covers",
  "thresholds": {
    "min_cell_pass_ratio": 0.9,
    "max_global_mean_abs_diff": 8.0,
    "max_ink_fraction_delta": 0.03
  }
}
```

4. Rerun the test and leave headroom: a strict fixture that only barely passes
   will fail on the next Chromium bump.

Regenerating the manifest preserves `strict`, `thresholds`, and `note` for
existing names, so promotion survives `generate_manifest.py`.

## Commands

```sh
# regenerate fixtures.json from the pinned commit (downloads once, ~2-3 min)
python3 crates/office-terminal-core/tests/golden/external/generate_manifest.py

# one command: fetch + render references + run the comparison
python3 crates/office-terminal-core/tests/golden/external/run.py

# smoke a batch of decks (partial cache; skips cargo test by design)
python3 crates/office-terminal-core/tests/golden/external/run.py --limit 25 --skip-test

# use existing caches only (no npm, no Chromium download)
python3 crates/office-terminal-core/tests/golden/external/run.py --offline

# rebuild selectively
python3 .../run.py --force-fetch     # refetch fixtures, reuse references
python3 .../run.py --force-render    # discard and rerender all references
python3 .../run.py --skip-test       # only populate caches

# run the comparison directly (self-skips when the cache is absent)
cargo test -p office-terminal-core --test external_fixture_tests -- --nocapture
```

`generate_manifest.py` enumerates the directory through the GitHub contents API
(falling back to the git tree API if a listing ever exceeds 1000 entries),
downloads each raw blob once into `target/external/download/<commit>/`, hashes
it, and counts `ppt/slides/slide*.xml` members with `zipfile`. Output is sorted
2-space JSON with a trailing newline, so regeneration is byte-identical.
`--jobs`, `--refresh`, `--commit`, `--out`, and `--download-dir` are available.

`fetch_fixtures.py` performs a deliberately narrow partial clone of LibreOffice
core: `--depth=1 --filter=tree:0` plus a non-cone sparse checkout of exactly the
manifest paths (419 entries), then deletes the temporary clone. Every file is
SHA-256 verified; existing cache files are kept, so `--limit N` smoke runs never
evict a full corpus. `--force` re-verifies and overwrites the selected entries.

## Cache layout

```text
target/external/
├── download/<commit>/*.pptx        # generator's raw download cache
├── fixtures/<commit>/*.pptx        # sparse-checkout fetch (419)
└── goldens/
    ├── <commit>/
    │   ├── stamp.json              # written only after a full render
    │   ├── unrenderable.json       # decks the reference viewer rejected
    │   ├── Bar_List/slide_001.png
    │   └── ...                     # 418 deck directories, 543 PNGs
    └── thumbnails/<commit>/v1-w400/
        └── <deck>/slide_001.<len>-<mtime>.png   # generated on first compare
```

`stamp.json` records the manifest SHA-256, pinned commit, render width,
per-fixture name/sha256/slide count, renderer script hashes, and pinned
`@silurus/ooxml`/Playwright versions. Any renderer, width, or commit change
invalidates the whole reference cache; adding fixtures keeps existing pixels and
renders only the new decks. Editing a note or threshold does **not** invalidate
cached pixels.

The comparison metric runs at 400 px, so the first comparison flattens and
box-downscales each 1600 px reference into `goldens/thumbnails/<commit>/`
(git-ignored with the rest of `target/`). Later runs decode those thumbnails
instead of the full-resolution PNGs. The thumbnail name includes the source
file's length and mtime, so `--force-render` regenerates only the changed
references; bumping `COMPARE_WIDTH` starts a new `w<width>` directory. The
diagnostic score baseline records the width it was recorded at, so changing the
width skips the ratchet until the baseline is regenerated with
`UPDATE_EXTERNAL_SCORES=1`.

Rendering is resumable at deck granularity: a deck whose `slide_*.png` files all
exist is skipped, and freshly rendered slides are merged one file at a time so
an existing non-empty `slide-*.png` is never overwritten. The viewer renders all
slides of a deck in one page, so a partially rendered deck is rerendered.

`run.py` batches decks (32 at a time) and bisects a failed batch, so one bad
deck cannot abort the corpus; failures are recorded in `unrenderable.json` and
not retried (delete the file or use `--force-render` to retry). It prints the
measured rate and an extrapolated full-corpus time. At the pinned commit a full
render measured **~184 decks/min (~2.3 minutes for 419 decks)**; the comparison
test renders at 400 px and caches the downscaled references, so only the first
debug run pays the full-resolution decode.

`ExternalCorpus::discover()` reads `fixtures.json` and derives the commit and
cache paths from it, so adding a manifest entry needs no test edit.
`OT_EXTERNAL_FIXTURES` and `OT_EXTERNAL_GOLDENS` (legacy `KITTY_PPTX_*`) override the
two cache directories.

## What the test does

The comparison is a normal `#[test]` (not `#[ignore]`) that self-skips:

- Missing fixtures, missing references, a stale stamp, or no golden PNGs at all:
  prints `skipping external corpus comparison: ...` plus the run command and
  returns. `cargo test` passes on a machine that never opted in, in milliseconds.
- Cached references present: renders every manifest deck that has a complete
  golden set through the production pipeline at the 400 px comparison width
  (parallel workers), compares each slide against a cached reference thumbnail,
  and prints a summary:

```text
external corpus summary: compared=418 slides=543 strict=4 (pass=4 fail=0) \
diagnostics=414 missing_goldens=1 missing_fixtures=0 errors=0 \
diagnostic_errors=0 known_unrenderable=1
```

The reference viewer is never invoked by the test, and missing goldens are
counted and listed, never silently ignored. The separate
`manifest_is_internally_consistent` test validates unique safe names, 40/64-char
lowercase hex values, `name.pptx` path agreement, slide counts, notes,
threshold ranges, skip reasons, and that at least one fixture is strict.

## Thresholds

Actual and reference images are resized to 400 px wide with box averaging and
split into 10x10 px cells. Defaults, overridable per fixture via `thresholds`:

| key | default | meaning |
| --- | --- | --- |
| `min_cell_pass_ratio` | `0.90` | fraction of cells whose mean per-channel difference is at most 40/255 |
| `max_global_mean_abs_diff` | `8.0` | mean per-cell per-channel difference, in 0-255 units |
| `max_ink_fraction_delta` | `0.03` | absolute difference in non-white pixel fraction (3 percentage points) |

If a strict budget must change, regenerate references with `--force-render` and
review the metric drift first. Do not add fixture binaries or generated PNGs to
git.
