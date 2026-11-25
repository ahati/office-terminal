//! Element selection contexts (`element-selection.test.ts`, V3).

use office_terminal_core::selection::{
    bounded_chart_context_text, find_element_bounds_by_ids, hit_test_slide_context, hit_test_slide_element,
    limit_element_context, ElementContextOptions, SlidePoint,
};
use ooxml_common::chart::ChartSeries;
use pptx_model::{ChartElement, ChartModel, Slide, SlideElement, SlideElementSource};

fn text_body(text: &str) -> serde_json::Value {
    serde_json::json!({
        "verticalAnchor": "t",
        "paragraphs": [{
            "alignment": "l", "marL": 0, "marR": 0, "indent": 0,
            "spaceBefore": null, "spaceAfter": null, "spaceLine": null,
            "lvl": 0, "bullet": { "type": "inherit" },
            "defFontSize": null, "defColor": null, "defBold": null,
            "defItalic": null, "defFontFamily": null, "tabStops": [],
            "defTabSz": null, "rtl": false, "eaLnBrk": true, "fontAlgn": null,
            "runs": [{
                "type": "text", "text": text, "bold": null, "italic": null,
                "underline": false, "strikethrough": false, "strikeDouble": false,
                "hyperlinkUsesTextFill": false, "fieldType": null,
                "fontSize": 20, "color": "000000", "fontFamily": "Arial"
            }]
        }],
        "defaultFontSize": null, "defaultBold": null, "defaultItalic": null,
        "lIns": 0, "rIns": 0, "tIns": 0, "bIns": 0,
        "wrap": "square", "vert": "horz", "autoFit": "none"
    })
}

#[allow(clippy::too_many_arguments)]
fn shape(
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    rotation: f64,
    geometry: &str,
    id: &str,
    name: &str,
    body: Option<serde_json::Value>,
) -> SlideElement {
    let value = serde_json::json!({
        "type": "shape", "x": x, "y": y, "width": width, "height": height,
        "rotation": rotation, "flipH": false, "flipV": false,
        "geometry": geometry, "fill": null, "stroke": null,
        "textBody": body, "defaultTextColor": null, "custGeom": null,
        "adj": null, "adj2": null, "adj3": null, "adj4": null,
        "adj5": null, "adj6": null, "adj7": null, "adj8": null,
        "shadow": null, "innerShadow": null, "glow": null,
        "softEdge": null, "reflection": null, "id": id, "name": name,
        "hyperlink": null, "hyperlinkAction": null,
        "placeholderType": null, "placeholderIdx": null,
        "textRect": null, "scene3d": null, "sp3d": null
    });
    serde_json::from_value(value).expect("shape")
}

fn slide(elements: Vec<SlideElement>) -> Slide {
    Slide {
        theme: None,
        index: 0,
        slide_number: 1,
        part_name: Some("ppt/slides/slide1.xml".into()),
        background: None,
        element_sources: elements
            .iter()
            .enumerate()
            .map(|(index, _)| SlideElementSource {
                origin: if index == 0 {
                    pptx_model::SlideElementOrigin::Master
                } else {
                    pptx_model::SlideElementOrigin::Slide
                },
            })
            .collect(),
        elements,
        notes: None,
        comments: Vec::new(),
        hidden: false,
        parse_error: None,
    }
}

#[test]
fn topmost_element_context_reports_identity_and_origin() {
    let slide = slide(vec![
        shape(0, 0, 100, 50, 0.0, "rect", "1", "Back", None),
        shape(0, 0, 100, 50, 0.0, "roundRect", "2", "Front", None),
    ]);
    let context = hit_test_slide_context(
        0,
        &slide,
        SlidePoint { x: 20.0, y: 20.0 },
        ElementContextOptions::default(),
    )
    .expect("valid hit test")
    .expect("element hit");
    assert_eq!(context.format, "pptx");
    assert_eq!(context.kind, "element");
    assert_eq!(context.element_index, 1);
    assert_eq!(context.origin, "slide");
    assert_eq!(context.element_type, "shape");
    assert_eq!(context.shape_id.as_deref(), Some("2"));
    assert_eq!(context.name.as_deref(), Some("Front"));
    assert_eq!(context.geometry.as_deref(), Some("roundRect"));
    assert_eq!(context.bounds.width, 100.0);
    assert_eq!(context.text_characters, 0);
    assert!(!context.truncated);
}

#[test]
fn rotation_inversion_and_line_tolerance() {
    let rotated = shape(25, 0, 50, 100, 90.0, "rect", "1", "Rotated", None);
    assert!(hit_test_slide_element(&rotated, SlidePoint { x: 0.0, y: 25.0 }, 0.0));

    let line = shape(0, 0, 100, 0, 0.0, "line", "2", "Line", None);
    assert!(!hit_test_slide_element(&line, SlidePoint { x: 50.0, y: 4.0 }, 3.0));
    assert!(hit_test_slide_element(&line, SlidePoint { x: 50.0, y: 4.0 }, 5.0));
}

#[test]
fn bounded_element_text_never_splits_unicode() {
    let shape = shape(0, 0, 100, 50, 0.0, "rect", "1", "Text", Some(text_body("\u{1F600}x")));
    let slide = slide(vec![shape]);
    let point = SlidePoint { x: 10.0, y: 10.0 };
    let empty = hit_test_slide_context(
        0,
        &slide,
        point,
        ElementContextOptions {
            tolerance: 0.0,
            max_text_characters: Some(1),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(empty.text.as_deref(), Some(""));
    assert!(empty.truncated);

    let emoji = hit_test_slide_context(
        0,
        &slide,
        point,
        ElementContextOptions {
            tolerance: 0.0,
            max_text_characters: Some(2),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(emoji.text.as_deref(), Some("\u{1F600}"));
    assert!(emoji.truncated);

    let limited = limit_element_context(emoji, 1).unwrap();
    assert_eq!(limited.text.as_deref(), Some(""));
    assert_eq!(limited.text_characters, 0);
    assert_eq!(limited.max_text_characters, 1);
}

#[test]
fn chart_labels_stream_into_bounded_context_text() {
    let chart = ChartElement {
        id: Some("7".into()),
        x: 0,
        y: 0,
        width: 100,
        height: 50,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        chart: ChartModel {
            chart_type: "bar".into(),
            title: Some("Revenue".into()),
            categories: vec!["Q1".into(), "Q2".into()],
            series: vec![ChartSeries {
                name: "FY26".into(),
                values: vec![Some(10.0), Some(12.0)],
                ..ChartSeries::default()
            }],
            ..ChartModel::default()
        },
    };
    let summary = bounded_chart_context_text(&chart.chart, None);
    assert_eq!(
        summary.text.as_deref(),
        Some("Chart type: bar\nTitle: Revenue\nCategories: Q1, Q2\nSeries FY26: 10, 12")
    );
    assert!(!summary.truncated);
}

#[test]
fn bounds_lookup_prefers_slide_origin_over_master() {
    let slide = slide(vec![
        shape(0, 0, 10, 10, 0.0, "rect", "9", "Master copy", None),
        shape(20, 20, 10, 10, 0.0, "rect", "9", "Slide copy", None),
    ]);
    let entries = find_element_bounds_by_ids(&slide, &["9".into(), "missing".into()]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].element_index, 1);
    assert_eq!(entries[0].origin, "slide");
    assert_eq!(entries[0].bounds.x, 20.0);
}
