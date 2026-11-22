//! Measured chart-frame parity tests.
//!
//! Reference: `/tmp/oooxml` @ 1f5c6e1d
//! - `packages/core/src/chart/layout.ts:236-330` (measured legend reserve, title band)
//! - `packages/core/src/chart/layout.ts:455-524` (`cartesianTitleBand`, axis titles)
//! - `packages/core/src/chart/layout.ts:674-799` (`resolveManualLayoutRect`,
//!   `computeChartFrame`)
//! - `packages/core/src/chart/shared/title.ts:106-126` (`measuredCartesianTitleBand`)
//! - `packages/core/src/chart/shared/legend.ts:950` (`measuredLegendReserve`)

#![recursion_limit = "256"]

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::charts::layout::{compute_chart_layout, text_width};
use office_terminal_core::compiler::charts::types::{detect_chart_family, ChartFamily};
use office_terminal_core::contracts::VectorCommand;
use office_terminal_core::{DefaultSlideCompiler, SlideCompiler};
use ooxml_common::chart::{ChartManualLayout, ChartModel, ChartSeries};
use pptx_model::{ChartElement, Slide, SlideElement};

/// One CSS pixel in EMU at the reference 96 dpi host scale.
const PX: f32 = 9525.0;
const W_PX: f32 = 400.0;
const H_PX: f32 = 300.0;

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

fn frame(c: &ChartModel) -> office_terminal_core::compiler::charts::layout::ChartLayout {
    let family = detect_chart_family(c);
    compute_chart_layout(c, W_PX * PX, H_PX * PX, family)
}

fn compile(c: ChartModel, width: i64, height: i64) -> Vec<VectorCommand> {
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
        chart: c,
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

fn text_runs(commands: &[VectorCommand], text: &str) -> Vec<(f32, [u8; 4])> {
    commands
        .iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => Some(runs),
            _ => None,
        })
        .flatten()
        .filter(|run| run.text == text)
        .map(|run| (run.font_size_pt, run.color))
        .collect()
}

// ── Title band ──────────────────────────────────────────────────────────────

#[test]
fn title_band_is_font_plus_pads_and_pushes_the_plot_down() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.title = Some("Revenue".into());
    c.title_font_size_hpt = Some(1600);

    let layout = frame(&c);
    let font_emu = 16.0 * 12700.0;

    // ref layout.ts:287-306: bandH = fontPx + topPad + bottomPad.
    assert!(layout.title.band_h >= font_emu, "band {band_h}, font {font_emu}", band_h = layout.title.band_h);
    assert!(
        (layout.title.band_h - (font_emu + layout.title.top_pad + layout.title.bottom_pad)).abs() < 1.0,
        "band must equal font + pads"
    );
    // ref layout.ts:293-306: the draw offset is 0.62 x font, below the top pad.
    assert!(layout.title.top_pad > 0.0 && layout.title.top_pad <= font_emu * 0.62 + 1.0);
    // The plot starts below the reserved band, never under the title.
    assert!(
        layout.plot_y >= layout.title.band_h,
        "plot_y {} vs band {}",
        layout.plot_y,
        layout.title.band_h
    );

    // Without a title the band collapses and the plot starts at the 2 % pad.
    let mut plain = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    plain.title = None;
    let bare = frame(&plain);
    assert!(bare.title.is_empty());
    assert!(bare.plot_y < layout.plot_y);
}

