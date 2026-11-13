//! `powerpoint-line-metrics.test.ts` — #1610 line metrics, kernel level.
//!
//! Ported from `packages/pptx/src/powerpoint-line-metrics.test.ts` (read-only @
//! `1f5c6e1d`). The spec drives `renderTextBody` with a canvas stub whose only
//! metric input is the 1.2 × size line; the numbers below are therefore a
//! function of the *face's catalogued split* and the layout kernels. Rust
//! resolves the split from the installed face (`text/shape.rs:104-160`), so the
//! ports pass the catalogue share in and assert the production kernels
//! (`natural_line_box`, `spaced_line_box`, `paragraph_spacing`) plus the anchor
//! rule of `renderer.ts:5530-5575` through `support::Stack`.
//!
//! Cases that need a symbol the Rust core does not have yet
//! (`powerPointAscentShare`, the `compatLnSpc="0"` Excel box, `fontAlgn`,
//! the `a:br`/`endParaRPr` face accounting, the share cache) are reported as
//! BLOCKED in `tests/parity/coverage/text-box-a.md`, never dropped silently.

#[path = "text_box_a_support/mod.rs"]
mod support;

use kitty_pptx_core::text::metrics::FaceMetrics;
use support::{
    body, close, close_tol, compile, line, line_pts, para, text, units, Para, Stack, ARIAL,
    BASKERVILLE, CALIBRI, COURIER, GABRIOLA, GEORGIA, MEIRYO, MS_GOTHIC, MS_PGOTHIC, STENCIL,
    SYNTHETIC, TIMES,
};

/// `it('splits each face by its usWin (or USE_TYPO_METRICS typo) ascent share')`
/// — the A-* controls: the first and second baselines of two same-size lines.
#[test]
fn face_share_splits_each_line_box() {
    let cases: [(&str, f32, FaceMetrics, [i32; 2]); 9] = [
        ("Arial", 72.0, ARIAL, [97, 217]),
        ("Calibri", 72.0, CALIBRI, [94, 214]),
        ("Yu Gothic", 72.0, support::YU_GOTHIC, [92, 212]),
        ("Gabriola", 72.0, GABRIOLA, [98, 218]),
        ("Baskerville Old Face", 200.0, BASKERVILLE, [258, 591]),
        ("Stencil", 200.0, STENCIL, [255, 589]),
        ("Times New Roman", 200.0, TIMES, [268, 602]),
        ("Georgia", 200.0, GEORGIA, [269, 602]),
        ("Courier New", 200.0, COURIER, [245, 578]),
    ];
    for (name, size, face, expected) in cases {
        assert_eq!(
            Stack::new(Para::many(2, size, face)).units(),
            expected,
            "{name} {size} pt"
        );
    }
}

/// `it('rounds each baseline, not the line pitch, to a whole 1/100 inch')`.
#[test]
fn each_baseline_rounds_independently_of_the_pitch() {
    // 100 pt lines are 166.67 units high: Arial steps 167, Meiryo 166.
    assert_eq!(Stack::new(Para::many(2, 100.0, ARIAL)).units(), [135, 302]);
    assert_eq!(Stack::new(Para::many(2, 100.0, MEIRYO)).units(), [118, 284]);
    // Fractional sizes keep an exact 1.2 × size line (Z-*).
    assert_eq!(
        Stack::new(Para::many(8, 10.5, ARIAL)).units(),
        [14, 32, 49, 67, 84, 102, 119, 137]
    );
    assert_eq!(
        Stack::new(Para::many(8, 13.33, MEIRYO)).units(),
        [16, 38, 60, 82, 105, 127, 149, 171]
    );
}

/// `it('unions the runs of a mixed line and rescales them into 1.2 × the
/// largest size')`.
#[test]
fn mixed_runs_union_then_rescale_into_the_largest_size() {
    let mixed = |a: FaceMetrics, b: FaceMetrics| {
        Stack::new(vec![
            Para::one(100.0, a),
            Para::of(vec![Line2::two(100.0, a, b)]),
            Para::one(100.0, a),
        ])
        .baselines()
    };
    let meiryo = mixed(ARIAL, MEIRYO);
    assert_eq!(units(&meiryo), [135, 289, 468]);
    // The middle line is exactly 1.2 × 100 even though its parts are not.
    let (a, d) = kitty_pptx_core::text::metrics::natural_line_box(&[(100.0, ARIAL), (100.0, MEIRYO)]);
    close(a + d, 120.0, "mixed line box");
    // 40 pt line, 40 + 100 pt mixed line, 40 pt line.
    let sized = Stack::new(vec![
        Para::one(40.0, ARIAL),
        Para::of(vec![support::Line(vec![(40.0, ARIAL), (100.0, ARIAL)])]),
        Para::one(40.0, ARIAL),
    ])
    .units();
    assert_eq!(sized, [54, 202, 287]);
}

