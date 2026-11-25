//! Chart-family dispatch parity tests.
//!
//! Reference: `/tmp/oooxml` @ 1f5c6e1d
//! - `packages/core/src/chart/plot-groups.ts:95-208` (`classicPlotDispatch`)
//! - `packages/core/src/chart/renderer.ts:318-327` ("Unsupported chart")
//! - `packages/core/src/chart/renderer.ts:354-368` (unknown legacy type)

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::charts::compile_chart;
use office_terminal_core::compiler::charts::types::{detect_chart_family, resolve_chart_family, ChartFamily};
use office_terminal_core::contracts::{FidelityClass, VectorCommand, VectorPaint, WarningEvent};
use ooxml_common::chart::{ChartModel, ChartPlotGroup, ChartSeries};
use pptx_model::ChartElement;

fn series(name: &str, values: &[f64]) -> ChartSeries {
    ChartSeries {
        name: name.into(),
        color: Some("4472C4".into()),
        values: values.iter().copied().map(Some).collect(),
        ..ChartSeries::default()
    }
}

/// A plottable two-series chart, so dispatch is reached instead of the
/// `chart_series` placeholder.
fn chart(chart_type: &str) -> ChartModel {
    ChartModel {
        chart_type: chart_type.into(),
        categories: vec!["Category 1".into(), "Category 2".into()],
        series: vec![series("Series 1", &[10.0, 20.0]), series("Series 2", &[5.0, 15.0])],
        ..ChartModel::default()
    }
}

fn group(kind: &str, series_start: usize, series_count: usize) -> ChartPlotGroup {
    ChartPlotGroup {
        kind: kind.into(),
        series_start,
        series_count,
        category_axis: "primary".into(),
        value_axis: "primary".into(),
        series_axis: "none".into(),
        axis_ids: None,
        grouping: None,
        bar_direction: None,
        scatter_style: None,
        radar_style: None,
        vary_colors: None,
        gap_width: None,
        overlap: None,
        bubble_scale: None,
        bubble_size_represents: None,
        show_negative_bubbles: None,
    }
}

fn compile(model: ChartModel) -> (Vec<VectorCommand>, Vec<WarningEvent>) {
    let (tx, rx) = unbounded();
    let elem = ChartElement {
        id: Some("chart1".into()),
        x: 1_000_000,
        y: 800_000,
        width: 5_000_000,
        height: 3_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: model,
    };
    let commands = compile_chart(&elem, &tx);
    drop(tx);
    (commands, rx.try_iter().collect())
}

fn text_runs(commands: &[VectorCommand]) -> Vec<&str> {
    commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawText { runs, .. } => Some(runs.as_slice()),
            _ => None,
        })
        .flatten()
        .map(|run| run.text.as_str())
        .collect()
}

/// Assert the reference fail-closed paint path: a light frame and exactly one
/// centred "Unsupported chart" label, with no series/axis/legend geometry.
fn assert_unsupported_placeholder(commands: &[VectorCommand]) {
    let texts = text_runs(commands);
    assert_eq!(
        texts.len(),
        1,
        "placeholder must emit exactly one text run, got {texts:?}"
    );
    assert!(
        texts[0].contains("Unsupported chart"),
        "placeholder label must contain 'Unsupported chart', got {:?}",
        texts[0]
    );

    let text_commands = commands
        .iter()
        .filter(|command| matches!(command, VectorCommand::DrawText { .. }))
        .count();
    assert_eq!(text_commands, 1, "no title/axis/legend text may be painted");

    let filled: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            VectorCommand::DrawPath {
                path,
                fill: Some(VectorPaint::Solid(color)),
                ..
            } => Some((path, *color)),
            _ => None,
        })
        .collect();
    assert_eq!(filled.len(), 1, "only the placeholder background rect may be filled");
    assert_eq!(filled[0].1, [245, 245, 245, 255], "placeholder background is light");
    assert!(
        commands
            .iter()
            .all(|command| !matches!(command, VectorCommand::DrawPath { stroke: Some(_), .. })),
        "no stroked series/axis geometry may be painted"
    );
    assert!(
        texts.iter().all(|text| *text != "Series 1" && *text != "Category 1"),
        "series and category labels must not be painted"
    );
}

fn warning(events: &[WarningEvent]) -> (&'static str, FidelityClass) {
    assert_eq!(events.len(), 1, "exactly one dispatch warning expected");
    match &events[0] {
        WarningEvent::FidelityDegraded { feature, class, .. } => (*feature, *class),
        other => panic!("expected FidelityDegraded, got {other:?}"),
    }
}