#[test]
fn long_title_is_elided_to_the_available_width() {
    let long = "Quarterly revenue by region, product line and channel for the fiscal year";
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.title = Some(long.into());
    c.show_legend = true;
    c.legend_pos = Some("r".into());
    c.series[0].name = "Net revenue".into();

    let layout = frame(&c);
    let reserve = layout.legend.as_ref().expect("legend").reserve_w;
    // Plain titles paint across the frame in the reference
    // (shared/title.ts:150-170), so the elision bound is w minus the legend.
    let avail = layout.w - reserve;
    let font_emu = 14.0 * 12700.0;

    assert_ne!(layout.title.text, long, "long title must be truncated");
    assert!(layout.title.text.ends_with('\u{2026}'), "elided text: {:?}", layout.title.text);
    assert!(
        text_width(long, font_emu, false, false) > avail,
        "fixture must overflow the band"
    );
    assert!(
        text_width(&layout.title.text, font_emu, false, false) <= avail + 1.0,
        "elided title must fit the band"
    );

    // The elided text is what reaches the display list.
    let commands = compile(c, (W_PX * PX) as i64, (H_PX * PX) as i64);
    let drawn: Vec<String> = commands
        .iter()
        .filter_map(|cmd| match cmd {
            VectorCommand::DrawText { runs, .. } => Some(runs.iter().map(|r| r.text.clone())),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(drawn.iter().any(|t| t.ends_with('\u{2026}')), "title run must be elided");
    assert!(!drawn.iter().any(|t| t == long));
}

// ── Legend reserve ──────────────────────────────────────────────────────────

#[test]
fn side_legend_reduces_plot_width_by_the_measured_reserve() {
    let mut without = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    without.show_legend = false;
    without.legend_pos = None;
    let bare = frame(&without);

    let mut with = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    with.show_legend = true;
    with.legend_pos = Some("r".into());
    with.series[0].name = "Extremely long series name that cannot fit the band".into();
    let layout = frame(&with);

    let leg = layout.legend.as_ref().expect("legend reserve");
    // ref layout.ts:245-253: the side band is capped at 30 % of the frame.
    assert!(leg.reserve_w > 0.0);
    assert!(
        leg.reserve_w <= layout.w * 0.30 + 1.0,
        "reserve {} exceeds the 30 % cap",
        leg.reserve_w
    );
    assert!((leg.reserve_w - layout.w * 0.30).abs() < 1.0, "long name hits the cap");
    // The plot loses exactly the measured reserve and nothing else.
    assert!(
        (bare.plot_w - layout.plot_w - leg.reserve_w).abs() < 1.0,
        "plot width reduction {} != reserve {}",
        bare.plot_w - layout.plot_w,
        leg.reserve_w
    );
    // The band is measured from the entry (text + key + gap), so a short name
    // reserves less than a long one.
    let mut short = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    short.show_legend = true;
    short.legend_pos = Some("r".into());
    short.series[0].name = "A".into();
    let short_layout = frame(&short);
    let short_reserve = short_layout.legend.as_ref().expect("legend").reserve_w;
    assert!(short_reserve < leg.reserve_w);
    assert!(short_reserve >= 80.0 * PX - 1.0, "80 px minimum side band");
}

#[test]
fn legend_that_cannot_fit_is_dropped() {
    let mut c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    c.show_legend = true;
    c.legend_pos = Some("r".into());
    c.series[0].name = "Alpha".into();

    // A 26 px wide frame cannot hold even one legend key plus a glyph.
    let layout = compute_chart_layout(
        &c,
        26.0 * PX,
        120.0 * PX,
        detect_chart_family(&c),
    );
    assert!(layout.legend.is_none(), "legend must be dropped");
    assert!(!layout.show_legend);
    assert!(layout.legend_box.is_none());

    let commands = compile(c, (26.0 * PX) as i64, (120.0 * PX) as i64);
    assert!(commands
        .iter()
        .all(|cmd| !matches!(cmd, VectorCommand::DrawText { runs, .. }
            if runs.iter().any(|r| r.text == "Alpha"))));
}

#[test]
fn top_legend_reserves_measured_rows_and_never_more_than_30_percent() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.show_legend = true;
    c.legend_pos = Some("t".into());
    c.series = (0..12)
        .map(|idx| series(&format!("Series number {idx}"), &[1.0, 2.0]))
        .collect();

    let layout = frame(&c);
    let leg = layout.legend.as_ref().expect("legend reserve");
    assert_eq!(leg.side, "t");
    assert!(leg.reserve_h > 0.0);
    // ref layout.ts:255-266: packed rows, capped at 30 % of the chart height.
    assert!(
        leg.reserve_h <= layout.h * 0.30 + 1.0,
        "reserve {} exceeds the 30 % cap",
        leg.reserve_h
    );
    assert!(layout.plot_y >= layout.title.band_h + leg.reserve_h - 1.0);
}

// ── Axis titles ─────────────────────────────────────────────────────────────

#[test]
fn axis_titles_reserve_bands_and_paint_with_authored_style() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.cat_axis_title = Some("Quarter".into());
    c.cat_axis_title_font_size_hpt = Some(1400);
    c.val_axis_title = Some("Revenue".into());
    c.val_axis_title_font_size_hpt = Some(1200);
    c.val_axis_title_font_color = Some("7030A0".into());

    let layout = frame(&c);
    let cat = layout.cat_title.as_ref().expect("category title band");
    let val = layout.val_title.as_ref().expect("value title band");
    assert!((cat.font_pt - 14.0).abs() < 1e-4);
    assert!((val.font_pt - 12.0).abs() < 1e-4);
    // ref layout.ts:497-524: fontPx + margin + 4.
    assert!(cat.thickness > 14.0 * 12700.0);
    assert!(val.thickness > 12.0 * 12700.0);
    assert!(layout.plot_y + layout.plot_h <= layout.h - cat.thickness + 1.0);

    let bare = frame(&chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]));
    assert!(layout.plot_h < bare.plot_h, "category band must shrink the plot");
    assert!(layout.plot_w < bare.plot_w, "value band must shrink the plot");

    let commands = compile(c, (W_PX * PX) as i64, (H_PX * PX) as i64);
    // The horizontal category title is one run with the authored size/colour.
    let cat_runs = text_runs(&commands, "Quarter");
    assert_eq!(cat_runs.len(), 1);
    assert!((cat_runs[0].0 - 14.0).abs() < 1e-4);

    // The rotated value title is emitted glyph-per-line (the run contract has
    // no rotation); every glyph keeps the authored size and colour.
    let purple = [0x70, 0x30, 0xA0, 255];
    let mut vertical = String::new();
    for ch in "Revenue".chars() {
        let runs = text_runs(&commands, &ch.to_string());
        let (size, color) = runs
            .iter()
            .find(|(size, _)| (*size - 12.0).abs() < 1e-4)
            .copied()
            .unwrap_or_else(|| panic!("missing {ch:?} run at 12 pt"));
        assert!((size - 12.0).abs() < 1e-4);
        assert_eq!(color, purple, "value title colour");
        vertical.push(ch);
    }
    assert_eq!(vertical, "Revenue");
}

