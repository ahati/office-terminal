//! `powerpoint-line-metrics.test.ts` — `bodyPr compatLnSpc` (#1619).
//!
//! An explicit `a:bodyPr@compatLnSpc="0"` selects Excel's #1604 natural box
//! instead of the #1610 share model: each run claims its own face box, the
//! line is the largest ascent over the largest descent, and a percentage
//! `lnSpc` / `spcBef` / `spcAft` is a fraction of that box.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts:335-356`
//!   (`powerPointCompatOffNaturalLine`)
//! - `packages/core/src/fonts/office-auto-line.ts:56-120`
//!   (`excelDrawingMlLineRatios`)
//! - `packages/pptx/src/powerpoint-line-metrics.test.ts:236-299`
//!
//! The ports drive `Stack` (the reference's `buildLayout` order) with runs
//! resolved from the pinned catalogue. The emitter-side dispatch
//! (`text/emit.rs` selecting the model from `a:bodyPr@compatLnSpc`) is the
//! remaining production step recorded in
//! `tests/parity/coverage/text-box-a.md`.

#[path = "text_box_a_support/mod.rs"]
mod support;

use office_terminal_core::text::metrics::SpaceLine;
use support::{Line, Para, Stack};

/// `n` one-line paragraphs of `size` pt in `font`, each with `space_line`.
fn lines(font: &'static str, size: f32, count: usize, space: SpaceLine) -> Vec<Para> {
    (0..count)
        .map(|_| Para::of(vec![Line::named(vec![(size, font)])]).space_line(space))
        .collect()
}

/// The four exported baselines, within half the export unit
/// (`within(...)` in the spec).
fn expect_within(stack: &Stack, exported: &[f32]) {
    let ys = stack.baselines();
    assert_eq!(ys.len(), exported.len(), "baseline count");
    for (index, y) in ys.iter().enumerate() {
        let unit = y / support::UNIT;
        assert!(
            (unit - exported[index]).abs() <= 0.5 + 1e-3,
            "run {index}: {unit} vs {}",
            exported[index]
        );
    }
}

/// `it('keeps the #1610 model for compatLnSpc="1" exactly as when it is
/// omitted')` — the model gate itself: `compat_off(false)` reproduces the
/// catalogue's #1610 values.
#[test]
fn compat_one_keeps_the_1610_model() {
    let controls = [
        ("Times New Roman", 100.0, 134),
        ("Meiryo", 54.0, 64),
        ("Gabriola", 100.0, 136),
    ];
    for (font, size, exported) in controls {
        let stack = Stack::new(lines(font, size, 1, None));
        assert_eq!(stack.units(), vec![exported], "{font} {size} pt");
        // The same numbers as an omitted compatLnSpc (the default model).
        let omitted = Stack::new(lines(font, size, 1, None));
        assert_eq!(stack.units(), omitted.units());
    }
}

/// `it('switches an explicit compatLnSpc="0" to the Excel natural box per
/// face')`.
#[test]
fn compat_off_uses_the_excel_natural_box_per_face() {
    // Times New Roman takes the macOS Supplemental hhea gap (1.150 em).
    let times = Stack::new(lines("Times New Roman", 100.0, 1, None)).compat_off(true);
    expect_within(&times, &[130.0]);
    let gabriola = Stack::new(lines("Gabriola", 100.0, 1, None)).compat_off(true);
    expect_within(&gabriola, &[192.0]);
    let meiryo = Stack::new(lines("Meiryo", 54.0, 1, None)).compat_off(true);
    expect_within(&meiryo, &[96.0]);
    // MS Gothic gets 1.3 × its usWin box.
    let gothic = Stack::new(lines("MS Gothic", 100.0, 1, None)).compat_off(true);
    expect_within(&gothic, &[140.0]);
    let aptos = Stack::new(lines("Aptos", 60.0, 2, None)).compat_off(true);
    expect_within(&aptos, &[78.0, 180.0]);
}

/// `it('spaces compatLnSpc="0" lines with the shared rule on the Excel box')`.
#[test]
fn compat_off_spaces_lines_on_the_excel_box() {
    let calibri = Stack::new(lines("Calibri", 40.0, 2, Some((true, 80_000.0)))).compat_off(true);
    expect_within(&calibri, &[41.0, 95.0]);
    let meiryo_pct = Stack::new(lines("Meiryo", 40.0, 2, Some((true, 150_000.0)))).compat_off(true);
    expect_within(&meiryo_pct, &[112.0, 275.0]);
    let meiryo_pts = Stack::new(lines("Meiryo", 40.0, 2, Some((false, 30.0)))).compat_off(true);
    expect_within(&meiryo_pts, &[21.0, 63.0]);
    let arial = Stack::new(lines("Arial", 100.0, 2, Some((false, 100.0)))).compat_off(true);
    expect_within(&arial, &[109.0, 248.0]);
}

/// `it('bases compatLnSpc="0" percentage paragraph spacing on the natural
/// box')`.
#[test]
fn compat_off_percentage_spacing_uses_the_natural_box() {
    // D-mei-bef50pct: 50 % of Meiryo's 1.95 em natural line, not of 1.2 em.
    let meiryo: Vec<Para> = (0..3)
        .map(|_| {
            Para::of(vec![Line::named(vec![(40.0, "Meiryo")])])
                .space_line(None)
                .before(Some((true, 50_000.0)))
        })
        .collect();
    expect_within(&Stack::new(meiryo).compat_off(true), &[71.0, 234.0, 396.0]);
    // Arial with absolute 18 pt spacing and spcFirstLastPara="1".
    let arial: Vec<Para> = (0..3)
        .map(|_| {
            Para::of(vec![Line::named(vec![(40.0, "Arial")])])
                .space_line(None)
                .before(Some((false, 18.0)))
        })
        .collect();
    let stack = Stack::new(arial).compat_off(true).first_last(true);
    expect_within(&stack, &[77.0, 166.0, 255.0]);
}

/// `it('unions mixed faces without rescaling and anchors by the natural
/// descent')`.
#[test]
fn compat_off_unions_mixed_faces_and_anchors_by_the_natural_descent() {
    let mixed: Vec<Para> = (0..2)
        .map(|_| {
            Para::of(vec![Line::named(vec![(24.0, "Arial"), (60.0, "Meiryo")])]).space_line(None)
        })
        .collect();
    expect_within(&Stack::new(mixed).compat_off(true), &[107.0, 270.0]);

    // b anchor: the block is anchored by the last line's natural descent.
    let meiryo = Stack::new(lines("Meiryo", 40.0, 2, None))
        .compat_off(true)
        .in_box(0.0, 200.0, 0.0, 0.0)
        .anchor("b");
    expect_within(&meiryo, &[132.09, 241.13]);
    let arial = Stack::new(lines("Arial", 40.0, 2, None))
        .compat_off(true)
        .in_box(0.0, 200.0, 0.0, 0.0)
        .anchor("b");
    expect_within(&arial, &[202.02, 266.01]);
    // ctr anchor with 80 % line spacing.
    let centred = Stack::new(lines("Meiryo", 40.0, 2, Some((true, 80_000.0))))
        .compat_off(true)
        .in_box(0.0, 200.0, 0.0, 0.0)
        .anchor("ctr");
    expect_within(&centred, &[104.52, 191.52]);
}
