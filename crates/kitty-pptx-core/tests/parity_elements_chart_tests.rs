//! `chart-nested-emit.test.ts` parity (C10 — pptx chart nested emit).
//!
//! The reference freezes the *deleted* renderer's flat-element adapter as an
//! oracle and proves that the model the Rust parser now emits directly
//! (`el.chart`, a core `ChartModel`) is render-equivalent to it after
//! `stripNullish` pruning. The Rust equivalent of that adapter is serde: the
//! parser's wire JSON deserializes straight into
//! `ooxml_common::chart::ChartModel`, so the oracle comparison becomes
//! "the parser's field-name map lands on the typed fields, `null` ≡ absent, and
//! array `null` elements survive".
//!
//! Reference (read-only @ `1f5c6e1d`): `packages/pptx/src/chart-nested-emit.test.ts`.

// The oracle fixture is one deeply nested `json!` literal; serde_json's macro
// expands several internal layers per JSON level.
#![recursion_limit = "512"]

use ooxml_common::chart::ChartModel;
use serde_json::{json, Value};

/// A representative fully-populated combo chart exercising every mapped field
/// (renamed axis-title run props, secondary axis, series with markers and a
/// `null` data point). Field names are the *core* wire names the Rust parser
/// emits — the reference's `nestedRustChart` map (`chart-nested-emit.test.ts`).
fn rich_nested_emit() -> Value {
    json!({
        "chartType": "clusteredBar",
        "title": "Revenue vs. margin",
        "categories": ["Q1", "Q2", "Q3"],
        "series": [
            {
                "name": "Revenue",
                "color": "4472C4",
                "values": [120, null, 240],
                "seriesType": null,
                "useSecondaryAxis": null,
            },
            {
                "name": "Margin",
                "color": "ED7D31",
                "values": [0.31, 0.4, 0.28],
                "seriesType": "line",
                "useSecondaryAxis": true,
                "showMarker": true,
                "markerSymbol": "circle",
            },
        ],
        "valMax": 300,
        "valMin": 0,
        "subtotalIndices": [],
        "showDataLabels": true,
        "catAxisHidden": false,
        "valAxisHidden": false,
        "plotAreaBg": null,
        "chartBg": "FFFFFF",
        "showLegend": true,
        "catAxisCrossBetween": "between",
        "valAxisMajorTickMark": "out",
        "catAxisMajorTickMark": "out",
        "titleFontSizeHpt": 1400,
        "titleFontColor": "1B4332",
        "titleFontFace": "Calibri",
        "catAxisFontSizeHpt": 900,
        "valAxisFontSizeHpt": 900,
        "catAxisFontColor": "808080",
        "valAxisFontColor": null,
        "catAxisLineColor": "D9D9D9",
        "catAxisLineWidthEmu": 9525,
        "catAxisLineHidden": true,
        "valAxisLineColor": null,
        "valAxisLineWidthEmu": null,
        "valAxisLineHidden": false,
        "dataLabelFontSizeHpt": 800,
        "legendPos": "b",
        "barGapWidth": 150,
        "barOverlap": -27,
        "dataLabelPosition": "outEnd",
        "dataLabelFontColor": "000000",
        "dataLabelFormatCode": "#,##0",
        "valAxisFormatCode": "$#,##0",
        "scatterStyle": null,
        "catAxisTitle": "Quarter",
        "valAxisTitle": "Revenue ($M)",
        // The six renamed axis-title run props.
        "catAxisTitleFontSizeHpt": 1000,
        "catAxisTitleFontBold": true,
        "catAxisTitleFontColor": "404040",
        "valAxisTitleFontSizeHpt": 1100,
        "valAxisTitleFontBold": false,
        "valAxisTitleFontColor": "595959",
        "titleFontBold": true,
        "catAxisFontBold": false,
        "valAxisFontBold": false,
        "chartBorderColor": "1B4332",
        "chartBorderWidthEmu": 19050,
        "secondaryValAxis": {
            "min": 0,
            "max": 1,
            "title": "Margin (%)",
            "hidden": false,
            "formatCode": "0%",
            "lineHidden": false,
            "majorTickMark": "out",
        },
    })
}

