# Blocked-case archive (`office-terminal-tui/tests/parity/pending/`)

Blocked parity cases (no production API yet) are archived here as plain text
(`<target-stem>.rs.txt`), so cargo never compiles them.

Rule: **a test that is included must run** — the compiled test tree contains no
`#[ignore]`. `cargo test -p office-terminal-tui` runs every test in
`crates/office-terminal-tui/tests/`, and `grep -rn '#\[ignore' crates/*/tests --include='*.rs'`
is empty.

The coverage ledger stays authoritative: all nav cases are now reinstated and green;
`office-terminal-tui` has 0 pending archives.
