# Blocked-case archive (`kitty-pptx-tui/tests/parity/pending/`)

Blocked parity cases (no production API yet) are archived here as plain text
(`<target-stem>.rs.txt`), so cargo never compiles them.

Rule: **a test that is included must run** — the compiled test tree contains no
`#[ignore]`. `cargo test -p kitty-pptx-tui` runs every test in
`crates/kitty-pptx-tui/tests/`, and `grep -rn '#\[ignore' crates/*/tests --include='*.rs'`
is empty.

The coverage ledger stays authoritative: the BLOCKED rows remain in
`crates/kitty-pptx-core/tests/parity/COVERAGE.md` and
`crates/kitty-pptx-core/tests/parity/coverage/viewer.md` (they cover both nav targets).
Each archive header repeats the missing API per function.

Archived: `parity_viewer_nav_tests.rs.txt` — 3 cases (hidden-slide policy, find
controller, hyperlink-click delegation).

Wave 3 reinstates each function (copy it back, drop `#[ignore]`, adjust to the landed API,
move its ledger row from BLOCKED to ported). The full procedure is in
`crates/kitty-pptx-core/tests/parity/pending/README.md`.
