# PRD 13a: Metafile Fidelity Matrix & Validation Plan

> Companion to [PRD 13](./13-metafile-pictures-and-vector-playback.md). This document is the normative **fidelity ledger**, **diagnostics taxonomy**, **corpus specification** and **test plan** for EMF/WMF/EMF+ playback.
> Test levels and diff metrics follow [PRD 07](./07-quality-and-testing.md) §2–§5.

---

## 1. Method

Three rules govern this document:

1. **Every record family is classified before implementation.** No family ships without a matrix row, a corpus fixture and an expected warning list.
2. **The ledger is executable.** Each row maps to at least one fixture in the corpus (§4) and one assertion in L2a/L2b (§5); a row without a fixture is not implemented.
3. **Classes are honest.** `FULL` requires a faithful reconstruction with no diagnostics; `DEGRADED` requires a visible approximation **and** a typed warning; `PLACEHOLDER` requires the §3 box from [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) and a warning.

---

## 2. Fidelity Matrix

### 2.1 Classic EMF (MS-EMF)

| Feature family | Records (examples) | Class | Notes / diagnostic |
| :--- | :--- | :--- | :--- |
| Header, bounds, frame, device units | `EMR_HEADER` | **FULL** | `output_bounds` + `physical_size` drive §5 mapping; malformed bounds → `metafile_bounds` (PLACEHOLDER) |
| Line / polyline / polygon / polypolygon | `POLYLINE`, `POLYGON`, `POLYPOLYGON` (+16-bit variants) | **FULL** | Direct `DrawPath`; open vs closed semantics preserved |
| Bezier curves | `POLYBEZIER`, `POLYBEZIERTO` (+16) | **FULL** | Cubic segments preserved |
| Rectangles, rounded rects, ellipses | `RECTANGLE`, `ROUNDRECT`, `ELLIPSE` | **FULL** | Rounded-rect radii from `EMR_ROUNDRECT`; ellipse via 4 cubics |
| Arcs, pies, chords | `ARC`, `PIE`, `CHORD` | **FULL** | `ArcKind` → sectored paths; max $5^\circ$ per segment |
| Path bracket + fill/stroke | `BEGINPATH`…`ENDPATH`, `FILLPATH`, `STROKEPATH`, `STROKEANDFILLPATH` | **FULL** | Even-odd/nonzero from `SetPolyFillMode` |
| Solid pens, width, caps, joins | `CREATEPEN`, `EXTCREATEPEN` | **DEGRADED** | Geometric widths scaled (§5.3); cosmetic pens ≥ 1 device px; distinct EMF+ start/end caps collapse to the start cap (patch 0004), since `Stroke` carries one cap |
| Dashed / styled pens | `EXTCREATEPEN` | **DEGRADED** | Custom dash patterns preserved where representable; hatch pens → solid + `metafile_pen_style` |
| Solid brushes | `CREATEBRUSHINDIRECT` | **FULL** | — |
| Hatched brushes | `CREATEBRUSHINDIRECT` (hatch) | **DEGRADED** | `VectorPattern` preset approximation + `metafile_hatch` |
| Pattern brushes / DIB pattern brush | `CREATEDIBPATTERNBRUSHPT`, `CREATEPATTERNBRUSH` | **DEGRADED** | Solid/tone approximation with `metafile_pattern_brush` |
| Palettes | `CREATE PALETTE`, `SETPALETTEENTRIES` | **FULL** | Indexed DIBs resolved to RGBA |
| Text output | `EXTTEXTOUTA/W`, `TEXTOUT` | **FULL** | Re-shaped through PRD 02c; alignment, escapement, `dx`, opaque background |
| Text with glyph indices | `ETO_GLYPH_INDEX` | **DEGRADED** | Re-shaped by text content + `metafile_glyph_index_text` |
| Font selection | `EXTCREATEFONTINDIRECTW` | **FULL** | Substitution matrix; `FontSubstituted` when the face is absent |
| Bitmaps | `STRETCHDIBITS`, `BITBLT`, `STRETCHBLT`, `SETDIBITSTODEVICE` | **FULL** | DIB decoded by `metafile-dib`; placed via affine |
| Alpha / mask / pattern blits | `ALPHABLEND`, `MASKBLT`, `PLGBLT` | **DEGRADED** | Nearest-equivalent blend; `metafile_blit` |
| Clipping: rect, region, path, intersect | `INTERSECTCLIPRECT`, `SELECTCLIPPATH`, `EXTSELECTCLIPRGN` | **FULL** | Nested `PushClip`/`PopClip` |
| Clipping: union / xor / diff | `EXTSELECTCLIPRGN` combine modes | **FULL / DEGRADED** | Exact compound paths for simple closed rect/polygon operands (OR/XOR; DIFF via two-clip subtraction). Curved, nested, even-odd or `Complement` operands keep the conservative bounding box + `metafile_clip_combine` |
| ROP2 | `SETROP2` | **FULL** for `R2_COPYPEN`, **DEGRADED** otherwise | Copy semantics; other modes ignored + `metafile_rop` |
| World transform | `SETWORLDTRANSFORM`, `MODIFYWORLDTRANSFORM` | **FULL** | Affine applied by the engine; arbitrary rotation/shear preserved |
| Window/viewport mapping | `SETWINDOW*`, `SETVIEWPORT*`, `SETMAPMODE` | **FULL** | Engine applies the mapping; adapter receives output coordinates |
| DC save/restore | `SAVEDC`, `RESTOREDC` | **FULL** | Engine state stack; unbalanced restore reported by engine |
| Comments / embedded EMF+ | `EMR_COMMENT` | **FULL** | EMF+ payloads dispatched to §2.2 |
| Unknown records | any | **DEGRADED** | No-op + `metafile_record_ignored`; critical records → PLACEHOLDER |
| Picture crop (`<a:srcRect>`) | — | **FULL** | Visible source window scaled into the destination and clipped (same guard as `DrawImage`) |
| Picture silhouette (`prstGeom`/`custGeom`) | — | **FULL** | Existing compiler clip wraps the expanded commands |
| Picture opacity (`<a:alphaModFix>`) | — | **DEGRADED** | Applied to embedded bitmaps only; vector ink ignores it + `metafile_opacity` |
| Unexpanded `DrawMetafile` marker | — | **PLACEHOLDER** | Defensive: expansion must have run; marker at a backend is a defect + `metafile_unexpanded` |

