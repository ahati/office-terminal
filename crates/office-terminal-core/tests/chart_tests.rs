#![recursion_limit = "256"]

use crossbeam_channel::unbounded;
use office_terminal_core::contracts::{VectorCommand, WarningEvent};
use office_terminal_core::{DefaultSlideCompiler, SlideCompiler};
use ooxml_common::chart::{ChartModel, ChartSeries};
use pptx_model::{ChartElement, Slide, SlideElement};

fn make_slide(elem: SlideElement) -> Slide {
    Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: None,
        background: None,
        elements: vec![elem],
        element_sources: vec![],
        notes: None,
        comments: vec![],
        hidden: false,
        parse_error: None,
    }
}

#[test]
fn test_column_chart_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let chart_elem = ChartElement {
        id: Some("chart1".into()),
        x: 1_000_000,
        y: 1_000_000,
        width: 6_000_000,
        height: 4_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "clusteredColumn".into(),
            title: Some("Quarterly Revenue".into()),
            categories: vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
            series: vec![
                ChartSeries {
                    name: "2024".into(),
                    color: Some("4F81BD".into()),
                    values: vec![Some(100.0), Some(150.0), Some(200.0), Some(250.0)],
                    ..ChartSeries::default()
                },
                ChartSeries {
                    name: "2025".into(),
                    color: Some("C0504D".into()),
                    values: vec![Some(120.0), Some(180.0), Some(220.0), Some(300.0)],
                    ..ChartSeries::default()
                },
            ],
            show_legend: true,
            legend_pos: Some("r".into()),
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(chart_elem));

    let vs = compiler
        .compile(&slide, 9_144_000, 6_858_000, &tx)
        .expect("compile slide with chart");

    assert!(!vs.commands.is_empty());

    // Verify presence of title text
    let has_title = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawText { runs, .. } => runs.iter().any(|r| r.text.contains("Quarterly Revenue")),
        _ => false,
    });
    assert!(has_title, "Chart title must be emitted");

    // Verify category labels on axis
    let has_categories = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawText { runs, .. } => runs.iter().any(|r| r.text == "Q1"),
        _ => false,
    });
    assert!(has_categories, "Category labels must be emitted");

    // Verify legend items
    let has_legend_2024 = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawText { runs, .. } => runs.iter().any(|r| r.text.contains("2024")),
        _ => false,
    });
    assert!(has_legend_2024, "Legend entry '2024' must be emitted");

    // Verify bar rectangle DrawPaths exist
    let draw_paths_count = vs
        .commands
        .iter()
        .filter(|cmd| matches!(cmd, VectorCommand::DrawPath { fill: Some(_), .. }))
        .count();
    // 4 categories * 2 series = 8 bars (+ swatches)
    assert!(draw_paths_count >= 8, "Expected at least 8 filled bar paths");
}

#[test]
fn test_horizontal_bar_chart_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let chart_elem = ChartElement {
        id: Some("chart_h".into()),
        x: 500_000,
        y: 500_000,
        width: 5_000_000,
        height: 3_500_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "clusteredBarH".into(),
            title: Some("Project Hours".into()),
            categories: vec!["Dev".into(), "Design".into(), "QA".into()],
            series: vec![ChartSeries {
                name: "Hours".into(),
                color: Some("5B9BD5".into()),
                values: vec![Some(80.0), Some(45.0), Some(30.0)],
                ..ChartSeries::default()
            }],
            show_legend: false,
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(chart_elem));

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();

    let has_dev_cat = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawText { runs, .. } => runs.iter().any(|r| r.text == "Dev"),
        _ => false,
    });
    assert!(has_dev_cat, "Horizontal bar category label 'Dev' must be emitted");
}

#[test]
fn test_line_chart_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let chart_elem = ChartElement {
        id: Some("line_chart".into()),
        x: 1_000_000,
        y: 1_000_000,
        width: 6_000_000,
        height: 4_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "line".into(),
            title: Some("Trends Over Time".into()),
            categories: vec!["Jan".into(), "Feb".into(), "Mar".into(), "Apr".into()],
            series: vec![ChartSeries {
                name: "Active Users".into(),
                line_color: Some("ED7D31".into()),
                values: vec![Some(10.0), Some(25.0), Some(40.0), Some(60.0)],
                show_marker: Some(true),
                ..ChartSeries::default()
            }],
            show_legend: true,
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(chart_elem));

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();

    // Verify stroke paths (polyline line and grid)
    let stroke_paths = vs
        .commands
        .iter()
        .filter(|cmd| matches!(cmd, VectorCommand::DrawPath { stroke: Some(_), .. }))
        .count();
    assert!(stroke_paths >= 2, "Expected polyline stroke paths");

    // Verify markers (filled paths with stroke)
    let markers = vs
        .commands
        .iter()
        .filter(|cmd| matches!(cmd, VectorCommand::DrawPath { fill: Some(_), stroke: Some(_), .. }))
        .count();
    assert_eq!(markers, 4, "Expected 4 data point markers");
}