/// Helper alias so the mixed-line fixtures stay readable.
struct Line2;

impl Line2 {
    fn two(size: f32, first: FaceMetrics, second: FaceMetrics) -> support::Line {
        support::Line(vec![(size, first), (size, second)])
    }
}

/// `it('re-divides spaced lines with the shared DrawingML rule and rounds
/// spcPts to whole points')`.
#[test]
fn spaced_lines_redivide_and_round_spc_pts_to_whole_points() {
    let pts = |val: f32| Some((false, val));
    let two = |size: f32, face: FaceMetrics, space| {
        Stack::new(Para::many(2, size, face).into_iter().map(|p| p.space_line(space)).collect())
            .units()
    };
    assert_eq!(two(100.0, ARIAL, pts(114.0)), [127, 285]);
    assert_eq!(two(100.0, ARIAL, pts(126.0)), [131, 306]);
    // H equal to the natural 120 pt keeps the natural split (MS Gothic).
    assert_eq!(two(100.0, MS_GOTHIC, pts(120.0)), [143, 310]);
    assert_eq!(two(100.0, MEIRYO, pts(144.0)), [143, 343]);
    assert_eq!(two(100.0, MEIRYO, Some((true, 150_000.0))), [180, 430]);
    // P-*: 40.5 pt spacing lays out as 41 pt, 48.33 pt as 48 pt.
    assert_eq!(
        Stack::new(
            Para::many(8, 40.0, ARIAL)
                .into_iter()
                .map(|p| p.space_line(pts(40.5)))
                .collect()
        )
        .units(),
        [44, 101, 158, 215, 272, 329, 386, 443]
    );
    assert_eq!(
        Stack::new(
            Para::many(8, 40.0, ARIAL)
                .into_iter()
                .map(|p| p.space_line(pts(48.33)))
                .collect()
        )
        .units(),
        [54, 121, 187, 254, 321, 387, 454, 521]
    );
}

/// `it('applies edge spacing only with spcFirstLastPara and anchors the
/// unrounded block')`.
#[test]
fn edge_spacing_gate_and_unrounded_block_anchor() {
    let edge = || {
        vec![
            Para::one(100.0, ARIAL).before(Some((false, 72.0))),
            Para::one(100.0, ARIAL),
        ]
    };
    // bodyPr@spcFirstLastPara defaults to false: the first gap moves nothing.
    assert_eq!(Stack::new(edge()).units(), [135, 302]);
    assert_eq!(Stack::new(edge()).first_last(true).units(), [235, 402]);

    // G-Arial-ctr: two 100 pt lines centred in a 420 pt box, 3.6 pt insets.
    let ctr = Stack::new(Para::many(2, 100.0, ARIAL))
        .in_box(0.0, 420.0, 3.6, 3.6)
        .anchor("ctr");
    let top = 3.6 + (420.0 - 7.2 - 2.0 * 120.0) / 2.0;
    let shifted: Vec<i32> = ctr
        .baselines()
        .iter()
        .map(|y| ((y - top) / support::UNIT + 0.5).floor() as i32)
        .map(|n| n + (((top - 3.6) / support::UNIT + 0.5).floor() as i32))
        .collect();
    assert_eq!(shifted, [255, 422]);
}

/// `it('sizes a line by the run latin face even when it draws no glyph, never
/// by an unused ea face')` (supplement-3 U-* / supplement-2 R-*).
#[test]
fn line_box_counts_the_latin_slot_and_ignores_an_unused_ea_slot() {
    let probe = |mid: support::Line| {
        Stack::new(vec![
            Para::one(100.0, ARIAL),
            Para::of(vec![mid]),
            Para::one(100.0, ARIAL),
        ])
        .units()
    };
    // Ideographs only: the latin face paints nothing but still counts.
    assert_eq!(probe(Line2::two(100.0, CALIBRI, MS_PGOTHIC)), [135, 299, 468]);
    assert_eq!(probe(Line2::two(100.0, MEIRYO, MS_PGOTHIC)), [135, 291, 468]);
    // Latin only: an unused Meiryo ea face does not count.
    assert_eq!(probe(line(100.0, ARIAL)), [135, 302, 468]);
}

