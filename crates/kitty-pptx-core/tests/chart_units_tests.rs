//! Chart text-size unit and bar cluster geometry parity tests.
//!
//! Reference: `/tmp/oooxml` @ 1f5c6e1d
//! - `packages/core/src/chart/layout.ts:146-158` (hpt are hundredths of a point)
//! - `packages/core/src/chart/shared/axis.ts:584` (axis label fallback)
//! - `packages/core/src/chart/shared/data-labels.ts:118-121` (data label fallback)
//! - `packages/core/src/chart/families/bar.ts:1289-1333` (gapWidth/overlap)

#![recursion_limit = "256"]

use crossbeam_channel::unbounded;
use kitty_pptx_core::contracts::{PathSegment, VectorCommand};
use kitty_pptx_core::{DefaultSlideCompiler, SlideCompiler};
use ooxml_common::chart::{ChartModel, ChartSeries, ChartSeriesDataLabels};
use pptx_model::{ChartElement, Slide, SlideElement};

const W: i64 = 1_000_000;
const H: i64 = 1_000_000;

fn series(name: &str, values: &[f64]) -> ChartSeries {
    ChartSeries {
        name: name.into(),
        color: Some("4472C4".into()),
        values: values.iter().copied().map(Some).collect(),
        ..ChartSeries::default()
    }
}

fn chart(chart_type: &str, categories: &[&str], series_values: &[&[f64]]) -> ChartModel {
    ChartModel {
        chart_type: chart_type.into(),
        categories: categories.iter().map(|c| (*c).to_string()).collect(),
        series: series_values
            .iter()
            .enumerate()
            .map(|(idx, values)| series(&format!("Series {}", idx + 1), values))
            .collect(),
        ..ChartModel::default()
    }
}

fn compile(chart: ChartModel, width: i64, height: i64) -> Vec<VectorCommand> {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();
    let elem = ChartElement {
        id: Some("chart".into()),
        x: 0,
        y: 0,
        width,
        height,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart,
    };
    let slide = Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: None,
        background: None,
        elements: vec![SlideElement::Chart(elem)],
        element_sources: vec![],
        notes: None,
        comments: vec![],
        hidden: false,
        parse_error: None,
    };
    compiler
        .compile(&slide, 10_000_000, 7_500_000, &tx)
        .expect("compile slide with chart")
        .commands
}

fn font_pt(commands: &[VectorCommand], text: &str) -> Option<f32> {
    commands.iter().find_map(|cmd| match cmd {
        VectorCommand::DrawText { runs, .. } => runs
            .iter()
            .find(|run| run.text == text)
            .map(|run| run.font_size_pt),
        _ => None,
    })
}

#[derive(Clone, Copy, Debug)]
struct Bar {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Clustered/stacked bars are the only filled, stroke-less rectangle paths in
/// these fixtures (no chart/plot background, no legend).
fn filled_bars(commands: &[VectorCommand]) -> Vec<Bar> {
    commands
        .iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawPath {
                path,
                fill: Some(_),
                stroke: None,
                ..
            } => match path.segments.as_slice() {
                [
                    PathSegment::MoveTo { x, y },
                    PathSegment::LineTo { x: right, .. },
                    PathSegment::LineTo { y: bottom, .. },
                    PathSegment::LineTo { .. },
                    PathSegment::Close,
                ] => Some(Bar {
                    x: *x,
                    y: *y,
                    w: right - x,
                    h: bottom - y,
                }),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    let tolerance = expected.abs().max(1.0) * 1e-4;
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual}"
    );
}

// ── Text sizes: DrawingML `ST_TextFontSize` is hundredths of a point ────────

#[test]
fn chart_title_hpt_is_hundredths_of_a_point() {
    let mut c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    c.title = Some("Revenue".into());
    c.title_font_size_hpt = Some(1600);

    let commands = compile(c, W, H);

    // Ref layout.ts:153: 1600 hpt / 100 = 16pt (was 1600 / 2.0 = 800pt).
    assert_eq!(font_pt(&commands, "Revenue"), Some(16.0));
}

#[test]
fn chart_title_hpt_outside_schema_range_falls_back_to_14pt() {
    let mut c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    c.title = Some("Revenue".into());
    // Ref layout.ts:149-153: valid ST_TextFontSize is 100..=400000 hpt.
    c.title_font_size_hpt = Some(50);

    let commands = compile(c, W, H);

    assert_eq!(font_pt(&commands, "Revenue"), Some(14.0));
}

