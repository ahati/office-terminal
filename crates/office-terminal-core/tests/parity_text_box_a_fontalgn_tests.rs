//! `powerpoint-line-metrics.test.ts` — `pPr@fontAlgn` and line-break marks
//! (#1636).
//!
//! `fontAlgn` `t` / `ctr` / `b` place each run's box at the line's alignment
//! reference instead of on a baseline, and every offset is a whole export unit
//! (1/100 in = 0.72 pt at the specs' scale). The `a:br` and `endParaRPr` marks
//! contribute their face at the size of the run they follow, so a 16 pt or
//! 80 pt mark never changes the line.
//!
//! Reference (read-only @ `1f5c6e1d`):
//! - `packages/pptx/src/powerpoint-line-metrics.ts:358-469`
//!   (`powerPointFontAlgnReference`, `powerPointFontAlgnOffset`,
//!   `POWERPOINT_FONT_ALGN_UNIT_PT = 0.72`)
//! - `packages/pptx/src/renderer.ts:5410-5443`, `5786-5793`, `5982`
//! - `packages/pptx/src/powerpoint-line-metrics.test.ts:301-467`
//!
//! Values are the exported baselines of every run in 1/100 in from the text
//! area top, `[compatLnSpc="0", compatLnSpc="1"]`. The export rounds each
//! continuous position to that unit, so every one must lie within half a unit
//! (a few controls land exactly on a rounding tie, hence the tolerance rather
//! than an integer comparison).

#[path = "text_box_a_support/mod.rs"]
mod support;

use office_terminal_core::text::metrics::SpaceLine;
use office_terminal_core::text::power_point::FontAlgn;
use support::{Line, Para, Stack};

fn spacing(tag: &str) -> SpaceLine {
    if let Some(percent) = tag.strip_prefix("pct") {
        let value: f32 = percent.parse().expect("percent value");
        Some((true, value * 1000.0))
    } else {
        let value: f32 = tag.strip_prefix("pts").expect("pts tag").parse().expect("value");
        Some((false, value))
    }
}

fn algn(value: &str) -> FontAlgn {
    FontAlgn::of(value).expect("t / ctr / b")
}

/// `expectWithin`: the continuous run baselines against the exported ones.
///
/// The reference computes in f64; this port keeps layout in f32 (the repo's
/// contract), so a position that lands exactly on a rounding tie can sit a
/// few 1e-5 units past `±0.5`. `TIE` widens the band for the two cases the
/// reference itself documents as `0.004` past a tie (`TIE = 0.51` there).
fn expect_within_tol(stack: &Stack, exported: &[f32], tolerance: f32) {
    let ys = stack.run_baselines();
    assert_eq!(ys.len(), exported.len(), "run count");
    for (index, y) in ys.iter().enumerate() {
        let unit = y / support::UNIT;
        assert!(
            (unit - exported[index]).abs() <= tolerance,
            "run {index}: {unit} vs {}",
            exported[index]
        );
    }
}

/// `expectWithin` with the f32 headroom for exact rounding ties.
fn expect_within(stack: &Stack, exported: &[f32]) {
    expect_within_tol(stack, exported, 0.5 + 1e-3);
}

/// `TIE` (`powerpoint-line-metrics.test.ts:452`): the inherited-mark controls
/// land 0.004 unit past a rounding tie.
fn expect_within_tie(stack: &Stack, exported: &[f32]) {
    expect_within_tol(stack, exported, 0.51);
}

/// Two one-line 40 pt paragraphs in `font`, `fontAlgn`, line spacing `tag`.
fn two_lines(font: &'static str, font_algn: &str, tag: &str) -> Stack {
    let paragraphs: Vec<Para> = (0..2)
        .map(|_| {
            Para::of(vec![Line::named(vec![(40.0, font)])])
                .space_line(spacing(tag))
                .font_algn(algn(font_algn))
        })
        .collect();
    Stack::new(paragraphs)
}