/// `it('keeps each run latin slot when adjacent runs share a drawn face
/// (colour never moves a line)')`.
#[test]
fn adjacent_runs_keep_each_latin_slot() {
    // Two ideograph runs drawn in Meiryo with latin slots Calibri / Gabriola.
    let pair = || {
        vec![
            Para::of(vec![support::Line(vec![
                (60.0, CALIBRI),
                (60.0, GABRIOLA),
                (60.0, MEIRYO),
            ])]),
            Para::one(60.0, ARIAL),
        ]
    };
    let same = Stack::new(pair()).baselines();
    assert_eq!(same.len(), 2);
    // Gabriola's slot contributes: the pair differs from a Calibri-only line.
    let single = Stack::new(vec![
        Para::of(vec![Line2::two(60.0, CALIBRI, MEIRYO)]),
        Para::one(60.0, ARIAL),
    ])
    .baselines();
    assert!(
        (single[0] - same[0]).abs() > 0.5,
        "Gabriola slot must move the line: {} vs {}",
        single[0],
        same[0]
    );
}

/// `it('anchors by the last line natural descent and applies normAutofit to
/// the split')`.
#[test]
fn bottom_anchor_uses_the_last_natural_descent() {
    // Q-Arial-b-p150: two 100 pt lines at 150 %, bottom of a 420 pt box.
    let q = Stack::new(
        Para::many(2, 100.0, ARIAL)
            .into_iter()
            .map(|p| p.space_line(Some((true, 150_000.0))))
            .collect(),
    )
    .in_box(0.0, 420.0, 3.6, 3.6)
        .anchor("b");
    let rel: Vec<f32> = q.baselines().iter().map(|y| (y - 3.6) / support::UNIT).collect();
    close_tol(rel[0], 292.23, 0.52, "Q-Arial-b-p150 first");
    close_tol(rel[1], 542.21, 0.52, "Q-Arial-b-p150 second");

    // N-Arial-90-10-t: normAutofit fontScale 90 %, lnSpcReduction 10 %.
    let n = Stack::new(Para::many(4, 36.0, ARIAL))
        .reduction(0.1)
        .in_box(0.0, 260.0, 3.6, 3.6);
    assert_eq!(
        units(&n.baselines().iter().map(|y| y - 3.6).collect::<Vec<f32>>()),
        [43, 97, 151, 205]
    );
}

/// `it('keeps the ordinary line model when any run has an unresolved face')`.
#[test]
fn unresolved_face_keeps_the_synthetic_split() {
    let ys = Stack::new(Para::many(
        1,
        20.0,
        FaceMetrics {
            ascent_share: SYNTHETIC.ascent_share,
        },
    ))
    .baselines();
    close(ys[0], 20.0 * 1.2 * 0.8, "synthetic 0.8 split");
}

/// `describe('bodyPr compatLnSpc') > it('keeps the #1610 model for
/// compatLnSpc="1" exactly as when it is omitted')`.
#[test]
fn compat_ln_spc_one_keeps_the_1610_model() {
    // The three controls: one line each, taken from the reference PDF export.
    let controls: [(&str, f32, FaceMetrics, i32); 3] = [
        ("Times New Roman", 100.0, TIMES, 134),
        ("Meiryo", 54.0, MEIRYO, 64),
        ("Gabriola", 100.0, GABRIOLA, 136),
    ];
    for (name, size, face, exported) in controls {
        assert_eq!(
            Stack::new(vec![Para::one(size, face)]).units(),
            vec![exported],
            "{name} {size} pt"
        );
    }
    // End to end: an explicit compatLnSpc="1" lays out like an omitted one.
    let omitted = body(&[para(vec![text("Ag", 24.0)])]);
    let mut explicit = omitted.clone();
    explicit.compat_ln_spc = Some(true);
    let omitted_y = line_pts(&compile(&omitted, 400.0, 200.0));
    let explicit_y = line_pts(&compile(&explicit, 400.0, 200.0));
    assert_eq!(explicit_y, omitted_y);
    close(
        omitted_y[0],
        1.2 * 24.0 * kitty_pptx_core::text::shape::ascent_share_for("Carlito", false, false),
        "first baseline",
    );
    // Carlito is metric-compatible with Calibri (usWin 1950 / 2500).
    close_tol(omitted_y[0], 1.2 * 24.0 * 0.78, 1e-2, "Calibri split");
}