#[test]
fn axis_label_hpt_is_hundredths_of_a_point() {
    let mut c = chart("clusteredColumn", &["Cat A"], &[&[100.0]]);
    c.cat_axis_font_size_hpt = Some(900);
    c.val_axis_font_size_hpt = Some(800);

    let commands = compile(c, W, H);

    assert_eq!(font_pt(&commands, "Cat A"), Some(9.0));
    // Nice ticks for 0..100 include 20, whose label must use the value-axis size.
    assert_eq!(font_pt(&commands, "20"), Some(8.0));
}

#[test]
fn data_label_hpt_is_hundredths_of_a_point_and_series_overrides_chart() {
    let mut c = chart("clusteredColumn", &["A"], &[&[42.5]]);
    c.show_data_labels = true;
    c.data_label_font_size_hpt = Some(1100);

    let commands = compile(c, W, H);
    assert_eq!(font_pt(&commands, "42.5"), Some(11.0));

    let mut c = chart("clusteredColumn", &["A"], &[&[42.5]]);
    c.show_data_labels = true;
    c.data_label_font_size_hpt = Some(1100);
    c.series[0].series_data_labels = Some(ChartSeriesDataLabels {
        font_size_hpt: Some(1200),
        ..ChartSeriesDataLabels::default()
    });

    let commands = compile(c, W, H);
    // Ref data-labels.ts:118-119: series `dLbls@sz` wins over chart-level.
    assert_eq!(font_pt(&commands, "42.5"), Some(12.0));
}

#[test]
fn legend_hpt_is_hundredths_of_a_point() {
    let mut c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    c.show_legend = true;
    c.legend_pos = Some("r".into());
    c.legend_font_size_hpt = Some(1000);
    c.series[0].name = "Alpha".into();

    let commands = compile(c, W, H);

    assert_eq!(font_pt(&commands, "Alpha"), Some(10.0));
}

#[test]
fn missing_sizes_use_reference_defaults() {
    let mut c = chart("clusteredColumn", &["Cat A"], &[&[42.5]]);
    c.title = Some("Revenue".into());
    c.show_legend = true;
    c.legend_pos = Some("r".into());
    c.show_data_labels = true;
    c.series[0].name = "Area".into();

    let commands = compile(c, W, H);

    // Ref layout.ts:157: fixed 14pt title fallback.
    assert_eq!(font_pt(&commands, "Revenue"), Some(14.0));
    // Ref resource.ts:33: 10pt legend fallback.
    assert_eq!(font_pt(&commands, "Area"), Some(10.0));
    // Ref axis.ts:584: max(8, h * 0.045) px; 1,000,000 EMU is ~105 px,
    // so the 8px floor applies -> 6pt at the 96 dpi host scale.
    assert_eq!(font_pt(&commands, "Cat A"), Some(6.0));
    // Ref data-labels.ts:121: max(9, min(11, ph / 25)) px. Here the plot is
    // 590,000 EMU (~62 px), so the 9px floor applies -> 6.75pt.
    assert_eq!(font_pt(&commands, "42.5"), Some(6.75));
}

// ── Bar cluster geometry: ECMA-376 gapWidth / overlap ──────────────────────

#[test]
fn default_gap_width_150_gives_bar_width_slot_over_2_5() {
    let c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    assert_eq!(bars.len(), 2);
    let slot = bars[1].x - bars[0].x;
    // Ref bar.ts:1328: denom = 1 + 0*(1-overlap) + gapWidth/100 = 2.5.
    assert_close(bars[0].w, slot / 2.5, "default bar width");
    // Ref bar.ts:1332: catStart = (slot - clusterWidth) / 2, cluster centered.
    assert_close(bars[0].x, 140_000.0 + (slot - bars[0].w) / 2.0, "cluster centering");
}

#[test]
fn authored_gap_width_controls_bar_width() {
    for (gap_width, denom) in [(100, 2.0_f32), (50, 1.5_f32), (300, 4.0_f32)] {
        let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
        c.bar_gap_width = Some(gap_width);

        let commands = compile(c, W, H);
        let bars = filled_bars(&commands);

        assert_eq!(bars.len(), 2, "gapWidth={gap_width}");
        let slot = bars[1].x - bars[0].x;
        assert_close(
            bars[0].w,
            slot / denom,
            &format!("gapWidth={gap_width} bar width"),
        );
    }
}

