//! `percentage-line-spacing.test.ts`, `percentage-paragraph-spacing.test.ts`
//! and `norm-autofit-omitted-scale.test.ts` through the compiled text body.
//!
//! Reference (read-only @ `1f5c6e1d`): the three specs above plus
//! `renderer.ts:5351-5476` (natural box, `spaced_line_box`, the percentage
//! spacing unit, the `spcFirstLastPara` edges). Every probe compiles a real
//! `pptx_model::TextBody` through `text::emit::compile_text_body` and reads the
//! baselines back, so the assertions cover the kernels *and* the emit wiring
//! (`space_before_pct` / `space_after_pct` → `metrics::SpaceLine`,
//! `font_scale` → glyph size). The expected values are the specs' own, in
//! points (`scale = 1/12700` there means 1 canvas unit = 1 pt = 12700 EMU).
//!
//! What is not portable: `naturalWidthExceedsBbox` and the `normAutofit`
//! *derivation* (the core has no auto-scale search; it honours a stored
//! `fontScale` and nothing else) — see the coverage report.

#[path = "text_box_a_support/mod.rs"]
mod support;

use support::{
    body, body_with, close, compile, drawn_runs, line_pts, para, text, text_face, ParaSpec, BREAK,
};

/// `percentage-line-spacing.test.ts`: `baselinePitch(fontFamily, spaceLine)`.
///
/// Two 20 pt paragraphs, `wrap="square"`, 400 × 300 pt. The pitch is the
/// distance between the two baselines; the bullet marker shares its
/// paragraph's baseline, so it never adds one.
fn baseline_pitch(specs: Vec<ParaSpec>) -> f32 {
    let ys = line_pts(&compile(&body(&specs), 400.0, 300.0));
    assert_eq!(ys.len(), 2, "expected two lines, got {ys:?}");
    ys[1] - ys[0]
}

fn two_paragraphs(face: &'static str, space: Option<(bool, f32)>) -> Vec<ParaSpec> {
    vec![
        para(vec![text_face("第一段落", 20.0, face)]).space_line(space),
        para(vec![text_face("第二段落", 20.0, face)]).space_line(space),
    ]
}

fn bulleted(face: &'static str, space: Option<(bool, f32)>) -> Vec<ParaSpec> {
    vec![
        para(vec![text_face("第一段落", 20.0, face)])
            .space_line(space)
            .mar_l(342_900)
            .indent(-190_500)
            .bullet(),
        para(vec![text_face("第二段落", 20.0, face)])
            .space_line(space)
            .mar_l(342_900)
            .indent(-190_500)
            .bullet(),
    ]
}

const PCT100: Option<(bool, f32)> = Some((true, 100_000.0));

/// `it.each(['Meiryo UI', 'Sakkal Majalla'])('does not let the %s design-height
/// floor override an explicit 100%')`.
///
/// The design-height floor lived in the canvas `fontBoundingBox` metrics; the
/// #1610 model uses `1.2 × size` for every face, so an explicit 100 % is one
/// natural line whatever the face.
#[test]
fn explicit_100_pct_is_one_natural_line_for_every_face() {
    let arial = baseline_pitch(two_paragraphs("Arial", PCT100));
    close(arial, 24.0, "Arial 100 %");
    for face in ["Meiryo UI", "Sakkal Majalla"] {
        // Production substitutes where it must; the pitch is still exactly one
        // natural line of the largest run size.
        let pitch = baseline_pitch(two_paragraphs(face, PCT100));
        close(pitch, arial, face);
        close(pitch, 20.0 * 1.2, face);
    }
}

/// `it('applies the same explicit-percentage rule to bulleted paragraphs')`.
#[test]
fn bulleted_paragraphs_use_the_same_percentage_rule() {
    let pitch = baseline_pitch(bulleted("Arial", PCT100));
    close(pitch, baseline_pitch(two_paragraphs("Arial", PCT100)), "bulleted vs plain");
    close(pitch, 24.0, "bulleted 100 %");
}

/// `it.each(['Meiryo UI', 'Arial'])('uses the natural 120%% pitch for omitted
/// line spacing')`.
#[test]
fn omitted_line_spacing_uses_the_natural_120_pct_pitch() {
    for face in ["Meiryo UI", "Arial"] {
        for space in [None, Some((true, 100_000.0))] {
            close(baseline_pitch(two_paragraphs(face, space)), 20.0 * 1.2, face);
            close(baseline_pitch(bulleted(face, space)), 20.0 * 1.2, face);
        }
    }
}

/// `it('keeps absolute point line spacing independent of the font design
/// height')`.
#[test]
fn point_line_spacing_is_absolute() {
    let pts18 = Some((false, 18.0));
    let arial = baseline_pitch(two_paragraphs("Arial", pts18));
    close(arial, 18.0, "spcPts 18");
    close(
        baseline_pitch(bulleted("Meiryo UI", pts18)),
        arial,
        "spcPts 18, Meiryo UI bulleted",
    );
}

