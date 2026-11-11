# External renderer comparison (opt-in)

An expensive, locally-run comparison against a second PPTX renderer:
[`office-open-xml-viewer`](https://github.com/yukiyokotani/office-open-xml-viewer)
driven through Playwright. It is **not** run by `cargo test` or CI.

The point is not pixel identity or authority. It is an independent check that
our renderer still produces the right structure when another implementation
draws the same LibreOffice QA deck.

## One command

```sh
python3 crates/kitty-pptx-core/tests/golden/external/run.py
```

The first run needs network access, Git, Node.js/npm, and downloads Playwright
Chromium. Later runs use the cache under `target/external/` and only rerun the
Rust comparison.

Useful flags:

```sh
# Rebuild both caches
python3 .../run.py --force

# Refetch fixtures but reuse rendered references
python3 .../run.py --force-fetch

# Re-render references but reuse fixtures
python3 .../run.py --force-render

# Use existing caches only
python3 .../run.py --offline
```

## What it does

1. **Fetch pinned fixtures** — `fetch_fixtures.py` performs a deliberately
   narrow partial clone of `https://github.com/LibreOffice/core`:
   `--depth=1 --filter=tree:0` plus non-cone sparse checkout for exactly the
   paths in `fixtures.json`. Only those blobs are downloaded; the temporary
   clone is deleted. Files are SHA-256 verified and cached at
   `target/external/fixtures/<commit>/`.
2. **Render references** — `viewer/render.mjs` serves the viewer and fixtures
   locally, loads each deck in headless Chromium through Playwright, and writes
   PNGs to `target/external/goldens/<commit>/`. Reference PNGs are not checked
   in.
3. **Compare** — `cargo test -p kitty-pptx-core --test external_fixture_tests
   -- --ignored --nocapture` renders the same fixtures through the production
   pipeline and compares coarse colour cells, global RGB difference, and ink
   coverage.

A stamp in `target/external/goldens/<commit>/stamp.json` records the manifest,
renderer scripts, fixture hashes, and pinned viewer/Playwright versions. If it
matches, rendering is skipped.

## Licensing

The fixtures are fetched at test time from LibreOffice core at the pinned
commit and are **not distributed with this repository**. LibreOffice core is
MPL-2.0; the manifest records the upstream URL, commit, and per-file SHA-256.
The viewer (`@silurus/ooxml`) is MIT; Playwright is Apache-2.0. Do not add
downloaded fixture binaries or generated reference PNGs to git.

## Determinism and scope

- `@silurus/ooxml`, Playwright, and the renderer scripts are pinned. Changing
  any of them invalidates the reference stamp.
- Fonts still come from the host. The comparison is perceptual and tolerant;
  regenerate references when changing fonts or the viewer.
- The viewer is an independent implementation, not PowerPoint ground truth. A
  failure means "these two renderers disagree enough to inspect", not
  necessarily "we are wrong".
- The tier is not a CI gate.

## Adding a fixture

Add an entry to `fixtures.json`:

```json
{
  "name": "stable-local-stem",
  "path": "sd/qa/unit/data/pptx/file.pptx",
  "sha256": "<sha256 of the pinned blob>",
  "slides": 1,
  "strict": true,
  "note": "what this covers",
  "thresholds": {
    "min_cell_pass_ratio": 0.9,
    "max_global_mean_abs_diff": 8.0,
    "max_ink_fraction_delta": 0.03
  }
}
```

Then rebuild:

```sh
python3 .../run.py --force-fetch --force-render
```

`strict: false` keeps a fixture as a printed diagnostic without failing the
test. The current `3columns` entry uses that for a known column/inter-word
spacing gap.