/// The families without a compiler must never fall back to columns.
#[test]
fn unsupported_families_paint_the_placeholder() {
    let cases = [
        ("scatter", ChartFamily::Scatter),
        ("radar", ChartFamily::Radar),
        ("stock", ChartFamily::Stock),
        ("surface", ChartFamily::Surface),
        // ChartEx/unknown layouts have no family renderer either.
        ("sunburst", ChartFamily::Other),
    ];

    for (chart_type, expected) in cases {
        let model = chart(chart_type);
        assert_eq!(
            detect_chart_family(&model),
            expected,
            "{chart_type} must resolve to {}",
            expected.label()
        );
        assert!(
            !expected.is_supported(),
            "{} must not be in the painted subset",
            expected.label()
        );

        let (commands, events) = compile(model);
        assert_unsupported_placeholder(&commands);
        let (feature, class) = warning(&events);
        assert_eq!(feature, "chart_family_unsupported");
        assert_eq!(class, FidelityClass::Placeholder);
    }
}

/// The warning is emitted once per family named in `cases` above. The family
/// identity is the resolved dispatch value (`ChartFamily::label`), because
/// `WarningEvent::FidelityDegraded::feature` is a static tag.
#[test]
fn unsupported_family_warning_is_emitted_with_the_family_identity() {
    for (chart_type, family) in [
        ("scatter", ChartFamily::Scatter),
        ("radar", ChartFamily::Radar),
        ("stock", ChartFamily::Stock),
        ("surface", ChartFamily::Surface),
        ("treemap", ChartFamily::Other),
    ] {
        let model = chart(chart_type);
        assert_eq!(
            resolve_chart_family(&model),
            family,
            "warning context for {}",
            family.label()
        );
        let (_, events) = compile(model);
        let (feature, class) = warning(&events);
        assert_eq!(feature, "chart_family_unsupported", "{}", family.label());
        assert_eq!(class, FidelityClass::Placeholder, "{}", family.label());
    }
}

#[test]
fn bubble_family_compiles_series_geometry() {
    let mut model = chart("bubble");
    model.series[0].bubble_sizes = Some(vec![Some(50.0), Some(100.0)]);
    let (commands, events) = compile(model);
    assert!(
        events.is_empty(),
        "supported bubble chart emits no placeholder warnings"
    );
    assert!(
        commands.iter().any(|c| matches!(c, VectorCommand::DrawPath { .. })),
        "bubble chart must emit vector paths"
    );
}

#[test]
fn supported_line_chart_still_compiles_series() {
    let mut model = chart("line");
    model.show_legend = true;
    model.legend_pos = Some("r".into());
    assert_eq!(resolve_chart_family(&model), ChartFamily::Line);

    let (commands, events) = compile(model);
    assert!(events.is_empty(), "line charts must not emit a dispatch warning");
    let texts = text_runs(&commands);
    assert!(
        texts.iter().all(|text| !text.contains("Unsupported chart")),
        "line chart must not take the placeholder path"
    );
    assert!(
        texts.contains(&"Series 1"),
        "series legend text must be painted, got {texts:?}"
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, VectorCommand::DrawPath { stroke: Some(_), .. })),
        "line geometry must be painted"
    );
}

/// Two or more plot groups with different families cannot be painted by the
/// single-family compilers (`ref plot-groups.ts:95-208`).
#[test]
fn mixed_family_plot_groups_paint_the_placeholder() {
    let mut model = chart("clusteredBar");
    model.plot_groups = Some(vec![group("bar", 0, 1), group("line", 1, 1)]);

    assert_eq!(resolve_chart_family(&model), ChartFamily::Other);
    let (commands, events) = compile(model);
    assert_unsupported_placeholder(&commands);
    assert_eq!(
        warning(&events),
        ("chart_family_unsupported", FidelityClass::Placeholder)
    );
}

/// Plot groups whose ranges do not form a contiguous cover of `series` are
/// invalid and fail closed (`ref plot-groups.ts:20-30`).
#[test]
fn invalid_plot_group_ranges_paint_the_placeholder() {
    let mut model = chart("clusteredBar");
    model.plot_groups = Some(vec![group("bar", 0, 1), group("line", 1, 2)]);

    assert_eq!(resolve_chart_family(&model), ChartFamily::Other);
    let (commands, events) = compile(model);
    assert_unsupported_placeholder(&commands);
    assert_eq!(
        warning(&events),
        ("chart_family_unsupported", FidelityClass::Placeholder)
    );
}

/// Empty-type rule: `Column` is the default only for an entirely empty model.
/// A populated model with no `chartType` has no dispatch information and takes
/// the unknown-family placeholder path (`ref renderer.ts:354-368`).
#[test]
fn empty_chart_type_defaults_to_column_only_for_an_empty_model() {
    let empty = ChartModel::default();
    assert_eq!(detect_chart_family(&empty), ChartFamily::Column);
    assert_eq!(resolve_chart_family(&empty), ChartFamily::Column);

    let populated = chart("");
    assert_eq!(detect_chart_family(&populated), ChartFamily::Other);
    assert_eq!(resolve_chart_family(&populated), ChartFamily::Other);

    let titled = ChartModel {
        title: Some("No type".into()),
        ..ChartModel::default()
    };
    assert_eq!(detect_chart_family(&titled), ChartFamily::Other);
}
