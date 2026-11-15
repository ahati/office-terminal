# Fuzz targets

`fuzz_metafile` feeds arbitrary bytes through classification, the EMF/WMF/EMF+
playback engine and the display-list recorder (PRD 13 §8.3 / PRD 13a §7).

```bash
cargo install cargo-fuzz            # once
cargo +nightly fuzz run fuzz_metafile -- -runs=2000
```

Seeds live under `crates/kitty-pptx-core/tests/fixtures/metafile/`:

```bash
mkdir -p fuzz/corpus/fuzz_metafile
cp crates/kitty-pptx-core/tests/fixtures/metafile/* fuzz/corpus/fuzz_metafile/
```

The blocking per-commit smoke is
`crates/kitty-pptx-core/tests/metafile_robustness_tests.rs`, which replays a
deterministic corpus without libFuzzer; this crate carries the extended nightly
run. `fuzz/` is excluded from the workspace so `cargo test --workspace` does not
build it.
