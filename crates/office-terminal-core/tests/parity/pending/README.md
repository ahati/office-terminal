# Blocked-case archive (`tests/parity/pending/`)

This directory holds the parity cases that are **blocked on a production API that does
not exist yet**. Their source is stored here as plain text (`<target-stem>.rs.txt`), so
cargo never compiles them.

## The rule

> A test that is included must run.

The compiled test tree contains **no `#[ignore]`**. A case that cannot pass is not
compiled at all: its full source (doc comment, `#[test]` + `#[ignore = "<reason>"]`
attributes, body, and any helper used only by it) lives in the matching archive here.
`grep -rn '#\[ignore' crates/*/tests --include='*.rs'` must stay empty.

## The coverage ledger is authoritative

Archiving a case does **not** remove its coverage row. `tests/parity/COVERAGE.md` and the
per-batch reports in `tests/parity/coverage/*.md` still record every BLOCKED case, the
reference `it()`, and the symbol that would unblock it. The archive header repeats the
missing API per function, so the ledger row and the code stay in sync.

Counts: 23 residual cases (19 BLOCKED, 4 N/A) are archived here across 5 files;
the TUI archive is empty (0 pending). Every compiled test in `crates/office-terminal-core/tests` runs.

| archive | cases | BLOCKED | N/A |
| :--- | --: | --: | --: |
| `parity_playwright_tests.rs.txt` | 11 | 9 | 2 |
| `parity_text_box_b_warp_tests.rs.txt` | 2 | 2 | 0 |
| `parity_viewer_lifecycle_tests.rs.txt` | 4 | 4 | 0 |
| `parity_viewer_selection_tests.rs.txt` | 3 | 2 | 1 |
| `parity_viewer_worker_tests.rs.txt` | 3 | 2 | 1 |
| **total** | **23** | **19** | **4** |

## Wave 3 / Wave 5: reinstating a case

When the named API lands:

1. Copy the function (with its doc comment and attributes) from the archive back into the
   compiled target named in the archive's first line.
2. Remove the `#[ignore = ...]` attribute and any helper stubs it no longer needs; delete
   the function from the archive (and its header row).
3. Fix the call sites to the landed API and run the target:
   `cargo test -p office-terminal-core --test <target>`.
4. Move the ledger row in `tests/parity/COVERAGE.md` and the per-batch report from BLOCKED
   to EXACT/BEHAVIOURAL (adjust the test/pass counts by the same delta).