#[test]
fn test_pie_and_doughnut_chart_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    // 1. Pie chart
    let pie_elem = ChartElement {
        id: Some("pie1".into()),
        x: 500_000,
        y: 500_000,
        width: 4_000_000,
        height: 4_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "pie".into(),
            title: Some("Market Share".into()),
            categories: vec!["Chrome".into(), "Safari".into(), "Firefox".into()],
            series: vec![ChartSeries {
                name: "Share".into(),
                values: vec![Some(65.0), Some(25.0), Some(10.0)],
                ..ChartSeries::default()
            }],
            show_legend: true,
            legend_pos: Some("b".into()),
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(pie_elem));

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();

    // 3 pie slices
    let slice_count = vs
        .commands
        .iter()
        .filter(|cmd| matches!(cmd, VectorCommand::DrawPath { fill: Some(_), stroke: Some(_), .. }))
        .count();
    assert_eq!(slice_count, 3, "Expected 3 pie sector paths");

    // 2. Doughnut chart
    let doughnut_elem = ChartElement {
        id: Some("doughnut1".into()),
        x: 500_000,
        y: 500_000,
        width: 4_000_000,
        height: 4_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "doughnut".into(),
            title: Some("Storage Breakdown".into()),
            categories: vec!["Used".into(), "Free".into()],
            hole_size: Some(60),
            series: vec![ChartSeries {
                name: "Storage".into(),
                values: vec![Some(70.0), Some(30.0)],
                ..ChartSeries::default()
            }],
            show_legend: false,
            ..ChartModel::default()
        },
    };

    let slide2 = make_slide(SlideElement::Chart(doughnut_elem));

    let vs2 = compiler.compile(&slide2, 9_144_000, 6_858_000, &tx).unwrap();
    let doughnut_slices = vs2
        .commands
        .iter()
        .filter(|cmd| matches!(cmd, VectorCommand::DrawPath { fill: Some(_), stroke: Some(_), .. }))
        .count();
    assert_eq!(doughnut_slices, 2, "Expected 2 doughnut sector paths");
}

#[test]
fn test_area_chart_compilation() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let chart_elem = ChartElement {
        id: Some("area1".into()),
        x: 1_000_000,
        y: 1_000_000,
        width: 6_000_000,
        height: 4_000_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "area".into(),
            title: Some("Bandwidth Usage".into()),
            categories: vec!["Mon".into(), "Tue".into(), "Wed".into()],
            series: vec![ChartSeries {
                name: "GB".into(),
                color: Some("4472C4".into()),
                values: vec![Some(20.0), Some(35.0), Some(50.0)],
                ..ChartSeries::default()
            }],
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(chart_elem));

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();
    let has_polygon = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawPath { fill: Some(_), path, .. } => path.segments.iter().any(|s| matches!(s, office_terminal_core::contracts::PathSegment::Close)),
        _ => false,
    });
    assert!(has_polygon, "Expected closed area polygon path");
}

#[test]
fn test_chart_fallback_when_empty_series() {
    let (tx, rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let chart_elem = ChartElement {
        id: Some("empty_chart".into()),
        x: 100_000,
        y: 100_000,
        width: 2_000_000,
        height: 1_500_000,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "bar".into(),
            title: Some("Empty Chart".into()),
            series: vec![],
            ..ChartModel::default()
        },
    };

    let slide = make_slide(SlideElement::Chart(chart_elem));

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();

    // Must emit placeholder box
    let has_dashed_frame = vs.commands.iter().any(|cmd| match cmd {
        VectorCommand::DrawPath { stroke: Some(s), .. } => s.dash_segments.is_some(),
        _ => false,
    });
    assert!(has_dashed_frame, "Expected dashed frame placeholder");

    // Must emit warning event
    let warning = rx.try_recv().expect("Expected WarningEvent");
    assert!(matches!(warning, WarningEvent::FidelityDegraded { .. }));
}

#[test]
fn test_smartart_text_rect_layout() {
    let (tx, _rx) = unbounded();
    let compiler = DefaultSlideCompiler::new();

    let shape_str = r#"{
        "type": "shape",
        "id": "smartart_node",
        "x": 1000000,
        "y": 1000000,
        "width": 3000000,
        "height": 2000000,
        "rotation": 0.0,
        "flipH": false,
        "flipV": false,
        "geometry": "roundRect",
        "textRect": {
            "x": 1200000,
            "y": 1400000,
            "width": 2600000,
            "height": 1200000
        },
        "textBody": {
            "verticalAnchor": "ctr",
            "lIns": 0,
            "rIns": 0,
            "tIns": 0,
            "bIns": 0,
            "wrap": "square",
            "vert": "horz",
            "autoFit": "none",
            "fontScale": 1.0,
            "numCol": 1,
            "spcCol": 0,
            "rtlCol": false,
            "spcFirstLastPara": false,
            "anchorCtr": false,
            "paragraphs": [
                {
                    "alignment": "ctr",
                    "bullet": { "type": "none" },
                    "marL": 0,
                    "marR": 0,
                    "indent": 0,
                    "lvl": 0,
                    "tabStops": [],
                    "rtl": false,
                    "eaLnBrk": true,
                    "runs": [
                        {
                            "type": "text",
                            "text": "SmartArt Concept",
                            "bold": false,
                            "italic": false,
                            "underline": false,
                            "strikeDouble": false,
                            "hyperlinkUsesTextFill": false,
                            "strikethrough": false,
                            "fontSize": 18.0,
                            "color": "000000"
                        }
                    ]
                }
            ]
        }
    }"#;

    let shape_elem: SlideElement = serde_json::from_str(shape_str).unwrap();
    let slide = make_slide(shape_elem);

    let vs = compiler.compile(&slide, 9_144_000, 6_858_000, &tx).unwrap();

    // Find the text command and check its translation matches text_rect
    let text_cmd = vs.commands.iter().find(|cmd| matches!(cmd, VectorCommand::DrawText { .. }));
    assert!(text_cmd.is_some(), "Expected DrawText command");

    if let Some(VectorCommand::DrawText { transform, runs }) = text_cmd {
        assert_eq!(runs[0].text, "SmartArt Concept");
        // Verify transform uses the SmartArt text_rect coordinates (1,200,000, 1,400,000)
        assert_eq!(transform.tx, 1_200_000.0);
        assert_eq!(transform.ty, 1_400_000.0);
    }
}
