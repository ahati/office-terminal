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

Counts: 86 blocked cases are archived here (85 in the `core` batch targets, 1 in
`parity_text_tests`), plus 1 in the TUI archive — 87 in total. Every compiled test in
`crates/kitty-pptx-core/tests` runs.

| archive | blocked cases |
| :--- | --: |
| `parity_playwright_tests.rs.txt` | 11 |
| `parity_shapes_a_tests.rs.txt` | 1 |
| `parity_text_box_b_bullet_image_tests.rs.txt` | 4 |
| `parity_text_box_b_eavert_tests.rs.txt` | 13 (+6 scaffolding helpers) |
| `parity_text_box_b_stacked_tests.rs.txt` | 25 |
| `parity_text_box_b_warp_tests.rs.txt` | 14 |
| `parity_text_tests.rs.txt` | 1 |
| `parity_viewer_lifecycle_tests.rs.txt` | 5 |
| `parity_viewer_selection_tests.rs.txt` | 9 |
| `parity_viewer_worker_tests.rs.txt` | 3 |
| **total** | **86** |

The TUI crate has its own archive at
`crates/kitty-pptx-tui/tests/parity/pending/parity_viewer_nav_tests.rs.txt` (1 case,
re-scoped to V2/wave 4). `parity_viewer_nav_tests.rs.txt` was reinstated in wave 3
(the hidden-slide helpers, zoom clamp/change gate landed; the `fitWidth` DOM-container
case is N/A for a terminal presenter).

## Wave 3: reinstating a case

When the named API lands:

1. Copy the function (with its doc comment and attributes) from the archive back into the
   compiled target named in the archive's first line.
2. Remove the `#[ignore = ...]` attribute and any helper stubs it no longer needs; delete
   the function from the archive (and its header row).
3. Fix the call sites to the landed API and run the target:
   `cargo test -p kitty-pptx-core --test <target>`.
4. Move the ledger row in `tests/parity/COVERAGE.md` and the per-batch report from BLOCKED
   to EXACT/BEHAVIOURAL (adjust the test/pass counts by the same delta).

Helpers archived next to their cases (for example the `east_asian_default` stubs or the
line-end `shaft_ends` recorder in the `text-box-b` archives) come back with them.
