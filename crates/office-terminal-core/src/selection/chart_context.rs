//! Bounded chart summary for the element-context projection.

use super::{normalize_text_limit, utf16_prefix, BoundedText};

fn format_value(value: Option<f64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// Bounded chart summary (`boundedChartContextText`, chart-context.ts).
pub fn bounded_chart_context_text(
    chart: &pptx_model::ChartModel,
    requested_max_text_characters: Option<usize>,
) -> BoundedText {
    let max_units = normalize_text_limit(requested_max_text_characters);
    let mut chunks: Vec<String> = Vec::new();
    let mut length = 0usize;
    let mut truncated = false;
    let mut has_part = false;
    let mut append = |value: &str, begins_part: bool, chunks: &mut Vec<String>| -> bool {
        if begins_part && has_part {
            if length >= max_units {
                truncated = true;
                return false;
            }
            chunks.push("\n".into());
            length += 1;
        }
        has_part = true;
        let allowed = max_units.saturating_sub(length);
        let (chunk, units) = utf16_prefix(value, allowed);
        length += units;
        let complete = chunk.len() == value.len();
        chunks.push(chunk);
        if !complete {
            truncated = true;
            return false;
        }
        true
    };

    if !append(&format!("Chart type: {}", chart.chart_type), true, &mut chunks) {
        return BoundedText {
            text: Some(chunks.concat()),
            truncated,
            text_characters: length,
            max_text_characters: max_units,
        };
    }
    if let Some(title) = chart.title.as_deref() {
        if !append(&format!("Title: {title}"), true, &mut chunks) {
            return BoundedText {
                text: Some(chunks.concat()),
                truncated,
                text_characters: length,
                max_text_characters: max_units,
            };
        }
    }
    if !chart.categories.is_empty() {
        if !append("Categories: ", true, &mut chunks) {
            return BoundedText {
                text: Some(chunks.concat()),
                truncated,
                text_characters: length,
                max_text_characters: max_units,
            };
        }
        for (index, category) in chart.categories.iter().enumerate() {
            let value = format!("{}{}", if index == 0 { "" } else { ", " }, category);
            if !append(&value, false, &mut chunks) {
                return BoundedText {
                    text: Some(chunks.concat()),
                    truncated,
                    text_characters: length,
                    max_text_characters: max_units,
                };
            }
        }
    }
    for series in &chart.series {
        let name = if series.name.is_empty() {
            "(unnamed)"
        } else {
            &series.name
        };
        if !append(&format!("Series {name}: "), true, &mut chunks) {
            break;
        }
        for (index, value) in series.values.iter().enumerate() {
            let value = format!("{}{}", if index == 0 { "" } else { ", " }, format_value(*value));
            if !append(&value, false, &mut chunks) {
                return BoundedText {
                    text: Some(chunks.concat()),
                    truncated,
                    text_characters: length,
                    max_text_characters: max_units,
                };
            }
        }
    }
    BoundedText {
        text: Some(chunks.concat()),
        truncated,
        text_characters: length,
        max_text_characters: max_units,
    }
}