/// `it.each(sweep)` — 34 rows: 40 pt Arial / Meiryo, `fontAlgn` t / ctr / b,
/// `lnSpc` percentage and point values around both models' natural height.
/// One sweep row: face, `fontAlgn`, `lnSpc` tag, the `compatLnSpc="0"` and
/// `compatLnSpc="1"` exported run baselines.
type SweepRow = (&'static str, &'static str, &'static str, [f32; 2], [f32; 2]);

#[test]
fn font_algn_sweep_matches_the_export() {
    const SWEEP: [SweepRow; 34] = [
        ("Arial", "t", "pct90", [47.0, 105.0], [45.0, 105.0]),
        ("Arial", "t", "pct99", [52.0, 115.0], [49.0, 116.0]),
        ("Arial", "t", "pct100", [52.0, 116.0], [50.0, 117.0]),
        ("Arial", "t", "pct101", [53.0, 117.0], [51.0, 118.0]),
        ("Arial", "t", "pct110", [58.0, 129.0], [57.0, 130.0]),
        ("Arial", "t", "pts44", [50.0, 111.0], [46.0, 107.0]),
        ("Arial", "t", "pts47", [53.0, 119.0], [49.0, 114.0]),
        ("Arial", "t", "pts52", [60.0, 133.0], [56.0, 128.0]),
        ("Arial", "ctr", "pct90", [47.0, 105.0], [47.0, 107.0]),
        ("Arial", "ctr", "pct100", [52.0, 116.0], [52.0, 119.0]),
        ("Arial", "ctr", "pct101", [53.0, 117.0], [53.0, 120.0]),
        ("Arial", "ctr", "pct110", [58.0, 129.0], [59.0, 132.0]),
        ("Arial", "ctr", "pts46", [52.0, 116.0], [50.0, 114.0]),
        ("Arial", "b", "pct90", [46.0, 103.0], [48.0, 108.0]),
        ("Arial", "b", "pct100", [52.0, 116.0], [54.0, 121.0]),
        ("Arial", "b", "pct101", [42.0, 107.0], [45.0, 112.0]),
        ("Arial", "b", "pct110", [47.0, 117.0], [49.0, 122.0]),
        ("Arial", "b", "pts46", [42.0, 106.0], [52.0, 115.0]),
        ("Arial", "b", "pts52", [48.0, 120.0], [48.0, 120.0]),
        ("Meiryo", "t", "pct90", [63.0, 160.0], [54.0, 114.0]),
        ("Meiryo", "t", "pct100", [71.0, 179.0], [59.0, 126.0]),
        ("Meiryo", "t", "pct101", [72.0, 182.0], [60.0, 127.0]),
        ("Meiryo", "t", "pts44", [36.0, 97.0], [55.0, 116.0]),
        ("Meiryo", "t", "pts80", [74.0, 185.0], [103.0, 215.0]),
        ("Meiryo", "ctr", "pct90", [63.0, 161.0], [45.0, 105.0]),
        ("Meiryo", "ctr", "pct100", [71.0, 180.0], [50.0, 117.0]),
        ("Meiryo", "ctr", "pct110", [82.0, 201.0], [57.0, 130.0]),
        ("Meiryo", "ctr", "pts76", [69.0, 175.0], [89.0, 195.0]),
        ("Meiryo", "b", "pct99", [71.0, 178.0], [44.0, 110.0]),
        ("Meiryo", "b", "pct100", [72.0, 180.0], [45.0, 111.0]),
        ("Meiryo", "b", "pct101", [64.0, 173.0], [39.0, 106.0]),
        ("Meiryo", "b", "pts44", [28.0, 89.0], [39.0, 100.0]),
        ("Meiryo", "b", "pts52", [36.0, 108.0], [42.0, 114.0]),
        ("Meiryo", "b", "pts80", [65.0, 176.0], [71.0, 182.0]),
    ];
    for (font, font_algn, tag, compat0, compat1) in SWEEP {
        expect_within(&two_lines(font, font_algn, tag).compat_off(true), &compat0);
        expect_within(&two_lines(font, font_algn, tag), &compat1);
    }
}

/// `it('aligns the descent midpoints of 3-4 sizes under fontAlgn b')`.
#[test]
fn font_algn_b_aligns_descent_midpoints_of_mixed_runs() {
    const ARIAL4: [(f32, &str); 4] = [(20.0, "Arial"), (30.0, "Arial"), (45.0, "Arial"), (60.0, "Arial")];
    const MEI_ARIAL4: [(f32, &str); 4] = [
        (20.0, "Meiryo"),
        (30.0, "Arial"),
        (45.0, "Meiryo"),
        (60.0, "Arial"),
    ];
    const YU3: [(f32, &str); 3] = [(20.0, "Yu Gothic"), (30.0, "Yu Gothic"), (45.0, "Yu Gothic")];
    // Two one-line paragraphs under fontAlgn b; `space` = its line spacing.
    let stack = |spec: &[(f32, &'static str)], space: SpaceLine| -> Stack {
        let paragraphs: Vec<Para> = (0..2)
            .map(|_| {
                Para::of(vec![Line::named(spec.to_vec())])
                    .space_line(space)
                    .font_algn(FontAlgn::Bottom)
            })
            .collect();
        Stack::new(paragraphs)
    };
    expect_within(
        &stack(&ARIAL4, None).compat_off(true),
        &[84.0, 83.0, 80.0, 78.0, 180.0, 179.0, 176.0, 174.0],
    );
    expect_within(
        &stack(&ARIAL4, None),
        &[87.0, 86.0, 83.0, 81.0, 187.0, 186.0, 183.0, 181.0],
    );
    expect_within(
        &stack(&ARIAL4, spacing("pct80")).compat_off(true),
        &[65.0, 64.0, 61.0, 59.0, 141.0, 140.0, 137.0, 135.0],
    );
    expect_within(
        &stack(&ARIAL4, spacing("pct80")),
        &[67.0, 66.0, 63.0, 61.0, 147.0, 146.0, 143.0, 141.0],
    );
    expect_within(
        &stack(&ARIAL4, spacing("pct120")).compat_off(true),
        &[83.0, 82.0, 79.0, 77.0, 198.0, 197.0, 194.0, 192.0],
    );
    expect_within(
        &stack(&ARIAL4, spacing("pct120")),
        &[87.0, 86.0, 83.0, 81.0, 207.0, 206.0, 203.0, 201.0],
    );
    expect_within(
        &stack(&MEI_ARIAL4, None).compat_off(true),
        &[92.0, 97.0, 80.0, 92.0, 214.0, 219.0, 202.0, 214.0],
    );
    expect_within(
        &stack(&MEI_ARIAL4, None),
        &[83.0, 85.0, 75.0, 80.0, 183.0, 185.0, 175.0, 180.0],
    );
    expect_within(
        &stack(&MEI_ARIAL4, spacing("pct120")).compat_off(true),
        &[101.0, 106.0, 89.0, 101.0, 247.0, 252.0, 235.0, 247.0],
    );
    expect_within(
        &stack(&MEI_ARIAL4, spacing("pct120")),
        &[84.0, 86.0, 76.0, 81.0, 204.0, 206.0, 196.0, 201.0],
    );
    expect_within(
        &stack(&YU3, spacing("pct80")).compat_off(true),
        &[61.0, 58.0, 53.0, 145.0, 142.0, 137.0],
    );
    expect_within(
        &stack(&YU3, spacing("pct80")),
        &[47.0, 45.0, 42.0, 107.0, 105.0, 102.0],
    );
}

/// A two-line paragraph: `first` and `second` are the runs of line 1 and 2.
fn broken_lines(
    first: Vec<(f32, &'static str)>,
    second: Vec<(f32, &'static str)>,
    mark: Option<(f32, &'static str)>,
    end: Option<(f32, &'static str)>,
) -> Stack {
    let line1 = {
        let line = Line::named(first);
        match mark {
            Some((size, family)) => line.with_mark(size, family),
            None => line,
        }
    };
    let line2 = {
        let line = Line::named(second);
        match end {
            Some((size, family)) => line.with_mark(size, family),
            None => line,
        }
    };
    let para = Para::of(vec![line1, line2]).space_line(None);
    Stack::new(vec![para])
}

/// `it('sizes an a:br face at the preceding run size')` — R-* / E-*: the
/// break's own 16 / 80 pt size never matters (the emitter resolves the mark at
/// the size of the run it follows, `renderer.ts:5341-5346`).
#[test]
fn br_face_is_sized_at_the_preceding_run() {
    let run = |_authored_mark_size: f32| {
        broken_lines(
            vec![(24.0, "Meiryo"), (60.0, "Arial")],
            vec![(24.0, "Meiryo"), (60.0, "Arial")],
            Some((60.0, "Meiryo")),
            Some((60.0, "Arial")),
        )
    };
    for authored in [16.0, 80.0] {
        expect_within(&run(authored).compat_off(true), &[107.0, 107.0, 241.0, 241.0]);
        expect_within(&run(authored), &[73.0, 73.0, 181.0, 181.0]);
    }
    let arial_break = |_authored_mark_size: f32| {
        broken_lines(
            vec![(24.0, "Arial"), (60.0, "Meiryo")],
            vec![(24.0, "Arial"), (60.0, "Meiryo")],
            Some((60.0, "Arial")),
            Some((60.0, "Meiryo")),
        )
    };
    expect_within(
        &arial_break(16.0).compat_off(true),
        &[107.0, 107.0, 270.0, 270.0],
    );
    expect_within(&arial_break(16.0), &[73.0, 73.0, 171.0, 171.0]);
    let arial_only = |_authored_mark_size: f32| {
        broken_lines(
            vec![(60.0, "Arial"), (24.0, "Arial")],
            vec![(60.0, "Arial"), (24.0, "Arial")],
            Some((24.0, "Arial")),
            Some((24.0, "Arial")),
        )
    };
    expect_within(
        &arial_only(80.0).compat_off(true),
        &[78.0, 78.0, 174.0, 174.0],
    );
    expect_within(&arial_only(80.0), &[81.0, 81.0, 181.0, 181.0]);
}

/// Two one-line paragraphs with an `endParaRPr` mark on each.
fn end_marked(runs: Vec<(f32, &'static str)>, end: Option<(f32, &'static str)>) -> Stack {
    let paragraphs: Vec<Para> = (0..2)
        .map(|_| {
            let line = Line::named(runs.clone());
            let line = match end {
                Some((size, family)) => line.with_mark(size, family),
                None => line,
            };
            Para::of(vec![line]).space_line(None)
        })
        .collect();
    Stack::new(paragraphs)
}

/// `it('sizes an endParaRPr face at the last run size')`.
#[test]
fn end_para_face_is_sized_at_the_last_run() {
    let meiryo_end = end_marked(
        vec![(24.0, "Meiryo"), (60.0, "Arial")],
        Some((60.0, "Meiryo")),
    );
    expect_within(&meiryo_end.clone().compat_off(true), &[107.0, 107.0, 270.0, 270.0]);
    expect_within(&meiryo_end, &[73.0, 73.0, 173.0, 173.0]);
    let arial_end = end_marked(
        vec![(24.0, "Arial"), (60.0, "Meiryo")],
        Some((60.0, "Arial")),
    );
    expect_within(&arial_end.clone().compat_off(true), &[107.0, 107.0, 270.0, 270.0]);
    expect_within(&arial_end, &[73.0, 73.0, 173.0, 173.0]);
    let arial_only = end_marked(
        vec![(60.0, "Arial"), (24.0, "Arial")],
        Some((24.0, "Arial")),
    );
    expect_within(&arial_only.clone().compat_off(true), &[78.0, 78.0, 174.0, 174.0]);
    expect_within(&arial_only, &[81.0, 81.0, 181.0, 181.0]);
}

/// `it('adds an inherited endParaRPr face at the last run size')` (#1663): a
/// mark without a face of its own contributes the face it inherits (the
/// Meiryo paragraph default) at the last run's size; an omitted mark
/// contributes nothing.
#[test]
fn inherited_end_para_face_uses_the_last_run_size() {
    let runs = vec![(60.0, "Arial"), (24.0, "Arial")];
    for _authored in [16.0, 40.0, 80.0] {
        // The mark inherits the Meiryo face at the last run's 24 pt size; the
        // authored 16 / 40 / 80 pt never reaches the line box.
        let marked = end_marked(runs.clone(), Some((24.0, "Meiryo")));
        expect_within_tie(&marked.clone().compat_off(true), &[78.0, 78.0, 179.0, 179.0]);
        expect_within_tie(&marked, &[81.0, 81.0, 181.0, 181.0]);
    }
    let omitted = end_marked(runs.clone(), None);
    expect_within(&omitted.clone().compat_off(true), &[78.0, 78.0, 174.0, 174.0]);
    expect_within(&omitted, &[81.0, 81.0, 181.0, 181.0]);
    // The same with fontAlgn b: the mark inherits Meiryo, the runs are Arial.
    let aligned: Vec<Para> = (0..2)
        .map(|_| {
            Para::of(vec![Line::named(runs.clone()).with_mark(24.0, "Meiryo")])
                .space_line(None)
                .font_algn(FontAlgn::Bottom)
        })
        .collect();
    let aligned = Stack::new(aligned);
    expect_within(&aligned.clone().compat_off(true), &[78.0, 83.0, 176.0, 181.0]);
    expect_within(&aligned, &[81.0, 86.0, 181.0, 186.0]);
}

/// `it('adds an inherited a:br face at the preceding run size')`.
#[test]
fn inherited_br_face_uses_the_preceding_run_size() {
    let ari = vec![(60.0, "Arial"), (24.0, "Arial")];
    for _authored in [None, Some(16.0), Some(80.0)] {
        // The break inherits Meiryo at the preceding 24 pt run size.
        let stack = broken_lines(
            ari.clone(),
            ari.clone(),
            Some((24.0, "Meiryo")),
            Some((24.0, "Arial")),
        );
        expect_within_tie(&stack.clone().compat_off(true), &[78.0, 78.0, 179.0, 179.0]);
        expect_within_tie(&stack, &[81.0, 81.0, 181.0, 181.0]);
    }
}