#[test]
fn bar_group_gap_width_wins_over_chart_level() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.bar_gap_width = Some(100);
    c.series[0].bar_group_gap_width = Some(400);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    let slot = bars[1].x - bars[0].x;
    // Ref bar.ts:1324-1326: owner.barGroupGapWidth ?? chart.barGapWidth.
    assert_close(bars[0].w, slot / 5.0, "group bar width");
}

#[test]
fn overlap_is_honoured_inside_the_cluster() {
    let mut c = chart(
        "clusteredColumn",
        &["A", "B"],
        &[&[10.0, 20.0], &[30.0, 40.0]],
    );
    c.bar_gap_width = Some(150);
    c.bar_overlap = Some(50);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    assert_eq!(bars.len(), 4);
    // Bars are emitted category-major: c0s0, c0s1, c1s0, c1s1.
    let slot = bars[2].x - bars[0].x;
    // Ref bar.ts:1328: denom = 1 + 1*(1-0.5) + 1.5 = 3.
    let bar_w = slot / 3.0;
    assert_close(bars[0].w, bar_w, "overlapped bar width");
    // Ref bar.ts:1330: clusterGap = barW * (1 - overlap/100) = barW / 2.
    assert_close(bars[1].x - bars[0].x, bar_w / 2.0, "overlap pitch");
    // Clusters stay centered: clusterWidth = barW + clusterGap = 1.5 * barW.
    assert_close(
        bars[0].x,
        140_000.0 + (slot - 1.5 * bar_w) / 2.0,
        "overlapped cluster centering",
    );
}

#[test]
fn stacked_columns_also_use_gap_width() {
    let c = chart("stackedColumn", &["A", "B"], &[&[10.0, 20.0]]);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    assert_eq!(bars.len(), 2);
    let slot = bars[1].x - bars[0].x;
    // Ref bar.ts:1321-1328: stacked treats N=1 and overlap=0, but gapWidth
    // still applies -> 1 + 0 + 1.5 = 2.5.
    assert_close(bars[0].w, slot / 2.5, "stacked bar width");
}

#[test]
fn horizontal_bars_use_gap_width_and_overlap() {
    let mut c = chart("clusteredBarH", &["A", "B"], &[&[10.0, 20.0]]);
    c.bar_gap_width = Some(150);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    assert_eq!(bars.len(), 2);
    let slot = bars[1].y - bars[0].y;
    assert_close(bars[0].h, slot / 2.5, "horizontal bar height");

    let mut c = chart(
        "clusteredBarH",
        &["A", "B"],
        &[&[10.0, 20.0], &[30.0, 40.0]],
    );
    c.bar_gap_width = Some(150);
    c.bar_overlap = Some(50);

    let commands = compile(c, W, H);
    let bars = filled_bars(&commands);

    assert_eq!(bars.len(), 4);
    let slot = bars[2].y - bars[0].y;
    let bar_h = slot / 3.0;
    assert_close(bars[0].h, bar_h, "horizontal overlapped bar height");
    assert_close(bars[1].y - bars[0].y, bar_h / 2.0, "horizontal overlap pitch");
}

fn texts(commands: &[VectorCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => {
                Some(runs.iter().map(|run| run.text.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn series_level_show_val_paints_labels_when_the_legacy_projection_is_false() {
    // The sample deck authors `<c:dLbls><c:showVal val="1"/>` on the series
    // while the chart-wide legacy projection stays false. Checking only
    // `chart.show_data_labels` used to drop every label.
    let mut c = chart("clusteredColumn", &["A"], &[&[42.5]]);
    c.show_data_labels = false;
    c.series[0].series_data_labels = Some(ChartSeriesDataLabels {
        show_val: true,
        ..ChartSeriesDataLabels::default()
    });

    let commands = compile(c, W, H);
    assert!(
        texts(&commands).iter().any(|text| text == "42.5"),
        "series showVal must paint its value label: {:?}",
        texts(&commands)
    );
}

#[test]
fn bounded_axis_uses_the_reference_auto_unit() {
    // Reference `planLinearValueAxis` on a vertical 60..72 span: one interval
    // per ~28 pt chooses a 2-unit step, not the old 5-unit ladder.
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[63.1, 69.1]]);
    c.val_min = Some(60.0);
    c.val_max = Some(72.0);

    let commands = compile(c, W, H);
    let labels = texts(&commands);
    for expected in ["60", "62", "70", "72"] {
        assert!(
            labels.iter().any(|text| text == expected),
            "expected value-axis tick {expected}: {labels:?}"
        );
    }
    assert!(
        !labels.iter().any(|text| text == "65"),
        "the 5-unit ladder must not survive: {labels:?}"
    );
}