### 2.2 EMF+ (MS-EMFPLUS, Only and Dual streams)

| Feature family | Class | Notes / diagnostic |
| :--- | :--- | :--- |
| Object table (pens, brushes, paths, fonts, images, strings, regions) | **FULL** | Handles resolved by the engine; adapter sees resolved objects |
| Solid / linear-gradient brushes | **FULL** | `Paint::Solid`/`Paint::LinearGradient` → `VectorPaint::Solid`/`LinearGradient`; the gradient axis is the brush transform's x-axis |
| Path / radial gradients | **DEGRADED** | `EmfPlusPathGradientBrush` is approximated by a linear gradient (surrounding colours → centre) with `metafile_gradient`; the picture keeps painting instead of degrading to the placeholder (patch `0007`) |
| Texture brushes | **DEGRADED** | Alpha-weighted average colour of the texture bitmap + `metafile_pattern_brush` (no inline-image paint in the display list) |
| Hatched brushes | **DEGRADED** | Preset `VectorPattern` + `metafile_hatch` |
| Lines, polygons, paths, curves, beziers | **FULL** | Path → `VectorPath`; fill modes preserved |
| Rectangles, ellipses, arcs, pies, closed curves | **FULL** | Path construction per §4.3 of PRD 13 |
| Text (DrawString) | **FULL** | Alignment, escapement, `dx`, decorations; re-shaped via PRD 02c |
| Text with glyph indices | **DEGRADED** | Re-mapped through font cmap / `EmfPlusDrawDriverString` with fallback + `metafile_glyph_index_text` |
| Images (DrawImage, DrawImagePoints) | **FULL** | `DrawBitmap` with affine placement (rotation/shear preserved) |
| Clipping (SetClipRect, SetClipPath, SetClipRegion, ResetClip) | **FULL** | Nested clips; combine modes as §2.1 |
| World transform / page transform | **FULL** | Affine composition |
| Save/restore state | **FULL** | Engine state stack |
| Region combine (`OR`/`XOR`/`DIFF`) | **DEGRADED** | Bounding-box approximation + `metafile_clip_combine` |
| EMF+ Dual arbitration | **FULL** | EMF+ records preferred; classic fallback emits `metafile_emfplus_fallback` |
| EMF+ Only with no drawing records | **DEGRADED** | Classic records replayed (engine compatibility path) + `metafile_emfplus_fallback` |
| Timing / animation records | **OMITTED** | Ignored + `metafile_animation_ignored` (no time dimension in the display list) |

