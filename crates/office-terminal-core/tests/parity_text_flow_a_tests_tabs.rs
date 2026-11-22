//! Parity ports of the reference text-flow specs (batch `text-flow-a`, part 2:
//! tab-stop grid kernel).
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/tab-layout.test.ts`
//! (13 `it()` cases) plus the reading-frame half of `rtl-tab-stops.test.ts`
//! case (5). Every probe calls `resolve_tab_widths` with the spec's own item
//! widths, stops, start pen and limit and asserts the reference's gap widths.
//!
//! Coverage report: `tests/parity/coverage/text-flow-a.md`.

#[path = "parity_text_flow_a_support/mod.rs"]
mod support;

use office_terminal_core::text::TabAlign;
use support::{cell, close, stop, tab_item, tab_widths};

// ── tab-layout.test.ts ─────────────────────────────────────────────────────

/// `:26-36`: LTR single right stop — the following cell ENDS on the stop.
#[test]
fn c21_ltr_right_stop_ends_the_cell_on_the_stop() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(400.0, TabAlign::Right)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [360.0, 40.0]);
}

/// `:31-44`: N tab cells per line — right stops at 200 and 400 give gaps 110
/// and 160.
#[test]
fn c22_supports_n_tab_cells_per_line() {
    let out = tab_widths(
        &mut [cell(50.0), tab_item(), cell(40.0), tab_item(), cell(40.0)],
        &[
            stop(200.0, TabAlign::Right),
            stop(400.0, TabAlign::Right),
        ],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [50.0, 110.0, 40.0, 160.0, 40.0]);
    close(50.0 + out[1] + 40.0, 200.0, "cell 1 right edge");
    close(50.0 + out[1] + 40.0 + out[3] + 40.0, 400.0, "cell 2 right edge");
}

/// `:46-50`: a centre stop centres the following cell (400 − 40/2 = 380).
#[test]
fn c23_centre_stop_centres_the_cell() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(400.0, TabAlign::Center)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [380.0, 40.0]);
}

/// `:52-56`: a start (l) tab MATERIALISES the gap (#916 item-3).
#[test]
fn c24_left_tab_materialises_the_gap() {
    let out = tab_widths(
        &mut [tab_item(), cell(30.0)],
        &[stop(100.0, TabAlign::Left)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [100.0, 30.0]);
}

/// `:58-62`: `dec` aligns like right and renders the gap.
#[test]
fn c25_decimal_tab_aligns_like_right() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(400.0, TabAlign::Decimal)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [360.0, 40.0]);
}

/// `:64-68`: unreachable stop → the fallback (space) gap.
#[test]
fn c26_unreachable_stop_falls_back_to_a_space() {
    let out = tab_widths(&mut [tab_item(), cell(30.0)], &[], 0.0, 600.0, 20.0, 0.0);
    assert_eq!(out, [20.0, 30.0]);
}

/// `:70-75`: a cell that would overflow the trailing text edge is clamped
/// (#835) — stop 700 in a 600 area puts the cell at [560, 600].
#[test]
fn c27_overflow_clamps_the_cell() {
    let out = tab_widths(
        &mut [tab_item(), cell(40.0)],
        &[stop(700.0, TabAlign::Right)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [560.0, 40.0]);
    close(out[0] + out[1], 600.0, "cell right edge");
}

/// `:77-86`: the pen never moves backwards — a right stop behind the pen
/// collapses the tab.
#[test]
fn c28_pen_never_moves_backwards() {
    let out = tab_widths(
        &mut [tab_item(), cell(20.0), tab_item(), cell(40.0)],
        &[stop(100.0, TabAlign::Left), stop(140.0, TabAlign::Right)],
        50.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [50.0, 20.0, 0.0, 40.0]);
}

/// `:88-95`: the nearest stop strictly past the pen wins, order-agnostic.
#[test]
fn c29_nearest_stop_wins_regardless_of_order() {
    let out = tab_widths(
        &mut [tab_item(), cell(10.0), tab_item(), cell(10.0)],
        &[stop(400.0, TabAlign::Left), stop(200.0, TabAlign::Left)],
        0.0,
        600.0,
        0.0,
        0.0,
    );
    assert_eq!(out, [200.0, 10.0, 190.0, 10.0]);
}

/// `:97-102`: `defTabSz` grid — a tab with no explicit stop jumps to the next
/// grid line (100).
#[test]
fn c30_def_tab_grid_jumps_to_the_next_line() {
    let out = tab_widths(&mut [tab_item(), cell(30.0)], &[], 0.0, 600.0, 0.0, 100.0);
    assert_eq!(out, [100.0, 30.0]);
}

/// `:104-112`: the grid resumes past the last explicit stop (l@120, then grid
/// 100 → 200, gap 50).
#[test]
fn c31_def_tab_grid_resumes_past_the_last_stop() {
    let out = tab_widths(
        &mut [tab_item(), cell(30.0), tab_item(), cell(20.0)],
        &[stop(120.0, TabAlign::Left)],
        0.0,
        600.0,
        0.0,
        100.0,
    );
    assert_eq!(out, [120.0, 30.0, 50.0, 20.0]);
}

/// `:114-119`: a pen sitting exactly on a grid line advances to the NEXT line.
#[test]
fn c32_def_tab_grid_advances_from_a_grid_line() {
    let out = tab_widths(&mut [tab_item(), cell(10.0)], &[], 100.0, 600.0, 0.0, 100.0);
    assert_eq!(out, [100.0, 10.0]);
}

/// `:121-125`: `defTabSz = 0` keeps the legacy space degradation.
#[test]
fn c33_def_tab_zero_degrades_to_a_space() {
    let out = tab_widths(&mut [tab_item(), cell(30.0)], &[], 0.0, 600.0, 20.0, 0.0);
    assert_eq!(out, [20.0, 30.0]);
}