// ── percentage-paragraph-spacing.test.ts ───────────────────────────────────

/// `baselines(paragraphs, anchor, spcFirstLastPara)` of the spec: 400 × 400 pt,
/// no insets, `wrap="square"`.
fn baselines(specs: &[ParaSpec], anchor: &str, first_last: bool) -> Vec<f32> {
    let body = body_with(
        specs,
        anchor,
        (0.0, 0.0, 0.0, 0.0),
        "none",
        None,
        None,
        first_last,
        20.0,
    );
    line_pts(&compile(&body, 400.0, 400.0))
}

fn p(size: f64, before: Option<(bool, f32)>, after: Option<(bool, f32)>) -> ParaSpec {
    para(vec![text("A", size)]).before(before).after(after)
}

/// `it('adds a percentage of the following line before a paragraph')`.
#[test]
fn percentage_space_before_scales_the_following_line() {
    let plain = baselines(&[p(20.0, None, None), p(20.0, None, None)], "t", false);
    let pitch = plain[1] - plain[0];
    let ys = baselines(
        &[p(20.0, None, None), p(20.0, Some((true, 50_000.0)), None)],
        "t",
        false,
    );
    close(ys[1] - ys[0], pitch * 1.5, "50 % before");
}

/// `it('measures space before on the text size of the line it precedes')`.
#[test]
fn space_before_uses_the_line_it_precedes() {
    let large = baselines(&[p(20.0, None, None), p(40.0, None, None)], "t", false);
    let ys = baselines(
        &[p(20.0, None, None), p(40.0, Some((true, 100_000.0)), None)],
        "t",
        false,
    );
    close(ys[1] - ys[0] - (large[1] - large[0]), 40.0 * 1.2, "100 % of 40 pt");
}

/// `it('measures space before on the largest run of a mixed-size first line')`.
#[test]
fn space_before_uses_the_largest_run_of_the_first_line() {
    let mixed = || vec![text("B", 20.0), text("C", 40.0)];
    let without = baselines(
        &[
            para(vec![text("A", 40.0)]),
            para(mixed()),
        ],
        "t",
        false,
    );
    let ys = baselines(
        &[
            para(vec![text("A", 40.0)]),
            para(mixed()).before(Some((true, 100_000.0))),
        ],
        "t",
        false,
    );
    close(ys[1] - ys[0] - (without[1] - without[0]), 48.0, "100 % of 40 pt mixed");
}

/// `it('uses the first line for space before and the last line for space
/// after')`.
#[test]
fn first_line_owns_space_before_and_last_line_space_after() {
    let lines = |first: f32, last: f32, spaced: bool| {
        let gap = if spaced { Some((true, 100_000.0)) } else { None };
        vec![
            para(vec![text("A", 40.0)]),
            para(vec![text("B", first as f64), BREAK, text("C", last as f64)])
                .before(gap)
                .after(gap),
            para(vec![text("D", 40.0)]),
        ]
    };
    for (first, last) in [(20.0f32, 40.0f32), (40.0, 20.0)] {
        let plain = baselines(&lines(first, last, false), "t", false);
        let spaced = baselines(&lines(first, last, true), "t", false);
        close(spaced[1] - plain[1], first * 1.2, "space before");
        close(
            spaced[3] - spaced[2] - (plain[3] - plain[2]),
            last * 1.2,
            "space after",
        );
    }
}

/// `it('keeps the percentage unit independent of the paragraph line spacing')`.
#[test]
fn percentage_unit_ignores_the_paragraph_line_spacing() {
    let tight = Some((true, 80_000.0));
    let without = baselines(
        &[
            para(vec![text("A", 40.0)]).space_line(tight),
            para(vec![text("B", 40.0)]).space_line(tight),
        ],
        "t",
        false,
    );
    let ys = baselines(
        &[
            para(vec![text("A", 40.0)]).space_line(tight),
            para(vec![text("B", 40.0)])
                .space_line(tight)
                .before(Some((true, 100_000.0))),
        ],
        "t",
        false,
    );
    close(ys[1] - ys[0] - (without[1] - without[0]), 48.0, "80 % lnSpc");
}

/// `it('adds a percentage of the last line after a paragraph')`.
#[test]
fn percentage_space_after_scales_the_last_line() {
    let plain = baselines(&[p(20.0, None, None), p(20.0, None, None)], "t", false);
    let pitch = plain[1] - plain[0];
    let ys = baselines(
        &[p(20.0, None, Some((true, 20_000.0))), p(20.0, None, None)],
        "t",
        false,
    );
    close(ys[1] - ys[0], pitch * 1.2, "20 % after");
}

/// `it('keeps suppressing space before on the first paragraph')`.
#[test]
fn first_paragraph_space_before_stays_suppressed() {
    let plain = baselines(&[p(20.0, None, None), p(20.0, None, None)], "t", false);
    let ys = baselines(&[p(20.0, Some((true, 100_000.0)), None)], "t", false);
    close(ys[0], plain[0], "suppressed edge gap");
}