### 2.3 WMF (MS-WMF)

| Feature family | Class | Notes / diagnostic |
| :--- | :--- | :--- |
| Placeable header, units-per-inch | **FULL** | Mapping normalised to output coordinates |
| Lines, polygons, rectangles, ellipses, arcs, pies, chords | **FULL** | Same path construction as EMF |
| Pens, brushes, palettes | **FULL** | Solid; hatched → DEGRADED (`metafile_hatch`) |
| Text output | **FULL** | `TEXTOUT`/`EXTTEXTOUT`; re-shaped |
| DIB blits (`DIBBITBLT`, `DIBSTRETCHBLT`, `STRETCHDIB`) | **FULL** | DIB decode + affine placement |
| Clipping | **FULL** for rect/region/path; **DEGRADED** for combine | Same policy as EMF |
| Window/viewport, map modes | **FULL** | Engine mapping |
| Unknown records | **DEGRADED** | No-op + `metafile_record_ignored` |

### 2.4 Coverage ledger

Maintain the ledger at `crates/office-terminal-core/tests/fixtures/metafile/coverage.md`, one row per record family above, with columns:

| Family | Record IDs | Fixture | AST golden | PNG golden | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| … | … | `emf/…` | `ast/…json` | `png/…` | ✅ / ⚠️ / ❌ |

A matrix row may only move to `✅` when all four references exist and pass.

---

## 3. Diagnostics Taxonomy

Every engine diagnostic maps to exactly one tag; tags are deduplicated per `(element_id, tag)` before being emitted as `WarningEvent::FidelityDegraded { element_id, feature: tag, class }`.

| Tag | Trigger | Class | User-facing summary |
| :--- | :--- | :--- | :--- |
| `metafile_record_ignored` | Unsupported non-critical record | DEGRADED | "Metafile record ignored" |
| `metafile_rop` | `ROP2` other than copy | DEGRADED | "Raster operation approximated" |
| `metafile_clip_combine` | OR/XOR/DIFF clip regions | DEGRADED | "Complex clip approximated to its bounds" |
| `metafile_hatch` | Hatched brush | DEGRADED | "Hatch pattern approximated" |
| `metafile_pattern_brush` | DIB pattern brush | DEGRADED | "Pattern brush mapped to texture fill" |
| `metafile_texture_transform` | Texture transform outside the representable frame | DEGRADED | "Texture transform approximated" |
| `metafile_blit` | ALPHABLEND/MASKBLT/PLGBLT | DEGRADED | "Blit mode approximated" |
| `metafile_bitmap_downsampled` | Bitmap above the pixel limit | DEGRADED | "Embedded bitmap downsampled" |
| `metafile_bitmap_sampling` | Sampling mode not supported | DEGRADED | "Bitmap sampling approximated" |
| `metafile_pen_style` | Unrepresentable pen | DEGRADED | "Pen style approximated" |
| `metafile_text_dx` | `dx` length ≠ character count | DEGRADED | "Character spacing ignored" |
| `metafile_glyph_index_text` | Glyph-indexed text | DEGRADED | "Glyph text re-shaped" |
| `metafile_text_size` | Font size clamped | DEGRADED | "Text size clamped" |
| `metafile_gradient` | EMF+ path/radial gradient approximated by a linear ramp | DEGRADED | "Gradient approximated by a linear ramp" |
| `metafile_emfplus_fallback` | EMF+ dual/only fallback | DEGRADED | "EMF+ fell back to classic records" |
| `metafile_opacity` | Picture opacity < 1 | DEGRADED | "Picture transparency ignored for vector content" |
| `metafile_unexpanded` | Marker reached a backend | PLACEHOLDER | "Metafile expansion did not run" |
| `metafile_animation_ignored` | EMF+ timing records | OMITTED | "Metafile animation ignored" |
| `metafile_bounds` | Degenerate/oversized bounds | PLACEHOLDER | "Metafile bounds invalid" |
| `metafile_limit` | Resource limit hit | PLACEHOLDER | "Metafile exceeds resource limits" |
| `metafile_parse` | Parser/validation error | PLACEHOLDER | "Metafile could not be parsed" |

