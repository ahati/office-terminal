//! Bubble and bubble3D chart compilation integration tests (Issue #17 / PRD 02d).

use crossbeam_channel::unbounded;
use office_terminal_core::compiler::charts::compile_chart;
use office_terminal_core::contracts::{
    VectorCommand, VectorPaint, WarningEvent,
};
use ooxml_common::chart::{ChartModel, ChartSeries};
use pptx_model::ChartElement;

fn compile(model: ChartModel) -> (Vec<VectorCommand>, Vec<WarningEvent>) {
    let (tx, rx) = unbounded();
    let elem = ChartElement {
        id: Some("chart_bubble".into()),
        x: 500_000,
        y: 500_000,
        width: 4_000_000,
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

#[test]
fn flat_bubble_chart_compiles_circle_paths_without_placeholder() {
    let model = ChartModel {
        chart_type: "bubble".into(),
        categories: vec!["1".into(), "2".into(), "3".into()],
        series: vec![ChartSeries {
            name: "Series 1".into(),
            color: Some("4472C4".into()),
            values: vec![Some(10.0), Some(20.0), Some(30.0)],
            bubble_sizes: Some(vec![Some(25.0), Some(50.0), Some(100.0)]),
            bubble_3d: Some(false),
            ..ChartSeries::default()
        }],
        bubble_scale: Some(100),
        ..ChartModel::default()
    };

    let (commands, warnings) = compile(model);
    assert!(warnings.is_empty(), "supported bubble chart emits no placeholder warnings");

    // Must emit circle paths (paths with cubic beziers)
    let path_count = commands
        .iter()
        .filter(|c| matches!(c, VectorCommand::DrawPath { .. }))
        .count();
    assert!(path_count >= 3, "must draw at least 3 bubble paths, got {path_count}");

    // Should NOT have PushClip for flat bubbles
    let push_clips = commands
        .iter()
        .filter(|c| matches!(c, VectorCommand::PushClip(_)))
        .count();
    assert_eq!(push_clips, 0, "flat bubbles do not need push clip layers");
}

#[test]
fn bubble_3d_chart_compiles_three_radial_gradient_layers_with_clip() {
    let model = ChartModel {
        chart_type: "bubble".into(),
        categories: vec!["1".into()],
        series: vec![ChartSeries {
            name: "Series 3D".into(),
            color: Some("4472C4".into()),
            values: vec![Some(10.0)],
            bubble_sizes: Some(vec![Some(100.0)]),
            bubble_3d: Some(true),
            ..ChartSeries::default()
        }],
        bubble_scale: Some(100),
        ..ChartModel::default()
    };

    let (commands, warnings) = compile(model);
    assert!(warnings.is_empty());

    // Must have PushClip and PopClip for the 3D lighting envelope
    let push_clips = commands
        .iter()
        .filter(|c| matches!(c, VectorCommand::PushClip(_)))
        .count();
    let pop_clips = commands
        .iter()
        .filter(|c| matches!(c, VectorCommand::PopClip))
        .count();
    assert_eq!(push_clips, 1, "bubble3D must push a clip for its circle");
    assert_eq!(pop_clips, 1, "bubble3D must pop the clip");

    // Must have exactly 3 radial gradient layers (diffuse, shade, lower rim)
    let radial_gradients: Vec<_> = commands
        .iter()
        .filter_map(|c| match c {
            VectorCommand::DrawPath { fill: Some(VectorPaint::RadialGradient(grad)), .. } => {
                Some(grad)
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        radial_gradients.len(),
        3,
        "bubble3D must emit exactly 3 radial gradient material layers"
    );

    // Stop counts: 4 (diffuse), 5 (shade), 6 (lower rim) matching reference spec
    let stop_counts: Vec<_> = radial_gradients.iter().map(|g| g.stops.len()).collect();
    assert_eq!(stop_counts, vec![4, 5, 6], "radial gradient stops must be [4, 5, 6]");
}

#[test]
fn bubble_scale_zero_draws_no_bubbles() {
    let model = ChartModel {
        chart_type: "bubble".into(),
        categories: vec!["1".into()],
        series: vec![ChartSeries {
            name: "Hidden Bubbles".into(),
            values: vec![Some(10.0)],
            bubble_sizes: Some(vec![Some(100.0)]),
            ..ChartSeries::default()
        }],
        bubble_scale: Some(0),
        ..ChartModel::default()
    };

    let (commands, _) = compile(model);
    // Background and axes may be drawn, but no series bubbles
    let bubble_fills = commands
        .iter()
        .filter(|c| match c {
            VectorCommand::DrawPath { fill: Some(VectorPaint::Solid(color)), .. } => {
                // Background is [255, 255, 255] or transparent, not series color
                *color != [255, 255, 255, 255]
            }
            _ => false,
        })
        .count();
    assert_eq!(bubble_fills, 0, "bubble scale 0 must suppress all bubbles");
}

#[test]
fn negative_bubbles_are_skipped_when_show_negative_is_false() {
    let model = ChartModel {
        chart_type: "bubble".into(),
        categories: vec!["1".into(), "2".into()],
        series: vec![ChartSeries {
            name: "Mixed Bubbles".into(),
            color: Some("4472C4".into()),
            values: vec![Some(10.0), Some(20.0)],
            bubble_sizes: Some(vec![Some(50.0), Some(-50.0)]),
            ..ChartSeries::default()
        }],
        show_negative_bubbles: Some(false),
        ..ChartModel::default()
    };

    let (commands, _) = compile(model);
    // Only 1 positive bubble should be drawn
    let bubble_circles = commands
        .iter()
        .filter(|c| match c {
            VectorCommand::DrawPath { fill: Some(VectorPaint::Solid(color)), .. } => {
                *color == [68, 114, 196, 255] // 4472C4
            }
            _ => false,
        })
        .count();
    assert_eq!(bubble_circles, 1, "negative bubble must be skipped when show_negative_bubbles=false");
}

#[test]
fn line_chart_honors_marker_symbols_and_none() {
    // 1. None symbol: no markers
    let model_none = ChartModel {
        chart_type: "line".into(),
        categories: vec!["A".into(), "B".into()],
        series: vec![ChartSeries {
            name: "Line".into(),
            values: vec![Some(1.0), Some(2.0)],
            marker_symbol: Some("none".into()),
            ..ChartSeries::default()
        }],
        ..ChartModel::default()
    };
    let (commands_none, _) = compile(model_none);
    // Exactly 1 polyline stroke, no markers
    let filled_markers = commands_none
        .iter()
        .filter(|c| matches!(c, VectorCommand::DrawPath { fill: Some(VectorPaint::Solid(_)), .. }))
        .count();
    assert_eq!(filled_markers, 0, "symbol 'none' must emit zero marker fills");

    // 2. Square symbol: emits square marker paths
    let model_square = ChartModel {
        chart_type: "line".into(),
        categories: vec!["A".into(), "B".into()],
        series: vec![ChartSeries {
            name: "Square Markers".into(),
            values: vec![Some(1.0), Some(2.0)],
            marker_symbol: Some("square".into()),
            marker_size: Some(8.0),
            ..ChartSeries::default()
        }],
        ..ChartModel::default()
    };
    let (commands_square, _) = compile(model_square);
    let marker_paths = commands_square
        .iter()
        .filter(|c| matches!(c, VectorCommand::DrawPath { fill: Some(VectorPaint::Solid(_)), .. }))
        .count();
    assert_eq!(marker_paths, 2, "square markers must emit 2 marker fills");
}