/// `it('keeps absolute point spacing unchanged')`.
#[test]
fn absolute_point_spacing_is_unchanged() {
    let plain = baselines(&[p(20.0, None, None), p(20.0, None, None)], "t", false);
    let ys = baselines(
        &[p(20.0, None, None), p(20.0, Some((false, 12.0)), None)],
        "t",
        false,
    );
    close(ys[1] - ys[0], plain[1] - plain[0] + 12.0, "spcBef 12 pt");
}

/// `it('suppresses the last paragraph space after unless spcFirstLastPara is
/// set')` (ECMA-376 §21.1.2.1.1, bottom anchor).
#[test]
fn last_space_after_needs_spc_first_last_para() {
    let bottom = baselines(&[p(20.0, None, None)], "b", false);
    let pct = baselines(&[p(20.0, None, Some((true, 50_000.0)))], "b", false);
    close(pct[0], bottom[0], "percentage edge gap suppressed");
    let points = baselines(&[p(20.0, None, Some((false, 12.0)))], "b", false);
    close(points[0], bottom[0], "point edge gap suppressed");
    let respected = baselines(&[p(20.0, None, Some((false, 12.0)))], "b", true);
    close(bottom[0] - respected[0], 12.0, "respected point edge gap");
}

/// `it('applies the first paragraph space before when spcFirstLastPara is
/// set')`.
#[test]
fn first_space_before_applies_with_spc_first_last_para() {
    let plain = baselines(&[p(20.0, None, None), p(20.0, None, None)], "t", false);
    let ys = baselines(&[p(20.0, Some((false, 12.0)), None)], "t", true);
    close(ys[0] - plain[0], 12.0, "first paragraph gap");
}

/// `it('suppresses centre-anchored trailing point and percentage gaps when
/// omitted or false')`.
#[test]
fn centre_anchor_suppresses_trailing_gaps() {
    let base = baselines(&[p(20.0, None, None)], "ctr", false)[0];
    for after in [Some((false, 12.0)), Some((true, 50_000.0))] {
        let spaced = [p(20.0, None, after)];
        close(baselines(&spaced, "ctr", false)[0], base, "explicit false");
        let respected = baselines(&spaced, "ctr", true)[0];
        close(base - respected, 6.0, "half the authored gap");
    }
}

/// `it('keeps spcAft before an empty final paragraph as an interior gap')`.
#[test]
fn space_after_before_an_empty_paragraph_is_interior() {
    let empty = || para(vec![text("", 20.0)]);
    let without = baselines(&[p(20.0, None, None), empty()], "ctr", false);
    let with_gap = baselines(
        &[p(20.0, None, Some((false, 12.0))), empty()],
        "ctr",
        false,
    );
    close(without[0] - with_gap[0], 6.0, "interior gap lifts by half");
}

// ── norm-autofit-omitted-scale.test.ts ─────────────────────────────────────

/// `paintedSizes(text, boxHeight)` from the spec: the glyph size actually
/// painted, in points.
fn painted_sizes(specs: &[ParaSpec], box_height: f64, font_scale: Option<f64>) -> Vec<f32> {
    let body = body_with(
        specs,
        "t",
        (0.0, 0.0, 0.0, 0.0),
        "norm",
        font_scale,
        None,
        false,
        24.0,
    );
    let mut sizes: Vec<f32> = drawn_runs(&compile(&body, 400.0, box_height))
        .iter()
        .map(|run| run.font_size_pt)
        .collect();
    sizes.dedup();
    sizes
}

fn norm_paragraphs() -> Vec<ParaSpec> {
    (0..3)
        .map(|_| para(vec![text("Hamburger", 24.0)]))
        .collect()
}

/// `it('keeps authored glyph size through slight and large overflow')`.
#[test]
fn omitted_font_scale_keeps_the_authored_size() {
    for height in [140.0, 76.0, 44.0] {
        assert_eq!(painted_sizes(&norm_paragraphs(), height, None), vec![24.0], "{height} pt box");
    }
}

/// `it('does not derive a scale from percentage line or paragraph spacing')`.
#[test]
fn omitted_font_scale_ignores_spacing() {
    let mut specs = norm_paragraphs();
    specs[0] = specs[0].clone().space_line(Some((true, 120_000.0)));
    specs[2] = specs[2].clone().after(Some((true, 50_000.0)));
    let body = body_with(
        &specs,
        "t",
        (0.0, 0.0, 0.0, 0.0),
        "norm",
        None,
        None,
        true,
        24.0,
    );
    let sizes: Vec<f32> = drawn_runs(&compile(&body, 400.0, 44.0))
        .iter()
        .map(|run| run.font_size_pt)
        .collect();
    assert!(sizes.iter().all(|size| (size - 24.0).abs() < 1e-3), "{sizes:?}");
}

/// `it('uses a stored scale even when it does not match the content')`.
#[test]
fn stored_font_scale_is_honoured() {
    assert_eq!(
        painted_sizes(&norm_paragraphs(), 140.0, Some(0.5)),
        vec![12.0]
    );
}