Warnings are attached to the **slide**, not the whole deck; `element_id` is `slide/<n>/picture/<id or index>`.

---

## 4. Golden Corpus

### 4.1 Layout

```
crates/office-terminal-core/tests/fixtures/metafile/
├── manifest.json               # fixture registry + expectations + attribution
├── coverage.md                 # the §2.4 ledger
├── emf/                        # classic EMF fixtures
├── emfplus/                    # EMF+ Only / Dual fixtures
├── wmf/                        # WMF + placeable fixtures
├── emz/                        # gzip-wrapped + bomb fixtures
├── reference/                  # one-time reference renders + provenance
├── ast/                        # L2a snapshots (normalized JSON)
└── png/                        # L2b goldens (960×720, 1920×1080)
```

Synthetic fixtures are generated by `crates/office-terminal-core/tests/metafile_support/generate.rs` (record builders for every matrix row). Third-party fixtures are copied from `metafile-rs/fixtures/real-world/` (Apache-2.0) and **must** record `source`, `upstream_revision`, `license` and `attribution` in the manifest.

### 4.2 Manifest schema

```json
{
  "$schema": "./manifest.schema.json",
  "fixtures": [
    {
      "id": "emf-polyline-solid",
      "path": "emf/emf-polyline-solid.emf",
      "format": "emf",
      "source": "synthetic",
      "license": "MIT",
      "attribution": "kitty-pptx test corpus",
      "expect": {
        "class": "full",
        "diagnostics": [],
        "ast": "ast/emf-polyline-solid.json",
        "png": [
          { "target": "960x720", "dssim_max": 0.02 },
          { "target": "1920x1080", "dssim_max": 0.02 }
        ]
      }
    },
    {
      "id": "emfplus-dual-gradient-text",
      "path": "emfplus/emfplus-dual-gradient-text.emf",
      "format": "emfplus-dual",
      "source": "metafile-rs fixtures/real-world/windows-gdiplus-emf.emf",
      "upstream_revision": "1.0.1",
      "license": "Apache-2.0",
      "attribution": "Dantusaikamal/metafile-rs",
      "expect": {
        "class": "degraded",
        "diagnostics": ["metafile_emfplus_fallback"],
        "ast": "ast/emfplus-dual-gradient-text.json",
        "png": [{ "target": "960x720", "dssim_max": 0.03 }]
      }
    }
  ]
}
```

`class` is the **highest** class present: `full` < `degraded` < `placeholder`. A fixture that unexpectedly degrades fails L2a/L2b.

### 4.3 Corpus composition requirements

- At least one fixture per §2 matrix row (classic/EMF+/WMF), including every DEGRADED and PLACEHOLDER row.
- A **degradation suite**: one fixture per diagnostics tag in §3, asserting exactly that tag and no others.
- A **picture-level suite**: cropped EMF (`srcRect`), silhouette-clipped EMF (rounded/ellipse), opacity EMF, and a deliberately unexpanded-marker unit test (constructed `VectorSlide`, not a file).
- A **stress suite**: 10 MiB-class EMF, 1 000 000-point polyline (limit), 64-deep DC save/restore, EMF+ container nesting at the limit, EMZ bomb.
- A **real-world suite**: at least 12 files covering Office-pasted charts, MATLAB/R plots, Visio diagrams, GDI+ text/gradients, and placeable WMF.
- Fixtures must be **deterministic**: no timestamps, no external fonts beyond the bundled stack, no embedded Windows-only faces.