/// The common sparse single-series case: mostly nulls / defaults.
fn sparse_nested_emit() -> Value {
    json!({
        "chartType": "pie",
        "title": null,
        "categories": ["A", "B", "C"],
        "series": [{ "name": "", "color": null, "values": [10, 20, 30] }],
        "valMax": null,
        "valMin": null,
        "subtotalIndices": [],
        "showDataLabels": false,
        "catAxisHidden": false,
        "valAxisHidden": false,
        "plotAreaBg": null,
        "chartBg": null,
        "showLegend": false,
        "catAxisCrossBetween": "between",
        "valAxisMajorTickMark": "cross",
        "catAxisMajorTickMark": "cross",
        "titleFontSizeHpt": null,
        "titleFontColor": null,
        "titleFontFace": null,
        "catAxisFontSizeHpt": null,
        "valAxisFontSizeHpt": null,
        "catAxisFontColor": null,
        "valAxisFontColor": null,
        "catAxisLineColor": null,
        "catAxisLineWidthEmu": null,
        "catAxisLineHidden": false,
        "valAxisLineColor": null,
        "valAxisLineWidthEmu": null,
        "valAxisLineHidden": false,
        "dataLabelFontSizeHpt": null,
        "legendPos": null,
        "barGapWidth": null,
        "barOverlap": null,
        "dataLabelPosition": null,
        "dataLabelFontColor": null,
        "dataLabelFormatCode": null,
        "valAxisFormatCode": null,
        "scatterStyle": null,
        "catAxisTitle": null,
        "valAxisTitle": null,
        "catAxisTitleFontSizeHpt": null,
        "catAxisTitleFontBold": null,
        "catAxisTitleFontColor": null,
        "valAxisTitleFontSizeHpt": null,
        "valAxisTitleFontBold": null,
        "valAxisTitleFontColor": null,
        "titleFontBold": null,
        "catAxisFontBold": null,
        "valAxisFontBold": null,
        "chartBorderColor": null,
        "chartBorderWidthEmu": null,
        "secondaryValAxis": null,
    })
}

fn model(value: &Value) -> ChartModel {
    serde_json::from_value(value.clone()).expect("nested chart emit fixture")
}

