# External renderer comparison (opt-in, perceptual)

An independent check that `kitty-pptx` still produces the right structure when
another implementation draws the same LibreOffice QA deck. This is **not**
PowerPoint ground truth and **not** a pixel-identity test: fonts come from the
host, so glyphs and anti-aliasing differ between renderers.

Provenance:

- Fixtures: [LibreOffice core](https://github.com/LibreOffice/core), **MPL-2.0**,
  fetched at test time from the commit pinned in `fixtures.json`. They are not
  distributed with this repository.
- References: [`@silurus/ooxml`](https://github.com/yukiyokotani/office-open-xml-viewer),
  **MIT**, driven through Playwright (Apache-2.0) in headless Chromium.

## One command

```sh
python3 crates/kitty-pptx-core/tests/golden/external/run.py
```

The first run needs network access, Git, Node.js/npm, and downloads Playwright
Chromium. It fetches the pinned fixtures, renders the references, and runs the
comparison. Later runs reuse the cache and rerun only the Rust comparison.

Useful flags:

```sh
python3 .../run.py --force          # rebuild both caches
python3 .../run.py --force-fetch    # refetch fixtures, reuse references
python3 .../run.py --force-render   # rerender references, reuse fixtures
python3 .../run.py --skip-test      # only populate caches
python3 .../run.py --offline        # use existing caches only
```

## Cache layout and stamp

```text
target/external/
├── fixtures/<commit>/
│   ├── Bar_List.pptx
│   └── ...
└── goldens/<commit>/
    ├── stamp.json
    ├── Bar_List/slide_001.png
    └── ...
```

`fetch_fixtures.py` performs a deliberately narrow partial clone of LibreOffice
core: `--depth=1 --filter=tree:0` plus a non-cone sparse checkout of exactly the
manifest paths, then deletes the temporary clone. Every file is SHA-256
verified. `viewer/render.mjs` serves the viewer and fixtures locally and writes
one PNG per slide.

`stamp.json` records the manifest SHA-256, pinned commit, render width,
per-fixture name/sha256/slide count, renderer script hashes, and pinned
`@silurus/ooxml`/Playwright versions. `run.py` rerenders when any of it
changes. The Rust test only requires the stamped commit, width, and fixture
identities (name, sha256, slides) to match `fixtures.json`, so editing a
threshold or note does not invalidate cached pixels.

`ExternalCorpus::discover()` reads `fixtures.json` and derives the commit and
cache paths from it; nothing is hard-coded in the test, so adding a manifest
entry needs no test edit. `KITTY_PPTX_EXTERNAL_FIXTURES` and
`KITTY_PPTX_EXTERNAL_GOLDENS` override the two cache directories.

## Skip vs run

The comparison is a normal `#[test]` (not `#[ignore]`) that self-skips:

- Missing, incomplete, or stale cache: prints one line such as
  `skipping external corpus comparison: ...; run python3 .../run.py` and
  returns. `cargo test` passes on a machine that never opted in.
- Complete cache and matching stamp: renders every manifest fixture through the
  production pipeline and compares it against the cached references.

`run.py` still invokes `cargo test ... -- --ignored`, so the file keeps an
ignored alias test for that filter. The alias can be removed once `run.py`
passes `--include-ignored`.

## Thresholds

Actual and reference images are resized to 400 px wide with box averaging and
split into 10x10 px cells. Defaults, overridable per fixture via `thresholds`:

| key | default | meaning |
| --- | --- | --- |
| `min_cell_pass_ratio` | `0.90` | fraction of cells whose mean per-channel difference is at most 40/255 |
| `max_global_mean_abs_diff` | `8.0` | mean per-cell per-channel difference, in 0-255 units |
| `max_ink_fraction_delta` | `0.03` | absolute difference in non-white pixel fraction (3 percentage points) |

`strict: false` prints the metrics as a diagnostic but never fails the test.
The current diagnostic fixture is `3columns` (known column and inter-word
spacing gap). The other four entries are enforced. If a budget must change,
regenerate references with `--force-render` and review the metric drift first.

`manifest_is_internally_consistent` validates the manifest without the cache:
unique safe names, 40/64-character lowercase hex commit and SHA-256 values,
slide counts greater than zero, LibreOffice QA paths, non-empty notes, sane
thresholds, and at least one enforced fixture.

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

Then run:

```sh
python3 .../run.py --force-fetch --force-render
```

The comparison test picks the entry up automatically. Do not add fixture
binaries or generated reference PNGs to git.