---

## 5. Test Levels (mapped to PRD 07)

| Level | Metafile coverage | Files | Gate |
| :--- | :--- | :--- | :--- |
| **L1 — Unit** | Classification (ext/MIME/magic), pen/paint/path/text/bitmap/clip mappers, limits, diagnostics mapping, cache key/eviction | `tests/metafile_unit_tests.rs` | Blocking |
| **L2a — Vector Scene Goldens** | `VectorCommand` AST snapshots for every fixture, floats normalized to 4 decimals, bitmaps as `pngSha256` | `tests/metafile_golden_tests.rs` + `ast/*.json` | Blocking |
| **L2b — Composition Goldens** | Rasterized slide at 960×720 and 1920×1080; DSSIM/pixelmatch against committed PNGs | `tests/metafile_golden_tests.rs` + `png/` | Blocking |
| **L3 — Protocol** | Not applicable (metafiles do not reach the Kitty driver directly) | — | — |
| **L4 — Live Terminal Smoke** | A deck with EMF/EMF+ pictures renders in the TUI; warnings drawer lists the expected tags; sorter thumbnails match the canvas raster | `tests/metafile_smoke_tests.rs` (TestBackend) + manual matrix | Release-blocking |
| **L5 — CLI E2E** | `export --format png,svg`, `dump --json` (hash form + `--embed-bitmaps`), `info` counts, exit codes | `crates/office-terminal-cli/tests/metafile_e2e_tests.rs` | Blocking |
| **Fuzz** | Arbitrary bytes → classify → playback → recorder; no panic, bounded time/memory | `fuzz/fuzz_targets/fuzz_metafile.rs` | Blocking (smoke), nightly (extended) |

### 5.1 L2a normalization rules

- Transforms: `sx, ky, kx, sy, tx, ty` rounded to 4 decimals.
- Colors: straight RGBA8, no premultiplication.
- `DrawBitmap`: `{ width, height, pngSha256, placement, opacity, sampling }` — **never** base64 in AST goldens.
- Commands are compared in emission order; clip push/pop balance is asserted before comparison.
- Every golden scene is **expanded** before comparison: `assert!(!scene.commands.iter().any(|c| matches!(c, VectorCommand::DrawMetafile { .. })))`. An unexpanded marker is a test failure, not a golden diff.
- Picture-level fixtures assert the compiler's silhouette clip wrapping and the crop window; opacity fixtures assert the `metafile_opacity` warning.
- A golden diff prints the first divergent command with a JSON patch-style context.

### 5.2 L2b thresholds

Use [PRD 07 §4](./07-quality-and-testing.md) policy:

| Fixture class | Metric | Threshold |
| :--- | :--- | :--- |
| Synthetic geometry (lines/paths/bitmaps) | pixelmatch (YIQ, threshold 0.20, includeAA) | ≤ 0.5 % differing pixels |
| Text-bearing fixtures | DSSIM | ≤ 0.02 |
| Real-world fixtures | DSSIM | ≤ 0.06 (REVIEW band), reported per fixture |
| Degradation-suite placeholders | exact cell/box geometry | 0 differing pixels outside AA |

Determinism environment: pinned toolchain, committed `Cargo.lock`, bundled Carlito/Caladea, fixed targets.

---

## 6. Differential Strategy

Two independent oracles, neither linked into release builds:

1. **`metafile-svg` (dev-dependency, feature `metafile-oracle`)** — render the same bytes with the crate's reference SVG backend, rasterize that SVG with the project's existing `resvg`, and compare against our `VectorSlide` raster with DSSIM. This isolates **adapter mapping bugs** (our code) from **engine playback bugs**.
   - CI job: `cargo test --features metafile-oracle -p office-terminal-core --test metafile_oracle_tests`.
   - Advisory in M1–M2, blocking from M3 (after the adapter stabilizes).
2. **LibreOffice reference renders (one-time, offline)** — `soffice --headless --convert-to png` on the fixture deck, stored under `reference/` with tool version and command line in the manifest. Used only to validate the oracle itself; never executed in CI.

Divergence triage:

| Symptom | Likely cause |
| :--- | :--- |
| Our raster differs from `metafile-svg` | Adapter mapping (pen/paint/transform/clip) |
| Both differ from LibreOffice | Engine playback or font metrics |
| Text-only differences | Font substitution / shaping differences (expected; classify as REVIEW) |
| Bitmap-only differences | Sampling filter or placement transform |

---

## 7. Fuzz & Robustness

- Target: `fuzz/fuzz_targets/fuzz_metafile.rs`; seeds from `fixtures/metafile/**` plus truncated variants.
- Invariants:
  1. No panic, no `abort`, no integer overflow (release + debug).
  2. Playback terminates within 2× the per-metafile budget or aborts via limits.
  3. Recorder output always has balanced clip push/pop and finite transforms.
  4. Memory growth bounded by limits (checked with a counting allocator in the fuzz harness).
- Corpus additions: EMF+ container depth, cyclic region references, zero-size/negative bounds, 4 GiB-declared record sizes, EMZ bombs.
- Any panic is a release blocker ([PRD 07](./07-quality-and-testing.md) §3.3).

---

## 8. NFR Budgets & Measurement

| Metric | Target | Protocol |
| :--- | :--- | :--- |
| Playback + translate, EMF ≤ 1 MiB | p95 ≤ 50 ms | release, 10 iterations, `Instant` around `MetafileEngine::play` |
| Playback + translate, EMF ≤ 11 MiB | ≤ 250 ms | same, corpus large fixture |
| Cache hit | ≤ 1 ms | 1 000 hits, measure `Arc` clone path |
| Compiled command bytes per fixture | ≤ 8× input | `size_of` accounting in `cache.rs` |
| Peak RSS during playback | ≤ 8× input | `/proc/self/status` `VmHWM` delta |
| Repeat picture on 50 slides | total ≤ 1× playback + 50 cache hits | synthetic deck |
| Raster of compiled EMF at 1920×1080 | ≤ 1.5× equivalent vector slide | compare geometry-only fixture |
| SVG export size | ≤ 2× equivalent vector slide + embedded PNGs | `export --format svg` |

Budgets are enforced by `tests/metafile_perf_tests.rs` in release mode; regressions > 25 % fail CI on the dedicated perf job.

---

## 9. CI Gating & Golden Workflow

### 9.1 Gate matrix

| Job | Scope | Required |
| :--- | :--- | :--- |
| `cargo clippy --workspace --all-targets -- -D warnings` | whole workspace | ✅ |
| `cargo check -p office-terminal-core --no-default-features --tests` | feature-off build | ✅ |
| `cargo test -p office-terminal-core --test warning_plumbing_tests` | M0 warning path | ✅ |
| `cargo test -p office-terminal-core --lib metafile` | L1 mappers/limits/diagnostics/cache + unit tests | ✅ |
| `cargo test -p office-terminal-core --test metafile_skeleton_tests` | compiler/expansion seam | ✅ |
| `cargo test -p office-terminal-core --test metafile_milestone_tests` | M3–M5 fixtures | ✅ |
| `cargo test -p office-terminal-core --test metafile_mode_tests` | `--metafile` override | ✅ |
| `cargo test -p office-terminal-core --test metafile_robustness_tests` | panic/clip-balance smoke | ✅ |
| `cargo test --release -p office-terminal-core --test metafile_rss_tests` | peak-allocation budget | ✅ |
| `cargo test -p office-terminal-cli --test metafile_e2e_tests` | L5 `export`/`dump`/`info` | ✅ |
| `crates/office-terminal-tui/tests/overlay_tests.rs::metafile_warnings_render_in_the_drawer` | L4 warnings drawer | ✅ |
| `cargo test -p office-terminal-core --test external_fixture_tests external_libreoffice_renderer_comparison` | golden harness (expanded metafiles) | ✅ |
| `cargo test --release -p office-terminal-core --test metafile_perf_tests` | NFR budgets | ✅ |
| `cargo +nightly fuzz run fuzz_metafile -- -runs=2000` | extended robustness | nightly |
| `cargo test -p office-terminal-core --test metafile_golden_tests` | L2a + L2b AST/PNG corpus | ✅ |
| `cargo test -p office-terminal-core --test metafile_oracle_tests --features metafile-oracle` | `metafile-svg` differential | ⚠️ follow-up |
| `cargo test -p office-terminal-cli --test metafile_e2e_tests` | L5 CLI E2E | ✅ |