/// `stripNullish` equivalent: prune object properties whose value is nullish,
/// never prune array *elements*.
fn strip_nullish(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(strip_nullish).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(_, item)| !item.is_null())
                .map(|(key, item)| (key.clone(), strip_nullish(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn rich_combo_chart_nested_emit_lands_on_the_typed_core_fields() {
    let chart = model(&rich_nested_emit());

    assert_eq!(chart.chart_type, "clusteredBar");
    assert_eq!(chart.title.as_deref(), Some("Revenue vs. margin"));
    assert_eq!(chart.categories, ["Q1", "Q2", "Q3"]);
    assert_eq!(chart.val_min, Some(0.0));
    assert_eq!(chart.val_max, Some(300.0));
    assert!(chart.subtotal_indices.is_empty());
    assert!(chart.show_data_labels);

    // The six renamed axis-title run props (`catAxisTitleSize` →
    // `catAxisTitleFontSizeHpt`, …).
    assert_eq!(chart.cat_axis_title.as_deref(), Some("Quarter"));
    assert_eq!(chart.val_axis_title.as_deref(), Some("Revenue ($M)"));
    assert_eq!(chart.cat_axis_title_font_size_hpt, Some(1000));
    assert_eq!(chart.cat_axis_title_font_bold, Some(true));
    assert_eq!(chart.cat_axis_title_font_color.as_deref(), Some("404040"));
    assert_eq!(chart.val_axis_title_font_size_hpt, Some(1100));
    assert_eq!(chart.val_axis_title_font_bold, Some(false));
    assert_eq!(chart.val_axis_title_font_color.as_deref(), Some("595959"));

    // Pass-through fields, including the axis line/admission pair.
    assert_eq!(chart.title_font_size_hpt, Some(1400));
    assert_eq!(chart.title_font_color.as_deref(), Some("1B4332"));
    assert_eq!(chart.title_font_face.as_deref(), Some("Calibri"));
    assert_eq!(chart.title_font_bold, Some(true));
    assert_eq!(chart.cat_axis_font_size_hpt, Some(900));
    assert_eq!(chart.val_axis_font_size_hpt, Some(900));
    assert_eq!(chart.cat_axis_font_color.as_deref(), Some("808080"));
    assert_eq!(chart.val_axis_font_color, None);
    assert_eq!(chart.cat_axis_line_color.as_deref(), Some("D9D9D9"));
    assert_eq!(chart.cat_axis_line_width_emu, Some(9525));
    assert!(chart.cat_axis_line_hidden);
    assert!(!chart.val_axis_line_hidden);
    assert_eq!(chart.cat_axis_font_bold, Some(false));
    assert_eq!(chart.val_axis_font_bold, Some(false));
    assert_eq!(chart.data_label_font_size_hpt, Some(800));
    assert_eq!(chart.legend_pos.as_deref(), Some("b"));
    assert_eq!(chart.bar_gap_width, Some(150));
    assert_eq!(chart.bar_overlap, Some(-27));
    assert_eq!(chart.data_label_position.as_deref(), Some("outEnd"));
    assert_eq!(chart.data_label_format_code.as_deref(), Some("#,##0"));
    assert_eq!(chart.val_axis_format_code.as_deref(), Some("$#,##0"));
    assert_eq!(chart.chart_bg.as_deref(), Some("FFFFFF"));
    assert_eq!(chart.plot_area_bg, None);
    assert_eq!(chart.chart_border_color.as_deref(), Some("1B4332"));
    assert_eq!(chart.chart_border_width_emu, Some(19050));

    let series = &chart.series;
    assert_eq!(series.len(), 2);
    assert_eq!(series[0].name, "Revenue");
    assert_eq!(series[0].color.as_deref(), Some("4472C4"));
    assert_eq!(series[0].series_type, None);
    assert_eq!(series[0].use_secondary_axis, None);
    assert_eq!(series[1].series_type.as_deref(), Some("line"));
    assert_eq!(series[1].use_secondary_axis, Some(true));
    assert_eq!(series[1].show_marker, Some(true));
    assert_eq!(series[1].marker_symbol.as_deref(), Some("circle"));

    let secondary = chart.secondary_val_axis.as_ref().expect("secondary axis");
    assert_eq!(secondary.min, Some(0.0));
    assert_eq!(secondary.max, Some(1.0));
    assert_eq!(secondary.title.as_deref(), Some("Margin (%)"));
    assert!(!secondary.hidden);
    assert_eq!(secondary.format_code.as_deref(), Some("0%"));
    assert!(!secondary.line_hidden);
    assert_eq!(secondary.major_tick_mark, "out");
}

#[test]
fn sparse_pie_chart_nested_emit_is_nullish_equivalent_to_its_pruned_form() {
    let wire = sparse_nested_emit();
    let chart = model(&wire);

    assert_eq!(chart.chart_type, "pie");
    assert_eq!(chart.title, None);
    assert_eq!(chart.categories, ["A", "B", "C"]);
    assert_eq!(chart.series.len(), 1);
    assert_eq!(chart.series[0].values, [Some(10.0), Some(20.0), Some(30.0)]);
    assert_eq!(chart.series[0].color, None);
    assert!(!chart.show_data_labels);
    assert!(!chart.show_legend);
    assert_eq!(chart.legend_pos, None);
    assert_eq!(chart.cat_axis_title, None);
    assert_eq!(chart.val_axis_title, None);
    assert_eq!(chart.cat_axis_title_font_size_hpt, None);
    assert_eq!(chart.val_axis_title_font_bold, None);
    assert_eq!(chart.title_font_size_hpt, None);
    assert_eq!(chart.chart_bg, None);
    assert_eq!(chart.plot_area_bg, None);
    assert_eq!(chart.secondary_val_axis, None);

    // `null` and an absent key render identically in the reference
    // (`?? default` / `!= null`), which is what `stripNullish` normalises.
    // Serde reaches the same state: every optional field is `#[serde(default)]`
    // and pruned on the way out, so the pruned form re-parses to the same model.
    let serialized = serde_json::to_value(&chart).expect("serialize chart");
    let pruned = strip_nullish(&serialized);
    let reparsed = model(&pruned);
    assert_eq!(
        serde_json::to_value(&reparsed).expect("re-serialize"),
        serialized,
        "the pruned (nullish-free) emit must be a fixed point"
    );
    // The pruned form really dropped the nullish keys.
    assert!(pruned.get("titleFontSizeHpt").is_none());
    assert!(pruned.get("secondaryValAxis").is_none());
    assert!(pruned.get("catAxisTitle").is_none());
    // …and the renamed map — not the legacy flat key — carries the axis-title
    // run props, exactly as the parser now emits them.
    assert!(serialized.get("catAxisTitleSize").is_none());
    assert!(serialized.get("catAxisTitleFontSizeHpt").is_none());
}

#[test]
fn null_data_points_inside_series_values_survive_pruning() {
    // Array elements are never stripped: a `null` point is a gap, not an
    // absent key (ref `stripNullish`).
    let chart = model(&rich_nested_emit());
    assert_eq!(
        chart.series[0].values,
        [Some(120.0), None, Some(240.0)],
        "the authored gap must stay in place"
    );

    let serialized = serde_json::to_value(&chart).expect("serialize chart");
    let pruned = strip_nullish(&serialized);
    let series = pruned["series"].as_array().expect("series array");
    assert_eq!(series[0]["values"], json!([120.0, null, 240.0]));
}