// ── Manual plot-area layout ─────────────────────────────────────────────────

#[test]
fn manual_layout_factor_places_the_plot_at_the_centre_half() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.title = Some("Revenue".into());
    c.plot_area_manual_layout = Some(ChartManualLayout {
        x_mode: "factor".into(),
        y_mode: "factor".into(),
        w_mode: "factor".into(),
        h_mode: "factor".into(),
        layout_target: Some("inner".into()),
        x: 0.25,
        y: 0.25,
        w: Some(0.5),
        h: Some(0.5),
    });

    let layout = frame(&c);
    assert!(layout.manual_layout_applied);
    // ref layout.ts:674-702: factor x/y are chart-space fractions.
    assert!((layout.plot_x - layout.w * 0.25).abs() < 1.0);
    assert!((layout.plot_y - layout.h * 0.25).abs() < 1.0);
    assert!((layout.plot_w - layout.w * 0.5).abs() < 1.0);
    assert!((layout.plot_h - layout.h * 0.5).abs() < 1.0);
}

#[test]
fn manual_layout_edge_modes_resolve_right_and_bottom_coordinates() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.plot_area_manual_layout = Some(ChartManualLayout {
        x_mode: "edge".into(),
        y_mode: "edge".into(),
        w_mode: "edge".into(),
        h_mode: "edge".into(),
        layout_target: Some("inner".into()),
        x: 0.1,
        y: 0.2,
        w: Some(0.6),
        h: Some(0.8),
    });

    let layout = frame(&c);
    assert!(layout.manual_layout_applied);
    assert!((layout.plot_x - layout.w * 0.1).abs() < 1.0);
    assert!((layout.plot_y - layout.h * 0.2).abs() < 1.0);
    assert!((layout.plot_w - layout.w * 0.5).abs() < 1.0);
    assert!((layout.plot_h - layout.h * 0.6).abs() < 1.0);
}

#[test]
fn manual_layout_outer_target_insets_the_resolved_rectangle() {
    let mut c = chart("clusteredColumn", &["A", "B"], &[&[10.0, 20.0]]);
    c.plot_area_manual_layout = Some(ChartManualLayout {
        x_mode: "factor".into(),
        y_mode: "factor".into(),
        w_mode: "factor".into(),
        h_mode: "factor".into(),
        layout_target: Some("outer".into()),
        x: 0.1,
        y: 0.1,
        w: Some(0.8),
        h: Some(0.8),
    });

    let layout = frame(&c);
    assert!(layout.manual_layout_applied);
    // ECMA-376 §21.2.2.89: the outer rectangle includes the tick labels, so the
    // data region is inset by the automatic gutters (ref layout.ts:793-813).
    assert!(layout.plot_x > layout.w * 0.1 + 1.0);
    assert!(layout.plot_y > layout.h * 0.1 + 1.0);
    assert!(layout.plot_x + layout.plot_w < layout.w * 0.9 - 1.0);
    assert!(layout.plot_y + layout.plot_h < layout.h * 0.9 - 1.0);
}

// ── Data-label positions ────────────────────────────────────────────────────

#[test]
fn authored_data_label_position_moves_the_label() {
    let mut c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    c.show_data_labels = true;
    c.data_label_position = Some("outEnd".into());
    let outside = compile(c.clone(), (W_PX * PX) as i64, (H_PX * PX) as i64);

    c.data_label_position = Some("inEnd".into());
    let inside = compile(c, (W_PX * PX) as i64, (H_PX * PX) as i64);

    let ty = |commands: &[VectorCommand]| -> f32 {
        commands
            .iter()
            .find_map(|cmd| match cmd {
                VectorCommand::DrawText { runs, .. } => runs
                    .iter()
                    // The data label is the small-font "10"; the value-axis tick
                    // carries the same text at the axis-label size.
                    .find(|r| r.text == "10" && r.font_size_pt < 8.0)
                    .and_then(|r| r.glyphs.first().map(|g| g.y)),
                _ => None,
            })
            .expect("data label run")
    };
    // ref data-label-layout.ts:140-190: `inEnd` sits inside the bar end.
    assert!(ty(&inside) > ty(&outside), "inEnd must sit below outEnd");
}

#[test]
fn frame_family_is_cartesian_for_the_default_chart_type() {
    let c = chart("clusteredColumn", &["A"], &[&[10.0]]);
    assert_eq!(detect_chart_family(&c), ChartFamily::Column);
}