### 9.2 Golden update workflow

1. Change the adapter/engine; tests fail with a structured diff.
2. Run `KITTY_PPTX_UPDATE_METAFILE_GOLDENS=1 cargo test -p office-terminal-core --test metafile_golden_tests`.
3. Review generated `ast/*.json` and `png/*` diffs; update `coverage.md` and the manifest only when a matrix row changes.
4. PR must include the diff summary and a reviewer sign-off; goldens never change silently.

### 9.3 Failure triage

- **AST diff before PNG diff** → mapping regression (fix the adapter first).
- **PNG diff without AST diff** → rasterizer/SVG backend regression (not metafile-specific).
- **Diagnostics list diff** → engine version change or a new record in the corpus; update the manifest deliberately.

---

## 10. Acceptance Checklist

A metafile implementation is complete when all of the following hold:

- [ ] Every §2 matrix row has a fixture, an AST golden, a PNG golden and a `coverage.md` entry.
- [ ] No metafile picture ever produces an empty region: output is either playback commands or the §3 placeholder ([PRD 13 §7](./13-metafile-pictures-and-vector-playback.md)).
- [ ] No `DrawMetafile` marker survives expansion in any pipeline, export or dump path; the unexpanded-marker test passes.
- [ ] Cropped, silhouette-clipped and opacity metafile pictures match their picture-level goldens.
- [ ] Every DEGRADED/PLACEHOLDER fixture emits exactly its expected diagnostics, deduplicated per element.
- [ ] L1, L2a, L2b, L5 and the fuzz smoke are blocking and green; the differential oracle is green from M3.
- [ ] AST output is deterministic across runs and platforms (normalized floats, bitmap hashes).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean; every new file is < 600 lines.
- [ ] NFR budgets in §8 pass on the perf runner.
- [ ] `export --format svg` is self-contained (data URIs only) and text remains selectable under the default SVG text mode.
- [ ] `dump --json` emits bitmap hashes by default and full bytes only with `--embed-bitmaps`.
- [ ] TUI warnings drawer lists metafile diagnostics with slide/element labels ([PRD 04](./04-interactive-tui-presenter.md)), through the warning plumbing added in [PRD 13 §10.5](./13-metafile-pictures-and-vector-playback.md).
- [ ] Documentation updated: [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) EMF row, [PRD 06](./06-component-interface-contracts.md) `DrawBitmap`, [PRD 07](./07-quality-and-testing.md) corpus references, `crates/VENDORED.md`.

---

## 11. Open Items

1. **Exact region booleans** — decide after M2 whether `metafile_clip_combine` fixtures show visible error; if so, adopt `i_overlay` and promote the row to FULL.
2. **Bitmap sampling** — confirm `Nearest` vs `Linear` thresholds in L2b; add a per-fixture sampling override if AA noise dominates.
3. **Oracle build cost** — `metafile-oracle` pulls `metafile-svg`; measure CI build time before making it blocking for every push (may run on merge queue only).
4. **EMF+ animation records** — verify the engine reports them individually so `metafile_animation_ignored` can be deduplicated at slide level.
5. **Reference renders** — confirm LibreOffice version pinning and license for storing `reference/*.png`.

---

## 12. References

- [MS-EMF], [MS-EMFPLUS], [MS-WMF] — Microsoft Open Specifications.
- [`metafile-rs`](https://github.com/Dantusaikamal/metafile-rs) — record audits (`docs/emf-record-audit.md`, `docs/emfplus-record-audit.md`, `docs/wmf-record-audit.md`), qualification notes and fixtures.
- [PRD 07](./07-quality-and-testing.md) — L1–L5 hierarchy, DSSIM thresholds, determinism environment.
- [PRD 13](./13-metafile-pictures-and-vector-playback.md) — architecture, adapter mapping, limits, milestones.
